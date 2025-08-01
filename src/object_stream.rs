use crate::parser::{self, ParserInput};
use crate::{dictionary, Document, Error, Object, ObjectId, Result, Stream};
use std::collections::{BTreeMap, HashSet};
use std::num::TryFromIntError;
use std::str::FromStr;

use log::warn;
#[cfg(feature = "rayon")]
use rayon::prelude::*;

#[derive(Debug)]
pub struct ObjectStream {
    pub objects: BTreeMap<ObjectId, Object>,
    max_objects: usize,
    compression_level: u32,
}

#[derive(Debug, Clone)]
pub struct ObjectStreamBuilder {
    max_objects: usize,
    compression_level: u32,
}

#[derive(Debug, Clone)]
pub struct ObjectStreamConfig {
    pub max_objects_per_stream: usize,
    pub compression_level: u32,
}

impl Default for ObjectStreamConfig {
    fn default() -> Self {
        Self {
            max_objects_per_stream: 100,
            compression_level: 6,
        }
    }
}

impl ObjectStream {
    /// Parse an existing object stream
    pub fn new(stream: &mut Stream) -> Result<ObjectStream> {
        let _ = stream.decompress();

        if stream.content.is_empty() {
            return Ok(ObjectStream {
                objects: BTreeMap::new(),
                max_objects: 100,
                compression_level: 6,
            });
        }

        let first_offset = stream
            .dict
            .get(b"First")
            .and_then(Object::as_i64)?
            .try_into()
            .map_err(|e: TryFromIntError| Error::NumericCast(e.to_string()))?;
        let index_block = stream
            .content
            .get(..first_offset)
            .ok_or(Error::InvalidOffset(first_offset))?;

        let numbers_str = std::str::from_utf8(index_block).map_err(|e| Error::InvalidObjectStream(e.to_string()))?;
        let numbers: Vec<_> = numbers_str
            .split_whitespace()
            .map(|number| u32::from_str(number).ok())
            .collect();
        let len = numbers.len() / 2 * 2; // Ensure only pairs.

        let n = stream.dict.get(b"N").and_then(Object::as_i64)?;
        if numbers.len().try_into().ok() != n.checked_mul(2) {
            warn!("object stream: the object stream dictionary specifies a wrong number of objects")
        }

        let chunks_filter_map = |chunk: &[_]| {
            let id = chunk[0]?;
            let offset = first_offset + chunk[1]? as usize;

            if offset >= stream.content.len() {
                warn!("out-of-bounds offset in object stream");
                return None;
            }
            let object = parser::direct_object(ParserInput::new_extra(&stream.content[offset..], "direct object"))?;

            Some(((id, 0), object))
        };
        #[cfg(feature = "rayon")]
        let objects = numbers[..len].par_chunks(2).filter_map(chunks_filter_map).collect();
        #[cfg(not(feature = "rayon"))]
        let objects = numbers[..len].chunks(2).filter_map(chunks_filter_map).collect();

        Ok(ObjectStream { 
            objects,
            max_objects: 100,
            compression_level: 6,
        })
    }

    /// Create a builder for constructing new object streams
    pub fn builder() -> ObjectStreamBuilder {
        ObjectStreamBuilder {
            max_objects: 100,
            compression_level: 6,
        }
    }

    /// Add an object to the stream
    pub fn add_object(&mut self, id: ObjectId, obj: Object) -> Result<()> {
        // Check if object can be added to stream
        if matches!(obj, Object::Stream(_)) {
            return Err(Error::InvalidObjectStream("Stream objects cannot be stored in object streams".into()));
        }

        // Check capacity
        if self.objects.len() >= self.max_objects {
            return Err(Error::InvalidObjectStream(format!(
                "Object stream has reached maximum capacity of {} objects",
                self.max_objects
            )));
        }

        self.objects.insert(id, obj);
        Ok(())
    }

    /// Get the number of objects in the stream
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Build the stream content in the format required by PDF spec
    pub fn build_stream_content(&self) -> Result<Vec<u8>> {
        if self.objects.is_empty() {
            return Ok(Vec::new());
        }

        // Sort objects by ID for consistent output
        let mut sorted_objects: Vec<_> = self.objects.iter().collect();
        sorted_objects.sort_by_key(|(id, _)| *id);

        // First build the offset table to know its size
        let mut offset_entries = Vec::new();
        let mut current_offset = 0;
        
        for ((obj_num, _gen), obj) in &sorted_objects {
            // Store the object number and its offset
            offset_entries.push(format!("{} {}", obj_num, current_offset));
            
            // Calculate size of this object's serialization
            let mut obj_bytes = Vec::new();
            crate::writer::Writer::write_object(&mut obj_bytes, obj)?;
            current_offset += obj_bytes.len() + 1; // +1 for space separator
        }

        // Build the complete offset table with proper spacing
        let offset_table = offset_entries.join(" ") + " ";
        
        // Now build the final content
        let mut content = Vec::new();
        content.extend_from_slice(offset_table.as_bytes());
        
        // Add serialized objects with space separators
        for ((_, _), obj) in &sorted_objects {
            let mut obj_bytes = Vec::new();
            crate::writer::Writer::write_object(&mut obj_bytes, obj)?;
            content.extend_from_slice(&obj_bytes);
            content.push(b' '); // Space separator between objects
        }

        Ok(content)
    }

    /// Convert to a Stream object ready for insertion into a PDF
    pub fn to_stream_object(&self) -> Result<Stream> {
        let content = self.build_stream_content()?;
        
        // Calculate where the first object starts
        // We need to find the size of the offset table
        let mut sorted_objects: Vec<_> = self.objects.iter().collect();
        sorted_objects.sort_by_key(|(id, _)| *id);
        
        // Build the offset entries to calculate exact size
        let mut offset_entries = Vec::new();
        let mut current_offset = 0;
        
        for ((obj_num, _gen), obj) in &sorted_objects {
            offset_entries.push(format!("{} {}", obj_num, current_offset));
            
            // Calculate size of this object's serialization
            let mut obj_bytes = Vec::new();
            crate::writer::Writer::write_object(&mut obj_bytes, obj)?;
            current_offset += obj_bytes.len() + 1; // +1 for space separator
        }
        
        // The offset table is joined with spaces and has a trailing space
        let offset_table = offset_entries.join(" ") + " ";
        let first_offset = offset_table.len();
        
        let dict = dictionary! {
            "Type" => "ObjStm",
            "N" => self.objects.len() as i64,
            "First" => first_offset as i64,
        };

        let mut stream = Stream::new(dict, content);
        
        // Apply compression - object streams should always be compressed
        if self.compression_level > 0 {
            // Force compression by setting Filter directly
            use flate2::write::ZlibEncoder;
            use flate2::Compression;
            use std::io::prelude::*;
            
            let compression = match self.compression_level {
                0 => Compression::none(),
                1..=3 => Compression::fast(),
                4..=6 => Compression::default(),
                _ => Compression::best(),
            };
            
            let mut encoder = ZlibEncoder::new(Vec::new(), compression);
            encoder.write_all(&stream.content)?;
            let compressed = encoder.finish()?;
            
            stream.dict.set("Filter", "FlateDecode");
            stream.set_content(compressed);
        }

        Ok(stream)
    }

    /// Check if an object can be compressed into an object stream
    pub fn can_be_compressed(id: ObjectId, _obj: &Object, doc: &Document) -> bool {
        // Use transitive closure to find all non-compressible objects
        let non_compressible = Self::find_all_non_compressible_objects(doc);
        !non_compressible.contains(&id)
    }
    
    /// Find all non-compressible objects using transitive closure
    fn find_all_non_compressible_objects(doc: &Document) -> HashSet<ObjectId> {
        let mut non_compressible = HashSet::new();
        
        // Phase 1: Mark inherently non-compressible objects
        for (&id, obj) in &doc.objects {
            let mut is_non_compressible = false;
            
            // Streams cannot be compressed
            if matches!(obj, Object::Stream(_)) {
                is_non_compressible = true;
            }
            
            // Check object type
            if let Object::Dictionary(dict) = obj {
                if let Ok(type_obj) = dict.get(b"Type") {
                    if let Ok(type_name) = type_obj.as_name() {
                        match type_name {
                            b"Page" | b"Pages" | b"Catalog" | b"XRef" | b"ObjStm" => {
                                is_non_compressible = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            
            // Check if referenced in trailer
            for (_key, value) in doc.trailer.iter() {
                if value == &Object::Reference(id) {
                    is_non_compressible = true;
                    break;
                }
            }
            
            if is_non_compressible {
                non_compressible.insert(id);
            }
        }
        
        // Phase 2: Iteratively mark objects referenced by non-compressible objects
        let mut changed = true;
        while changed {
            changed = false;
            let mut newly_non_compressible = Vec::new();
            
            for &nc_id in &non_compressible {
                if let Ok(nc_obj) = doc.get_object(nc_id) {
                    let refs = Self::collect_all_references(nc_obj);
                    
                    for ref_id in refs {
                        if !non_compressible.contains(&ref_id) && doc.objects.contains_key(&ref_id) {
                            newly_non_compressible.push(ref_id);
                            changed = true;
                        }
                    }
                }
            }
            
            for id in newly_non_compressible {
                non_compressible.insert(id);
            }
        }
        
        non_compressible
    }
    
    /// Collect all object references from an object
    fn collect_all_references(obj: &Object) -> HashSet<ObjectId> {
        let mut refs = HashSet::new();
        Self::collect_references_recursive(obj, &mut refs);
        refs
    }
    
    /// Recursively collect all references from an object
    fn collect_references_recursive(obj: &Object, refs: &mut HashSet<ObjectId>) {
        match obj {
            Object::Reference(id) => {
                refs.insert(*id);
            }
            Object::Array(array) => {
                for item in array {
                    Self::collect_references_recursive(item, refs);
                }
            }
            Object::Dictionary(dict) => {
                for (_key, value) in dict.iter() {
                    Self::collect_references_recursive(value, refs);
                }
            }
            Object::Stream(stream) => {
                for (_key, value) in stream.dict.iter() {
                    Self::collect_references_recursive(value, refs);
                }
            }
            _ => {}
        }
    }
}

impl ObjectStreamBuilder {
    /// Set the maximum number of objects per stream
    pub fn max_objects(mut self, max: usize) -> Self {
        self.max_objects = max;
        self
    }

    /// Set the compression level (0-9)
    pub fn compression_level(mut self, level: u32) -> Self {
        self.compression_level = level;
        self
    }

    /// Build the ObjectStream
    pub fn build(self) -> ObjectStream {
        ObjectStream {
            objects: BTreeMap::new(),
            max_objects: self.max_objects,
            compression_level: self.compression_level,
        }
    }

    /// Get the current max_objects setting
    pub fn get_max_objects(&self) -> usize {
        self.max_objects
    }

    /// Get the current compression_level setting
    pub fn get_compression_level(&self) -> u32 {
        self.compression_level
    }
}
