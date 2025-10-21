use log::{error, warn};
use std::cmp;
use std::collections::{BTreeMap, HashSet};
use std::convert::TryInto;
#[cfg(not(feature = "async"))]
use std::fs::File;
#[cfg(not(feature = "async"))]
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;

#[cfg(feature = "rayon")]
use rayon::prelude::*;
#[cfg(feature = "async")]
use tokio::fs::File;
#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncReadExt};
#[cfg(feature = "async")]
use tokio::pin;

use crate::error::{ParseError, XrefError};
use crate::load_options::{LoadOptions, LoadProgress, ProgressInterval};
use crate::object_stream::ObjectStream;
use crate::parser::{self, ParserInput};
use crate::xref::XrefEntry;
use crate::{Document, Error, IncrementalDocument, Object, ObjectId, Result};

type FilterFunc = fn((u32, u16), &mut Object) -> Option<((u32, u16), Object)>;

/// Yield control back to the event loop (WASM or async runtime).
/// This allows the browser/runtime to process other events and update the UI.
#[cfg(target_arch = "wasm32")]
async fn yield_now() {
    use wasm_bindgen_futures::JsFuture;
    use js_sys::Promise;
    // Yield to JavaScript event loop
    let _ = JsFuture::from(Promise::resolve(&wasm_bindgen::JsValue::NULL)).await;
}

/// Yield control back to the tokio runtime.
#[cfg(all(feature = "async", not(target_arch = "wasm32")))]
async fn yield_now() {
    tokio::task::yield_now().await;
}

#[cfg(not(feature = "async"))]
impl Document {
    /// Load a PDF document from a specified file path.
    #[inline]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Document> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_internal(file, capacity, None)
    }

    #[inline]
    pub fn load_filtered<P: AsRef<Path>>(path: P, filter_func: FilterFunc) -> Result<Document> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_internal(file, capacity, Some(filter_func))
    }

    /// Load a PDF document from an arbitrary source.
    #[inline]
    pub fn load_from<R: Read>(source: R) -> Result<Document> {
        Self::load_internal(source, None, None)
    }

    fn load_internal<R: Read>(
        mut source: R, capacity: Option<usize>, filter_func: Option<FilterFunc>,
    ) -> Result<Document> {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        Reader {
            buffer: &buffer,
            document: Document::new(),
        }
        .read(filter_func)
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        buffer.try_into()
    }

    /// Load a PDF document from a specified file path with options.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lopdf::{Document, LoadOptions};
    ///
    /// let options = LoadOptions::new()
    ///     .with_progress(|p| {
    ///         println!("{}%: {}", (p.progress * 100.0) as u8, p.stage_name);
    ///     });
    ///
    /// let doc = Document::load_with_options("file.pdf", options)?;
    /// # Ok::<(), lopdf::Error>(())
    /// ```
    #[inline]
    pub fn load_with_options<'a, P: AsRef<Path>>(path: P, options: LoadOptions<'a>) -> Result<Document> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_internal_with_options(file, capacity, None, options)
    }

    /// Load a PDF document from an arbitrary source with options.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lopdf::{Document, LoadOptions};
    /// use std::fs::File;
    ///
    /// let file = File::open("file.pdf")?;
    /// let options = LoadOptions::new()
    ///     .with_progress(|p| {
    ///         println!("Loading: {}%", (p.progress * 100.0) as u8);
    ///     });
    ///
    /// let doc = Document::load_from_with_options(file, options)?;
    /// # Ok::<(), lopdf::Error>(())
    /// ```
    #[inline]
    pub fn load_from_with_options<'a, R: Read>(source: R, options: LoadOptions<'a>) -> Result<Document> {
        Self::load_internal_with_options(source, None, None, options)
    }

    /// Load a PDF document from a memory slice with options.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lopdf::{Document, LoadOptions, ProgressInterval};
    ///
    /// let pdf_bytes = include_bytes!("../assets/example.pdf");
    /// let options = LoadOptions::new()
    ///     .with_progress(|p| {
    ///         eprintln!("Progress: {}%", (p.progress * 100.0) as u8);
    ///     })
    ///     .with_progress_interval(ProgressInterval::Percentage(5.0));
    ///
    /// let doc = Document::load_mem_with_options(pdf_bytes, options)?;
    /// # Ok::<(), lopdf::Error>(())
    /// ```
    #[cfg(not(any(target_arch = "wasm32", feature = "async")))]
    pub fn load_mem_with_options<'a>(buffer: &[u8], options: LoadOptions<'a>) -> Result<Document> {
        Reader {
            buffer,
            document: Document::new(),
        }
        .read_with_options(None, options)
    }

    /// Load a PDF document from a memory slice with options (async version).
    ///
    /// This async version yields control back to the event loop periodically,
    /// allowing UI updates in WASM builds or other async operations to proceed.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lopdf::{Document, LoadOptions, ProgressInterval};
    ///
    /// # async fn example() -> Result<(), lopdf::Error> {
    /// let pdf_bytes = include_bytes!("../assets/example.pdf");
    /// let options = LoadOptions::new()
    ///     .with_progress(|p| {
    ///         eprintln!("Progress: {}%", (p.progress * 100.0) as u8);
    ///     })
    ///     .with_progress_interval(ProgressInterval::Percentage(5.0));
    ///
    /// let doc = Document::load_mem_with_options(pdf_bytes, options).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(target_arch = "wasm32", feature = "async"))]
    pub async fn load_mem_with_options<'a>(buffer: &[u8], options: LoadOptions<'a>) -> Result<Document> {
        Reader {
            buffer,
            document: Document::new(),
        }
        .read_with_options_async(None, options)
        .await
    }

    fn load_internal_with_options<'a, R: Read>(
        mut source: R,
        capacity: Option<usize>,
        filter_func: Option<FilterFunc>,
        options: LoadOptions<'a>,
    ) -> Result<Document> {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        Reader {
            buffer: &buffer,
            document: Document::new(),
        }
        .read_with_options(filter_func, options)
    }

    /// Load minimal PDF metadata (version, page count, info) without loading all objects.
    ///
    /// This is significantly faster than `load()` for extracting basic metadata,
    /// as it only loads the minimal objects needed (typically 3-20 objects vs thousands).
    ///
    /// # What is loaded
    /// - PDF version
    /// - Cross-reference table structure
    /// - Trailer dictionary
    /// - Catalog object
    /// - Pages tree (including Page objects but not their content)
    /// - Info dictionary (if present)
    ///
    /// # What is NOT loaded
    /// - Stream objects (page content, fonts, images)
    /// - Resources (fonts, images, etc.)
    /// - Annotations, forms, and other page content
    /// - Objects referenced by pages but not needed for page counting
    ///
    /// # Limitations
    /// - Does not load objects stored in object streams. If structural objects
    ///   (Catalog, Pages) are compressed in object streams, this method may not
    ///   work correctly. Most PDFs store structural objects uncompressed.
    ///
    /// # Performance
    /// Typically 2-10x faster than `load()` depending on PDF size and complexity.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lopdf::Document;
    ///
    /// let doc = Document::load_minimal("large.pdf")?;
    /// println!("Version: {}", doc.version);
    /// println!("Pages: {}", doc.get_pages().len());
    /// # Ok::<(), lopdf::Error>(())
    /// ```
    #[inline]
    pub fn load_minimal<P: AsRef<Path>>(path: P) -> Result<Document> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_minimal_internal(file, capacity)
    }

    /// Load minimal PDF metadata from an arbitrary source.
    ///
    /// See [`load_minimal`](Self::load_minimal) for details.
    #[inline]
    pub fn load_minimal_from<R: Read>(source: R) -> Result<Document> {
        Self::load_minimal_internal(source, None)
    }

    /// Load minimal PDF metadata from a memory slice.
    ///
    /// See [`load_minimal`](Self::load_minimal) for details.
    pub fn load_minimal_mem(buffer: &[u8]) -> Result<Document> {
        Reader {
            buffer,
            document: Document::new(),
        }
        .read_minimal()
    }

    fn load_minimal_internal<R: Read>(mut source: R, capacity: Option<usize>) -> Result<Document> {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        Reader {
            buffer: &buffer,
            document: Document::new(),
        }
        .read_minimal()
    }

    /// Process images from a PDF file with a callback function.
    ///
    /// This method loads only the structural objects and image XObjects needed to extract
    /// images, skipping content streams, fonts, and other resources. It's significantly
    /// faster than loading the full document when you only need images.
    ///
    /// The callback is invoked immediately for each image as it's discovered, allowing for
    /// progressive/streaming display rather than batch loading.
    ///
    /// # Performance
    ///
    /// Typically 5-20x faster than `load()` + `get_page_images()` for large PDFs, as it
    /// only loads ~5-15% of the objects in the document.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lopdf::Document;
    ///
    /// Document::process_images_with_callback("large.pdf", |page_image| {
    ///     println!("Page {}: Found {}x{} image",
    ///              page_image.page_number,
    ///              page_image.width,
    ///              page_image.height);
    ///
    ///     // Display or process the image immediately
    ///     if page_image.filters.contains(&"DCTDecode".to_string()) {
    ///         // It's JPEG data, can save directly
    ///         std::fs::write(
    ///             format!("page_{}_img_{}.jpg", page_image.page_number, page_image.id.0),
    ///             &page_image.content
    ///         )?;
    ///     }
    ///
    ///     Ok(())
    /// })?;
    /// # Ok::<(), lopdf::Error>(())
    /// ```
    #[inline]
    pub fn process_images_with_callback<P, F>(path: P, callback: F) -> Result<()>
    where
        P: AsRef<Path>,
        F: FnMut(crate::xobject::PageImage) -> Result<()>,
    {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::process_images_internal(file, capacity, callback)
    }

    /// Process images from an arbitrary source with a callback function.
    ///
    /// See [`process_images_with_callback`](Self::process_images_with_callback) for details.
    #[inline]
    pub fn process_images_from<R, F>(source: R, callback: F) -> Result<()>
    where
        R: Read,
        F: FnMut(crate::xobject::PageImage) -> Result<()>,
    {
        Self::process_images_internal(source, None, callback)
    }

    /// Process images from a memory slice with a callback function.
    ///
    /// This is the most efficient variant for concurrent operations, as multiple
    /// operations can share the same buffer using `Arc`.
    ///
    /// See [`process_images_with_callback`](Self::process_images_with_callback) for details.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lopdf::Document;
    /// use std::sync::Arc;
    ///
    /// // Read file once
    /// let pdf_bytes = Arc::new(std::fs::read("file.pdf")?);
    ///
    /// // Process images concurrently with other operations
    /// let bytes1 = Arc::clone(&pdf_bytes);
    /// let handle = std::thread::spawn(move || {
    ///     Document::process_images_mem(&bytes1, |img| {
    ///         println!("Found image on page {}", img.page_number);
    ///         Ok(())
    ///     })
    /// });
    ///
    /// // Can also run load_minimal_mem, load_mem_with_options, etc. concurrently
    /// let minimal = Document::load_minimal_mem(&pdf_bytes)?;
    ///
    /// handle.join().unwrap()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn process_images_mem<F>(buffer: &[u8], callback: F) -> Result<()>
    where
        F: FnMut(crate::xobject::PageImage) -> Result<()>,
    {
        Reader {
            buffer,
            document: Document::new(),
        }
        .process_images(callback)
    }

    fn process_images_internal<R, F>(mut source: R, capacity: Option<usize>, callback: F) -> Result<()>
    where
        R: Read,
        F: FnMut(crate::xobject::PageImage) -> Result<()>,
    {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        Reader {
            buffer: &buffer,
            document: Document::new(),
        }
        .process_images(callback)
    }
}

#[cfg(feature = "async")]
impl Document {
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Document> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let capacity = Some(metadata.len() as usize);
        Self::load_internal(file, capacity, None).await
    }

    pub async fn load_filtered<P: AsRef<Path>>(path: P, filter_func: FilterFunc) -> Result<Document> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let capacity = Some(metadata.len() as usize);
        Self::load_internal(file, capacity, Some(filter_func)).await
    }

    async fn load_internal<R: AsyncRead>(
        source: R, capacity: Option<usize>, filter_func: Option<FilterFunc>,
    ) -> Result<Document> {
        pin!(source);

        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer).await?;

        Reader {
            buffer: &buffer,
            document: Document::new(),
        }
        .read(filter_func)
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        buffer.try_into()
    }
}

impl TryInto<Document> for &[u8] {
    type Error = Error;

    fn try_into(self) -> Result<Document> {
        Reader {
            buffer: self,
            document: Document::new(),
        }
        .read(None)
    }
}

#[cfg(not(feature = "async"))]
impl IncrementalDocument {
    /// Load a PDF document from a specified file path.
    #[inline]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_internal(file, capacity)
    }

    /// Load a PDF document from an arbitrary source.
    #[inline]
    pub fn load_from<R: Read>(source: R) -> Result<Self> {
        Self::load_internal(source, None)
    }

    fn load_internal<R: Read>(mut source: R, capacity: Option<usize>) -> Result<Self> {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        let document = Reader {
            buffer: &buffer,
            document: Document::new(),
        }
        .read(None)?;

        Ok(IncrementalDocument::create_from(buffer, document))
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        buffer.try_into()
    }
}

#[cfg(feature = "async")]
impl IncrementalDocument {
    /// Load a PDF document from a specified file path.
    #[inline]
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let capacity = Some(metadata.len() as usize);
        Self::load_internal(file, capacity).await
    }

    /// Load a PDF document from an arbitrary source.
    #[inline]
    pub async fn load_from<R: AsyncRead>(source: R) -> Result<Self> {
        Self::load_internal(source, None).await
    }

    async fn load_internal<R: AsyncRead>(source: R, capacity: Option<usize>) -> Result<Self> {
        pin!(source);

        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer).await?;

        let document = Reader {
            buffer: &buffer,
            document: Document::new(),
        }
        .read(None)?;

        Ok(IncrementalDocument::create_from(buffer, document))
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        buffer.try_into()
    }
}

impl TryInto<IncrementalDocument> for &[u8] {
    type Error = Error;

    fn try_into(self) -> Result<IncrementalDocument> {
        let document = Reader {
            buffer: self,
            document: Document::new(),
        }
        .read(None)?;

        Ok(IncrementalDocument::create_from(self.to_vec(), document))
    }
}

pub struct Reader<'a> {
    pub buffer: &'a [u8],
    pub document: Document,
}

/// Maximum allowed embedding of literal strings.
pub const MAX_BRACKET: usize = 100;

impl Reader<'_> {
    /// Read whole document.
    pub fn read(mut self, filter_func: Option<FilterFunc>) -> Result<Document> {
        let offset = self.buffer.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
        self.buffer = &self.buffer[offset..];

        // The document structure can be expressed in PEG as:
        //   document <- header indirect_object* xref trailer xref_start
        let version =
            parser::header(ParserInput::new_extra(self.buffer, "header")).ok_or(ParseError::InvalidFileHeader)?;

        //The binary_mark is in line 2 after the pdf version. If at other line number, then will be declared as invalid pdf.
        if let Some(pos) = self.buffer.iter().position(|&byte| byte == b'\n') {
            if let Some(binary_mark) =
                parser::binary_mark(ParserInput::new_extra(&self.buffer[pos + 1..], "binary_mark"))
            {
                if binary_mark.iter().all(|&byte| byte >= 128) {
                    self.document.binary_mark = binary_mark;
                }
            }
        }

        let xref_start = Self::get_xref_start(self.buffer)?;
        if xref_start > self.buffer.len() {
            return Err(Error::Xref(XrefError::Start));
        }
        self.document.xref_start = xref_start;

        let (mut xref, mut trailer) =
            parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[xref_start..], "xref"), &self)?;

        // Read previous Xrefs of linearized or incremental updated document.
        let mut already_seen = HashSet::new();
        let mut prev_xref_start = trailer.remove(b"Prev");
        while let Some(prev) = prev_xref_start.and_then(|offset| offset.as_i64().ok()) {
            if already_seen.contains(&prev) {
                break;
            }
            already_seen.insert(prev);
            if prev < 0 || prev as usize > self.buffer.len() {
                return Err(Error::Xref(XrefError::PrevStart));
            }

            let (prev_xref, prev_trailer) =
                parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
            xref.merge(prev_xref);

            // Read xref stream in hybrid-reference file
            let prev_xref_stream_start = trailer.remove(b"XRefStm");
            if let Some(prev) = prev_xref_stream_start.and_then(|offset| offset.as_i64().ok()) {
                if prev < 0 || prev as usize > self.buffer.len() {
                    return Err(Error::Xref(XrefError::StreamStart));
                }

                let (prev_xref, _) =
                    parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
                xref.merge(prev_xref);
            }

            prev_xref_start = prev_trailer.get(b"Prev").cloned().ok();
        }
        let xref_entry_count = xref.max_id().checked_add(1).ok_or(ParseError::InvalidXref)?;
        if xref.size != xref_entry_count {
            warn!(
                "Size entry of trailer dictionary is {}, correct value is {}.",
                xref.size, xref_entry_count
            );
            xref.size = xref_entry_count;
        }

        self.document.version = version;
        self.document.max_id = xref.size - 1;
        self.document.trailer = trailer;
        self.document.reference_table = xref;

        let is_encrypted = self.document.trailer.get(b"Encrypt").is_ok();

        let zero_length_streams = Mutex::new(vec![]);
        let object_streams = Mutex::new(vec![]);

        let entries_filter_map = |(_, entry): (&_, &_)| {
            if let XrefEntry::Normal { offset, .. } = *entry {
                let (object_id, mut object) = self
                    .read_object(offset as usize, None, &mut HashSet::new())
                    .map_err(|e| error!("Object load error: {e:?}"))
                    .ok()?;
                if let Some(filter_func) = filter_func {
                    filter_func(object_id, &mut object)?;
                }

                if let Ok(ref mut stream) = object.as_stream_mut() {
                    if stream.dict.has_type(b"ObjStm") && !is_encrypted {
                        let obj_stream = ObjectStream::new(stream).ok()?;
                        let mut object_streams = object_streams.lock().unwrap();
                        // TODO: Is insert and replace intended behavior?
                        // See https://github.com/J-F-Liu/lopdf/issues/160 for more info
                        if let Some(filter_func) = filter_func {
                            let objects: BTreeMap<(u32, u16), Object> = obj_stream
                                .objects
                                .into_iter()
                                .filter_map(|(object_id, mut object)| filter_func(object_id, &mut object))
                                .collect();
                            object_streams.extend(objects);
                        } else {
                            object_streams.extend(obj_stream.objects);
                        }
                    } else if stream.content.is_empty() {
                        let mut zero_length_streams = zero_length_streams.lock().unwrap();
                        zero_length_streams.push(object_id);
                    }
                }

                Some((object_id, object))
            } else {
                None
            }
        };
        #[cfg(feature = "rayon")]
        {
            self.document.objects = self
                .document
                .reference_table
                .entries
                .par_iter()
                .filter_map(entries_filter_map)
                .collect();
        }
        #[cfg(not(feature = "rayon"))]
        {
            self.document.objects = self
                .document
                .reference_table
                .entries
                .iter()
                .filter_map(entries_filter_map)
                .collect();
        }
        // Only add entries, but never replace entries
        for (id, entry) in object_streams.into_inner().unwrap() {
            self.document.objects.entry(id).or_insert(entry);
        }

        for object_id in zero_length_streams.into_inner().unwrap() {
            let _ = self.read_stream_content(object_id);
        }

        let mut document = self.document;

        if document.authenticate_password("").is_ok() {
            document.decrypt("")?;
        }

        Ok(document)
    }

    /// Read whole document with progress tracking.
    pub fn read_with_options<'a>(mut self, filter_func: Option<FilterFunc>, options: LoadOptions<'a>) -> Result<Document> {
        // Stage 0: Reading file (already done)
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(0, 0.01, 0, 0, Some("Reading file".to_string())));
        }

        // Stage 1: Finding PDF header
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(1, 0.02, 0, 0, Some("Finding PDF header".to_string())));
        }

        let offset = self.buffer.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
        self.buffer = &self.buffer[offset..];

        // Stage 2: Parsing version
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(2, 0.03, 0, 0, Some("Parsing version".to_string())));
        }

        let version =
            parser::header(ParserInput::new_extra(self.buffer, "header")).ok_or(ParseError::InvalidFileHeader)?;

        //The binary_mark is in line 2 after the pdf version
        if let Some(pos) = self.buffer.iter().position(|&byte| byte == b'\n') {
            if let Some(binary_mark) =
                parser::binary_mark(ParserInput::new_extra(&self.buffer[pos + 1..], "binary_mark"))
            {
                if binary_mark.iter().all(|&byte| byte >= 128) {
                    self.document.binary_mark = binary_mark;
                }
            }
        }

        // Stage 3: Parsing cross-reference table
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(3, 0.06, 0, 0, Some("Parsing cross-reference table".to_string())));
        }

        let xref_start = Self::get_xref_start(self.buffer)?;
        if xref_start > self.buffer.len() {
            return Err(Error::Xref(XrefError::Start));
        }
        self.document.xref_start = xref_start;

        let (mut xref, mut trailer) =
            parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[xref_start..], "xref"), &self)?;

        // Read previous Xrefs
        let mut already_seen = HashSet::new();
        let mut prev_xref_start = trailer.remove(b"Prev");
        while let Some(prev) = prev_xref_start.and_then(|offset| offset.as_i64().ok()) {
            if already_seen.contains(&prev) {
                break;
            }
            already_seen.insert(prev);
            if prev < 0 || prev as usize > self.buffer.len() {
                return Err(Error::Xref(XrefError::PrevStart));
            }

            let (prev_xref, prev_trailer) =
                parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
            xref.merge(prev_xref);

            // Read xref stream in hybrid-reference file
            let prev_xref_stream_start = trailer.remove(b"XRefStm");
            if let Some(prev) = prev_xref_stream_start.and_then(|offset| offset.as_i64().ok()) {
                if prev < 0 || prev as usize > self.buffer.len() {
                    return Err(Error::Xref(XrefError::StreamStart));
                }

                let (prev_xref, _) =
                    parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
                xref.merge(prev_xref);
            }

            prev_xref_start = prev_trailer.get(b"Prev").cloned().ok();
        }
        let xref_entry_count = xref.max_id().checked_add(1).ok_or(ParseError::InvalidXref)?;
        if xref.size != xref_entry_count {
            warn!(
                "Size entry of trailer dictionary is {}, correct value is {}.",
                xref.size, xref_entry_count
            );
            xref.size = xref_entry_count;
        }

        // Stage 4: Parsing trailer
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(4, 0.41, 0, 0, Some("Parsing trailer".to_string())));
        }

        self.document.version = version;
        self.document.max_id = xref.size - 1;
        self.document.trailer = trailer;
        self.document.reference_table = xref;

        let is_encrypted = self.document.trailer.get(b"Encrypt").is_ok();

        let zero_length_streams = Mutex::new(vec![]);
        let object_streams = Mutex::new(vec![]);

        // Stage 5: Loading objects
        let total_objects = self.document.reference_table.entries.len();
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(5, 0.46, 0, total_objects, Some("Loading objects".to_string())));
        }

        // For progress tracking in parallel loading
        #[cfg(feature = "rayon")]
        use std::sync::atomic::{AtomicUsize, Ordering};
        #[cfg(feature = "rayon")]
        let processed = AtomicUsize::new(0);

        let entries_filter_map = |(_, entry): (&_, &_)| {
            if let XrefEntry::Normal { offset, .. } = *entry {
                let (object_id, mut object) = self
                    .read_object(offset as usize, None, &mut HashSet::new())
                    .map_err(|e| error!("Object load error: {e:?}"))
                    .ok()?;
                if let Some(filter_func) = filter_func {
                    filter_func(object_id, &mut object)?;
                }

                if let Ok(ref mut stream) = object.as_stream_mut() {
                    if stream.dict.has_type(b"ObjStm") && !is_encrypted {
                        let obj_stream = ObjectStream::new(stream).ok()?;
                        let mut object_streams = object_streams.lock().unwrap();
                        if let Some(filter_func) = filter_func {
                            let objects: BTreeMap<(u32, u16), Object> = obj_stream
                                .objects
                                .into_iter()
                                .filter_map(|(object_id, mut object)| filter_func(object_id, &mut object))
                                .collect();
                            object_streams.extend(objects);
                        } else {
                            object_streams.extend(obj_stream.objects);
                        }
                    } else if stream.content.is_empty() {
                        let mut zero_length_streams = zero_length_streams.lock().unwrap();
                        zero_length_streams.push(object_id);
                    }
                }

                // Report progress
                #[cfg(feature = "rayon")]
                if let Some(ref callback) = options.progress_callback {
                    let count = processed.fetch_add(1, Ordering::Relaxed) + 1;
                    let should_report = match options.progress_interval {
                        ProgressInterval::Items(n) => count % n == 0 || count == total_objects,
                        ProgressInterval::Percentage(p) => {
                            let current_pct = (count as f64 / total_objects as f64) * 100.0;
                            let prev_pct = ((count - 1) as f64 / total_objects as f64) * 100.0;
                            (current_pct / p).floor() > (prev_pct / p).floor() || count == total_objects
                        }
                    };
                    if should_report {
                        let progress = 0.46 + (0.54 * count as f64 / total_objects as f64);
                        callback(LoadProgress::new(5, progress, count, total_objects, None));
                    }
                }

                Some((object_id, object))
            } else {
                None
            }
        };

        #[cfg(feature = "rayon")]
        {
            self.document.objects = self
                .document
                .reference_table
                .entries
                .par_iter()
                .filter_map(entries_filter_map)
                .collect();
        }
        #[cfg(not(feature = "rayon"))]
        {
            self.document.objects = self
                .document
                .reference_table
                .entries
                .iter()
                .enumerate()
                .filter_map(|(idx, entry)| {
                    let result = entries_filter_map(entry);

                    // Report progress for sequential loading
                    if let Some(ref callback) = options.progress_callback {
                        let count = idx + 1;
                        let should_report = match options.progress_interval {
                            ProgressInterval::Items(n) => count % n == 0 || count == total_objects,
                            ProgressInterval::Percentage(p) => {
                                let current_pct = (count as f64 / total_objects as f64) * 100.0;
                                let prev_pct = ((count - 1) as f64 / total_objects as f64) * 100.0;
                                (current_pct / p).floor() > (prev_pct / p).floor() || count == total_objects
                            }
                        };
                        if should_report {
                            let progress = 0.46 + (0.54 * count as f64 / total_objects as f64);
                            callback(LoadProgress::new(5, progress, count, total_objects, None));
                        }
                    }

                    result
                })
                .collect();
        }

        // Only add entries, but never replace entries
        for (id, entry) in object_streams.into_inner().unwrap() {
            self.document.objects.entry(id).or_insert(entry);
        }

        for object_id in zero_length_streams.into_inner().unwrap() {
            let _ = self.read_stream_content(object_id);
        }

        let mut document = self.document;

        if document.authenticate_password("").is_ok() {
            document.decrypt("")?;
        }

        // Stage 6: Complete
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(6, 1.0, total_objects, total_objects, Some("Complete".to_string())));
        }

        Ok(document)
    }

    /// Read whole document with progress tracking (async version with yield points).
    ///
    /// This async version yields control back to the event loop at strategic points,
    /// allowing UI updates in WASM builds or concurrent async operations.
    #[cfg(any(target_arch = "wasm32", feature = "async"))]
    pub async fn read_with_options_async<'a>(mut self, filter_func: Option<FilterFunc>, options: LoadOptions<'a>) -> Result<Document> {
        // Stage 0: Reading file (already done)
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(0, 0.01, 0, 0, Some("Reading file".to_string())));
        }

        // Stage 1: Finding PDF header
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(1, 0.02, 0, 0, Some("Finding PDF header".to_string())));
        }

        let offset = self.buffer.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
        self.buffer = &self.buffer[offset..];

        // Stage 2: Parsing version
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(2, 0.03, 0, 0, Some("Parsing version".to_string())));
        }

        let version =
            parser::header(ParserInput::new_extra(self.buffer, "header")).ok_or(ParseError::InvalidFileHeader)?;

        // Yield after parsing header
        yield_now().await;

        //The binary_mark is in line 2 after the pdf version
        if let Some(pos) = self.buffer.iter().position(|&byte| byte == b'\n') {
            if let Some(binary_mark) =
                parser::binary_mark(ParserInput::new_extra(&self.buffer[pos + 1..], "binary_mark"))
            {
                if binary_mark.iter().all(|&byte| byte >= 128) {
                    self.document.binary_mark = binary_mark;
                }
            }
        }

        // Stage 3: Parsing cross-reference table
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(3, 0.06, 0, 0, Some("Parsing cross-reference table".to_string())));
        }

        let xref_start = Self::get_xref_start(self.buffer)?;
        if xref_start > self.buffer.len() {
            return Err(Error::Xref(XrefError::Start));
        }
        self.document.xref_start = xref_start;

        let (mut xref, mut trailer) =
            parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[xref_start..], "xref"), &self)?;

        // Yield after parsing xref table
        yield_now().await;

        // Read previous Xrefs
        let mut already_seen = HashSet::new();
        let mut prev_xref_start = trailer.remove(b"Prev");
        while let Some(prev) = prev_xref_start.and_then(|offset| offset.as_i64().ok()) {
            if already_seen.contains(&prev) {
                break;
            }
            already_seen.insert(prev);
            if prev < 0 || prev as usize > self.buffer.len() {
                return Err(Error::Xref(XrefError::PrevStart));
            }

            let (prev_xref, prev_trailer) =
                parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
            xref.merge(prev_xref);

            // Read xref stream in hybrid-reference file
            let prev_xref_stream_start = trailer.remove(b"XRefStm");
            if let Some(prev) = prev_xref_stream_start.and_then(|offset| offset.as_i64().ok()) {
                if prev < 0 || prev as usize > self.buffer.len() {
                    return Err(Error::Xref(XrefError::StreamStart));
                }

                let (prev_xref, _) =
                    parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
                xref.merge(prev_xref);
            }

            prev_xref_start = prev_trailer.get(b"Prev").cloned().ok();
        }
        let xref_entry_count = xref.max_id().checked_add(1).ok_or(ParseError::InvalidXref)?;
        if xref.size != xref_entry_count {
            warn!(
                "Size entry of trailer dictionary is {}, correct value is {}.",
                xref.size, xref_entry_count
            );
            xref.size = xref_entry_count;
        }

        // Stage 4: Parsing trailer
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(4, 0.41, 0, 0, Some("Parsing trailer".to_string())));
        }

        // Yield after parsing trailer
        yield_now().await;

        self.document.version = version;
        self.document.max_id = xref.size - 1;
        self.document.trailer = trailer;
        self.document.reference_table = xref;

        let is_encrypted = self.document.trailer.get(b"Encrypt").is_ok();

        let zero_length_streams = Mutex::new(vec![]);
        let object_streams = Mutex::new(vec![]);

        // Stage 5: Loading objects
        let total_objects = self.document.reference_table.entries.len();
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(5, 0.46, 0, total_objects, Some("Loading objects".to_string())));
        }

        // For sequential loading with yield points
        let mut objects = BTreeMap::new();
        let mut processed = 0;

        for (_, entry) in self.document.reference_table.entries.iter() {
            if let XrefEntry::Normal { offset, .. } = *entry {
                if let Ok((mut object_id, mut object)) = self
                    .read_object(offset as usize, None, &mut HashSet::new())
                    .map_err(|e| error!("Object load error: {e:?}"))
                {
                    if let Some(filter_func) = filter_func {
                        if let Some((id, obj)) = filter_func(object_id, &mut object) {
                            object_id = id;
                            object = obj;
                        } else {
                            continue;
                        }
                    }

                    if let Ok(ref mut stream) = object.as_stream_mut() {
                        if stream.dict.has_type(b"ObjStm") && !is_encrypted {
                            let obj_stream = ObjectStream::new(stream).ok();
                            if let Some(obj_stream) = obj_stream {
                                let mut object_streams_lock = object_streams.lock().unwrap();
                                if let Some(filter_func) = filter_func {
                                    let objs: BTreeMap<(u32, u16), Object> = obj_stream
                                        .objects
                                        .into_iter()
                                        .filter_map(|(object_id, mut object)| filter_func(object_id, &mut object))
                                        .collect();
                                    object_streams_lock.extend(objs);
                                } else {
                                    object_streams_lock.extend(obj_stream.objects);
                                }
                            }
                        } else if stream.content.is_empty() {
                            let mut zero_length_streams_lock = zero_length_streams.lock().unwrap();
                            zero_length_streams_lock.push(object_id);
                        }
                    }

                    objects.insert(object_id, object);
                }
            }

            processed += 1;

            // Yield every 10 objects to allow UI updates
            if processed % 10 == 0 {
                yield_now().await;

                // Report progress
                if let Some(ref callback) = options.progress_callback {
                    let should_report = match options.progress_interval {
                        ProgressInterval::Items(n) => processed % n == 0 || processed == total_objects,
                        ProgressInterval::Percentage(p) => {
                            let current_pct = (processed as f64 / total_objects as f64) * 100.0;
                            let prev_pct = ((processed - 1) as f64 / total_objects as f64) * 100.0;
                            (current_pct / p).floor() > (prev_pct / p).floor() || processed == total_objects
                        }
                    };
                    if should_report {
                        let progress = 0.46 + (0.54 * processed as f64 / total_objects as f64);
                        callback(LoadProgress::new(5, progress, processed, total_objects, None));
                    }
                }
            }
        }

        self.document.objects = objects;

        // Only add entries, but never replace entries
        for (id, entry) in object_streams.into_inner().unwrap() {
            self.document.objects.entry(id).or_insert(entry);
        }

        for object_id in zero_length_streams.into_inner().unwrap() {
            let _ = self.read_stream_content(object_id);
        }

        let mut document = self.document;

        if document.authenticate_password("").is_ok() {
            document.decrypt("")?;
        }

        // Stage 6: Complete
        if let Some(ref callback) = options.progress_callback {
            callback(LoadProgress::new(6, 1.0, total_objects, total_objects, Some("Complete".to_string())));
        }

        Ok(document)
    }

    /// Read minimal document metadata only.
    ///
    /// This method loads only the objects necessary to extract:
    /// - PDF version
    /// - Page count (via Pages tree traversal)
    /// - Info dictionary metadata
    ///
    /// It loads ~3-5 objects instead of all objects in the document.
    pub fn read_minimal(mut self) -> Result<Document> {
        // Parse header and version
        let offset = self.buffer.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
        self.buffer = &self.buffer[offset..];

        let version =
            parser::header(ParserInput::new_extra(self.buffer, "header")).ok_or(ParseError::InvalidFileHeader)?;

        // Parse binary mark
        if let Some(pos) = self.buffer.iter().position(|&byte| byte == b'\n') {
            if let Some(binary_mark) =
                parser::binary_mark(ParserInput::new_extra(&self.buffer[pos + 1..], "binary_mark"))
            {
                if binary_mark.iter().all(|&byte| byte >= 128) {
                    self.document.binary_mark = binary_mark;
                }
            }
        }

        // Parse xref and trailer
        let xref_start = Self::get_xref_start(self.buffer)?;
        if xref_start > self.buffer.len() {
            return Err(Error::Xref(XrefError::Start));
        }
        self.document.xref_start = xref_start;

        let (mut xref, mut trailer) =
            parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[xref_start..], "xref"), &self)?;

        // Read previous Xrefs
        let mut already_seen = HashSet::new();
        let mut prev_xref_start = trailer.remove(b"Prev");
        while let Some(prev) = prev_xref_start.and_then(|offset| offset.as_i64().ok()) {
            if already_seen.contains(&prev) {
                break;
            }
            already_seen.insert(prev);
            if prev < 0 || prev as usize > self.buffer.len() {
                return Err(Error::Xref(XrefError::PrevStart));
            }

            let (prev_xref, prev_trailer) =
                parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
            xref.merge(prev_xref);

            // Read xref stream in hybrid-reference file
            let prev_xref_stream_start = trailer.remove(b"XRefStm");
            if let Some(prev) = prev_xref_stream_start.and_then(|offset| offset.as_i64().ok()) {
                if prev < 0 || prev as usize > self.buffer.len() {
                    return Err(Error::Xref(XrefError::StreamStart));
                }

                let (prev_xref, _) =
                    parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
                xref.merge(prev_xref);
            }

            prev_xref_start = prev_trailer.get(b"Prev").cloned().ok();
        }

        let xref_entry_count = xref.max_id().checked_add(1).ok_or(ParseError::InvalidXref)?;
        if xref.size != xref_entry_count {
            xref.size = xref_entry_count;
        }

        self.document.version = version;
        self.document.max_id = xref.size - 1;
        self.document.trailer = trailer.clone();
        self.document.reference_table = xref;

        // Now load only the minimal objects needed
        // 1. Catalog (Root)
        if let Ok(catalog_id) = trailer.get(b"Root").and_then(Object::as_reference) {
            let (_, catalog_obj) = self.read_object(
                self.get_offset(catalog_id)? as usize,
                Some(catalog_id),
                &mut HashSet::new(),
            )?;
            self.document.objects.insert(catalog_id, catalog_obj);

            // 2. Load Pages tree (recursively, but not individual Page objects)
            if let Ok(catalog_dict) = self.document.get_dictionary(catalog_id) {
                if let Ok(pages_id) = catalog_dict.get(b"Pages").and_then(Object::as_reference) {
                    self.load_pages_tree(pages_id)?;
                }
            }
        }

        // 3. Info dictionary (if present)
        if let Ok(info_id) = trailer.get(b"Info").and_then(Object::as_reference) {
            if let Ok(offset) = self.get_offset(info_id) {
                if let Ok((_, info_obj)) = self.read_object(offset as usize, Some(info_id), &mut HashSet::new()) {
                    self.document.objects.insert(info_id, info_obj);
                }
            }
        }

        Ok(self.document)
    }

    /// Recursively load Pages tree (including Page objects) but not their content streams.
    /// This builds the page map structure without loading page content, fonts, images, etc.
    fn load_pages_tree(&mut self, pages_id: ObjectId) -> Result<()> {
        // Check if already loaded
        if self.document.objects.contains_key(&pages_id) {
            // Already loaded (possibly from object stream)
            // Still need to process kids (clone to avoid borrow issues)
            let kids_to_process = if let Ok(dict) = self.document.get_dictionary(pages_id) {
                if dict.get_type().ok() == Some(b"Pages") {
                    dict.get(b"Kids").and_then(Object::as_array).ok().cloned()
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(kids_array) = kids_to_process {
                for kid in kids_array.iter() {
                    if let Ok(kid_id) = kid.as_reference() {
                        let _ = self.load_pages_tree(kid_id);
                    }
                }
            }
            return Ok(());
        }

        // Try to load this node using the helper (handles object streams)
        if self.load_object_if_needed(pages_id).is_err() {
            return Ok(()); // Failed to load, skip
        }

        // Extract kids for recursive processing (clone to avoid borrow issues)
        let kids_to_process = if let Ok(dict) = self.document.get_dictionary(pages_id) {
            if dict.get_type().ok() == Some(b"Pages") {
                // It's a Pages node - get the kids array
                dict.get(b"Kids").and_then(Object::as_array).ok().cloned()
            } else {
                // It's a Page node - no kids to process
                None
            }
        } else {
            None
        };

        // If there are kids to process, recursively load them
        if let Some(kids_array) = kids_to_process {
            for kid in kids_array.iter() {
                if let Ok(kid_id) = kid.as_reference() {
                    let _ = self.load_pages_tree(kid_id); // Recursively load, ignore errors
                }
            }
        }

        Ok(())
    }

    fn read_stream_content(&mut self, object_id: ObjectId) -> Result<()> {
        let length = self.get_stream_length(object_id)?;
        let stream = self
            .document
            .get_object_mut(object_id)
            .and_then(Object::as_stream_mut)?;
        let start = stream
            .start_position
            .ok_or(Error::InvalidStream("missing start position".to_string()))?;

        if length < 0 {
            return Err(Error::InvalidStream("negative stream length.".to_string()));
        }

        let length = usize::try_from(length).map_err(|e| Error::NumericCast(e.to_string()))?;
        let end = start + length;

        if end > self.buffer.len() {
            return Err(Error::InvalidStream("stream extends after document end.".to_string()));
        }

        stream.set_content(self.buffer[start..end].to_vec());
        Ok(())
    }

    fn get_stream_length(&self, object_id: ObjectId) -> Result<i64> {
        let object = self.document.get_object(object_id)?;
        let stream = object.as_stream()?;
        stream
            .dict
            .get(b"Length")
            .and_then(|value| self.document.dereference(value))
            .and_then(|(_id, obj)| obj.as_i64())
            .inspect_err(|_err| {
                error!(
                    "stream dictionary of '{} {} R' is missing the Length entry",
                    object_id.0, object_id.1
                );
            })
    }

    /// Get object offset by object ID.
    fn get_offset(&self, id: ObjectId) -> Result<u32> {
        let entry = self.document.reference_table.get(id.0).ok_or(Error::MissingXrefEntry)?;
        match *entry {
            XrefEntry::Normal { offset, generation } if generation == id.1 => Ok(offset),
            _ => Err(Error::MissingXrefEntry),
        }
    }

    pub fn get_object(&self, id: ObjectId, already_seen: &mut HashSet<ObjectId>) -> Result<Object> {
        if already_seen.contains(&id) {
            warn!("reference cycle detected resolving object {} {}", id.0, id.1);
            return Err(Error::ReferenceCycle(id));
        }
        already_seen.insert(id);
        let offset = self.get_offset(id)?;
        let (_, obj) = self.read_object(offset as usize, Some(id), already_seen)?;

        Ok(obj)
    }

    fn read_object(
        &self, offset: usize, expected_id: Option<ObjectId>, already_seen: &mut HashSet<ObjectId>,
    ) -> Result<(ObjectId, Object)> {
        if offset > self.buffer.len() {
            return Err(Error::InvalidOffset(offset));
        }

        parser::indirect_object(
            ParserInput::new_extra(self.buffer, "indirect object"),
            offset,
            expected_id,
            self,
            already_seen,
        )
    }

    fn get_xref_start(buffer: &[u8]) -> Result<usize> {
        let seek_pos = buffer.len() - cmp::min(buffer.len(), 512);
        Self::search_substring(buffer, b"%%EOF", seek_pos)
            .and_then(|eof_pos| if eof_pos > 25 { Some(eof_pos) } else { None })
            .and_then(|eof_pos| Self::search_substring(buffer, b"startxref", eof_pos - 25))
            .ok_or(Error::Xref(XrefError::Start))
            .and_then(|xref_pos| {
                if xref_pos <= buffer.len() {
                    match parser::xref_start(ParserInput::new_extra(&buffer[xref_pos..], "xref")) {
                        Some(startxref) => Ok(startxref as usize),
                        None => Err(Error::Xref(XrefError::Start)),
                    }
                } else {
                    Err(Error::Xref(XrefError::Start))
                }
            })
    }

    fn search_substring(buffer: &[u8], pattern: &[u8], start_pos: usize) -> Option<usize> {
        let mut seek_pos = start_pos;
        let mut index = 0;

        while seek_pos < buffer.len() && index < pattern.len() {
            if buffer[seek_pos] == pattern[index] {
                index += 1;
            } else if index > 0 {
                seek_pos -= index;
                index = 0;
            }
            seek_pos += 1;

            if index == pattern.len() {
                let res = seek_pos - index;
                return Self::search_substring(buffer, pattern, res + 1).or(Some(res));
            }
        }

        None
    }

    /// Process images from PDF with a callback, loading them lazily as they're discovered.
    ///
    /// This loads only the structural objects and image XObjects, skipping content streams,
    /// fonts, and other resources. Much faster than loading the full document.
    ///
    /// The callback is invoked immediately when each image is found, allowing for
    /// streaming/progressive display of images rather than batch loading.
    pub fn process_images<F>(mut self, mut callback: F) -> Result<()>
    where
        F: FnMut(crate::xobject::PageImage) -> Result<()>,
    {
        // First, load minimal structure (header, trailer, catalog, pages tree)
        self = Self::load_minimal_structure(self)?;

        // Get all pages
        let pages = self.document.get_pages();

        // For each page, selectively load and process images
        for (page_number, page_id) in pages {
            if let Ok(images) = self.load_page_images(page_id) {
                // First, collect all image data with owned values (this ends the borrow of `images`)
                let image_data: Vec<_> = images.into_iter().map(|image| {
                    (
                        image.id,
                        image.width,
                        image.height,
                        image.color_space.clone(),
                        image.filters.unwrap_or_default(),
                        image.bits_per_component,
                        image.content.to_vec(),
                        image.origin_dict.clone(),
                    )
                }).collect();

                // Now we can access self.document again to extract SMask data
                for (id, width, height, color_space, filters, bits_per_component, content, dict) in image_data {
                    // Create owned PageImage
                    let mut page_image = crate::xobject::PageImage {
                        page_number,
                        page_id,
                        id,
                        width,
                        height,
                        color_space,
                        filters,
                        bits_per_component,
                        content,
                        dict: dict.clone(),
                        // Initialize SMask fields (v0.39.1)
                        smask_content: None,
                        smask_width: None,
                        smask_height: None,
                        smask_filters: None,
                    };

                    // Extract SMask data if available (enhancement for transparency support - v0.39.1)
                    if let Ok(smask_ref) = dict.get(b"SMask") {
                        if let Ok(smask_id) = smask_ref.as_reference() {
                            if let Ok(smask_obj) = self.document.get_object(smask_id) {
                                if let Ok(smask_stream) = smask_obj.as_stream() {
                                    // Extract SMask content
                                    page_image.smask_content = Some(smask_stream.content.clone());

                                    // Extract SMask dimensions
                                    page_image.smask_width = smask_stream
                                        .dict
                                        .get(b"Width")
                                        .ok()
                                        .and_then(|o| o.as_i64().ok());

                                    page_image.smask_height = smask_stream
                                        .dict
                                        .get(b"Height")
                                        .ok()
                                        .and_then(|o| o.as_i64().ok());

                                    // Extract SMask filters
                                    let filters = match smask_stream.dict.get(b"Filter") {
                                        Ok(Object::Name(name)) => {
                                            vec![String::from_utf8_lossy(name).into_owned()]
                                        }
                                        Ok(Object::Array(arr)) => arr
                                            .iter()
                                            .filter_map(|o| o.as_name().ok())
                                            .map(|name| String::from_utf8_lossy(name).into_owned())
                                            .collect(),
                                        _ => Vec::new(),
                                    };
                                    if !filters.is_empty() {
                                        page_image.smask_filters = Some(filters);
                                    }
                                }
                            }
                        }
                    }

                    // Invoke callback immediately
                    callback(page_image)?;
                }
            }
        }

        Ok(())
    }

    /// Load minimal PDF structure (header, trailer, catalog, pages tree only)
    /// This also loads any object streams that contain structural objects
    fn load_minimal_structure(mut self) -> Result<Self> {
        // Parse header and version
        let offset = self.buffer.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
        self.buffer = &self.buffer[offset..];

        let version =
            parser::header(ParserInput::new_extra(self.buffer, "header")).ok_or(ParseError::InvalidFileHeader)?;

        // Parse binary mark
        if let Some(pos) = self.buffer.iter().position(|&byte| byte == b'\n') {
            if let Some(binary_mark) =
                parser::binary_mark(ParserInput::new_extra(&self.buffer[pos + 1..], "binary_mark"))
            {
                if binary_mark.iter().all(|&byte| byte >= 128) {
                    self.document.binary_mark = binary_mark;
                }
            }
        }

        // Parse xref and trailer
        let xref_start = Self::get_xref_start(self.buffer)?;
        if xref_start > self.buffer.len() {
            return Err(Error::Xref(XrefError::Start));
        }
        self.document.xref_start = xref_start;

        let (mut xref, mut trailer) =
            parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[xref_start..], "xref"), &self)?;

        // Read previous Xrefs
        let mut already_seen = HashSet::new();
        let mut prev_xref_start = trailer.remove(b"Prev");
        while let Some(prev) = prev_xref_start.and_then(|offset| offset.as_i64().ok()) {
            if already_seen.contains(&prev) {
                break;
            }
            already_seen.insert(prev);
            if prev < 0 || prev as usize > self.buffer.len() {
                return Err(Error::Xref(XrefError::PrevStart));
            }

            let (prev_xref, prev_trailer) =
                parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
            xref.merge(prev_xref);

            let prev_xref_stream_start = trailer.remove(b"XRefStm");
            if let Some(prev) = prev_xref_stream_start.and_then(|offset| offset.as_i64().ok()) {
                if prev < 0 || prev as usize > self.buffer.len() {
                    return Err(Error::Xref(XrefError::StreamStart));
                }

                let (prev_xref, _) =
                    parser::xref_and_trailer(ParserInput::new_extra(&self.buffer[prev as usize..], ""), &self)?;
                xref.merge(prev_xref);
            }

            prev_xref_start = prev_trailer.get(b"Prev").cloned().ok();
        }

        let xref_entry_count = xref.max_id().checked_add(1).ok_or(ParseError::InvalidXref)?;
        if xref.size != xref_entry_count {
            xref.size = xref_entry_count;
        }

        self.document.version = version;
        self.document.max_id = xref.size - 1;
        self.document.trailer = trailer.clone();
        self.document.reference_table = xref;

        // DON'T load all object streams upfront - load them on-demand instead!
        // This is the key fix for performance with compressed PDFs

        // Load catalog (will load its object stream on-demand if needed)
        if let Ok(catalog_id) = trailer.get(b"Root").and_then(Object::as_reference) {
            self.load_object_if_needed(catalog_id)?;

            // Load Pages tree
            if let Ok(catalog_dict) = self.document.get_dictionary(catalog_id) {
                if let Ok(pages_id) = catalog_dict.get(b"Pages").and_then(Object::as_reference) {
                    self.load_pages_tree(pages_id)?;
                }
            }
        }

        Ok(self)
    }

    /// Load all object streams in the document
    /// This is needed for compressed PDFs where structural objects might be in object streams
    fn load_all_object_streams(&mut self) -> Result<()> {
        use crate::object_stream::ObjectStream;

        let is_encrypted = self.document.trailer.get(b"Encrypt").is_ok();
        if is_encrypted {
            // Don't try to load object streams for encrypted PDFs
            return Ok(());
        }

        // Collect all object stream IDs
        let mut objstm_ids = Vec::new();
        for (id, entry) in &self.document.reference_table.entries {
            if let XrefEntry::Normal { offset, generation } = entry {
                if *generation == 0 {
                    // Try to peek at the object to see if it's an ObjStm
                    if let Ok((_, obj)) = self.read_object(*offset as usize, Some((*id, 0)), &mut HashSet::new()) {
                        if let Ok(stream) = obj.as_stream() {
                            if stream.dict.has_type(b"ObjStm") {
                                objstm_ids.push((*id, obj));
                            }
                        }
                    }
                }
            }
        }

        // Load objects from each object stream
        for (objstm_id, mut objstm_obj) in objstm_ids {
            if let Ok(stream) = objstm_obj.as_stream_mut() {
                if let Ok(obj_stream) = ObjectStream::new(stream) {
                    // Insert all objects from this stream
                    for (id, obj) in obj_stream.objects {
                        self.document.objects.entry(id).or_insert(obj);
                    }
                }
            }
            // Also store the object stream itself (objstm_id is already an ObjectId tuple)
            self.document.objects.insert((objstm_id, 0), objstm_obj);
        }

        Ok(())
    }

    /// Load an object if it's not already loaded (handles both normal and object stream objects)
    fn load_object_if_needed(&mut self, id: ObjectId) -> Result<()> {
        // Check if already loaded
        if self.document.objects.contains_key(&id) {
            return Ok(());
        }

        // Get the xref entry for this object
        let entry = self.document.reference_table.get(id.0).ok_or(Error::MissingXrefEntry)?;

        match *entry {
            XrefEntry::Normal { offset, generation } if generation == id.1 => {
                // Normal object - load it directly from offset
                let (_, obj) = self.read_object(offset as usize, Some(id), &mut HashSet::new())?;
                self.document.objects.insert(id, obj);
            }
            XrefEntry::Compressed { container, index: _ } => {
                // Object is compressed in an object stream
                // Load the object stream on-demand (if not already loaded)
                self.load_object_stream_if_needed(container)?;
                // The object should now be in document.objects
                // If it's not there, that's an error in the PDF
            }
            _ => return Err(Error::MissingXrefEntry),
        }

        Ok(())
    }

    /// Load a specific object stream on-demand (only if not already loaded)
    fn load_object_stream_if_needed(&mut self, objstm_id: u32) -> Result<()> {
        use crate::object_stream::ObjectStream;

        let objstm_obj_id = (objstm_id, 0);

        // Check if this object stream is already loaded
        if self.document.objects.contains_key(&objstm_obj_id) {
            return Ok(()); // Already loaded
        }

        // Get the xref entry for the object stream
        let entry = self.document.reference_table.get(objstm_id).ok_or(Error::MissingXrefEntry)?;

        if let XrefEntry::Normal { offset, generation } = *entry {
            if generation == 0 {
                // Load the object stream
                let (_, mut obj) = self.read_object(offset as usize, Some(objstm_obj_id), &mut HashSet::new())?;

                // Check if it's actually an object stream
                if let Ok(stream) = obj.as_stream_mut() {
                    if stream.dict.has_type(b"ObjStm") {
                        // Decompress and extract all objects from this stream
                        if let Ok(obj_stream) = ObjectStream::new(stream) {
                            for (id, obj) in obj_stream.objects {
                                self.document.objects.entry(id).or_insert(obj);
                            }
                        }
                    }
                }

                // Store the object stream itself
                self.document.objects.insert(objstm_obj_id, obj);
            }
        }

        Ok(())
    }

    /// Load images from a specific page by loading only necessary objects
    fn load_page_images(&mut self, page_id: ObjectId) -> Result<Vec<crate::xobject::PdfImage<'_>>> {
        use crate::xobject::PdfImage;

        let mut images = Vec::new();

        // First, get the resources object (clone to avoid borrow issues)
        let resources_obj = self.document.get_dictionary(page_id)
            .and_then(|page| page.get(b"Resources"))
            .ok()
            .cloned();

        let resources_id = match resources_obj {
            Some(Object::Reference(res_id)) => {
                // Load the resources object if not already loaded
                let _ = self.load_object_if_needed(res_id);
                Some(res_id)
            }
            _ => None,
        };

        // Now get the XObject reference from resources (clone to avoid borrow issues)
        let xobject_obj = if let Some(res_id) = resources_id {
            self.document.get_dictionary(res_id)
                .and_then(|res| res.get(b"XObject"))
                .ok()
                .cloned()
        } else if let Some(Object::Dictionary(dict)) = resources_obj {
            dict.get(b"XObject").ok().cloned()
        } else {
            None
        };

        let xobject_id = match xobject_obj {
            Some(Object::Reference(xobj_id)) => {
                // Load the XObject dict if not already loaded
                let _ = self.load_object_if_needed(xobj_id);
                Some(xobj_id)
            }
            _ => None,
        };

        // Collect image IDs from the XObject dictionary
        let mut image_ids = Vec::new();
        if let Some(xobj_id) = xobject_id {
            if let Ok(xobject) = self.document.get_dictionary(xobj_id) {
                for (_, xvalue) in xobject.iter() {
                    if let Ok(id) = xvalue.as_reference() {
                        image_ids.push(id);
                    }
                }
            }
        } else if let Some(Object::Dictionary(dict)) = xobject_obj {
            for (_, xvalue) in dict.iter() {
                if let Ok(id) = xvalue.as_reference() {
                    image_ids.push(id);
                }
            }
        }

        // First, load all image objects (mutation phase)
        for id in &image_ids {
            // Load the image stream if not already loaded
            let _ = self.load_object_if_needed(*id);

            // Load SMask if referenced (enhancement for transparency support - v0.39.1)
            if let Ok(stream) = self.document.get_object(*id).and_then(Object::as_stream) {
                if let Ok(smask_ref) = stream.dict.get(b"SMask") {
                    if let Ok(smask_id) = smask_ref.as_reference() {
                        // Load SMask object
                        let _ = self.load_object_if_needed(smask_id);

                        // Load SMask stream content if empty
                        if let Ok(smask_stream) = self.document.get_object(smask_id).and_then(Object::as_stream) {
                            if smask_stream.content.is_empty() {
                                let _ = self.read_stream_content(smask_id);
                            }
                        }
                    }
                }
            }

            // Load image stream content if needed
            if let Ok(stream) = self.document.get_object(*id).and_then(Object::as_stream) {
                if stream.content.is_empty() {
                    let _ = self.read_stream_content(*id);
                }
            }
        }

        // Now extract image information (borrow phase)
        for id in image_ids {
            if let Ok(xvalue) = self.document.get_object(id) {
                if let Ok(xvalue) = xvalue.as_stream() {
                    let dict = &xvalue.dict;
                    if dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image") {
                        let width = dict.get(b"Width").and_then(Object::as_i64).ok();
                        let height = dict.get(b"Height").and_then(Object::as_i64).ok();

                        if let (Some(width), Some(height)) = (width, height) {
                            let color_space = match dict.get(b"ColorSpace") {
                                Ok(cs) => match cs {
                                    Object::Array(array) => array.first()
                                        .and_then(|obj| obj.as_name().ok())
                                        .map(|n| String::from_utf8_lossy(n).to_string()),
                                    Object::Name(name) => Some(String::from_utf8_lossy(name).to_string()),
                                    _ => None,
                                },
                                Err(_) => None,
                            };

                            let bits_per_component = dict.get(b"BitsPerComponent")
                                .and_then(Object::as_i64)
                                .ok();

                            let mut filters = Vec::new();
                            if let Ok(filter) = dict.get(b"Filter") {
                                match filter {
                                    Object::Array(array) => {
                                        for obj in array.iter() {
                                            if let Ok(name) = obj.as_name() {
                                                filters.push(String::from_utf8_lossy(name).to_string());
                                            }
                                        }
                                    }
                                    Object::Name(name) => {
                                        filters.push(String::from_utf8_lossy(name).to_string());
                                    }
                                    _ => {}
                                }
                            }

                            images.push(PdfImage {
                                id,
                                width,
                                height,
                                color_space,
                                bits_per_component,
                                filters: Some(filters),
                                content: &xvalue.content,
                                origin_dict: &xvalue.dict,
                            });
                        }
                    }
                }
            }
        }

        Ok(images)
    }
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn load_document() {
    let mut doc = Document::load("assets/example.pdf").unwrap();
    assert_eq!(doc.version, "1.5");

    // Create temporary folder to store file.
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_2_load.pdf");
    doc.save(file_path).unwrap();
}

#[cfg(all(test, feature = "async"))]
#[tokio::test]
async fn load_document() {
    let mut doc = Document::load("assets/example.pdf").await.unwrap();
    assert_eq!(doc.version, "1.5");

    // Create temporary folder to store file.
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_2_load.pdf");
    doc.save(file_path).unwrap();
}

#[test]
#[should_panic(expected = "Xref(Start)")]
fn load_short_document() {
    let _doc = Document::load_mem(b"%PDF-1.5\n%%EOF\n").unwrap();
}

#[test]
fn load_document_with_preceding_bytes() {
    let mut content = Vec::new();
    content.extend(b"garbage");
    content.extend(include_bytes!("../assets/example.pdf"));
    let doc = Document::load_mem(&content).unwrap();
    assert_eq!(doc.version, "1.5");
}

#[test]
fn load_many_shallow_brackets() {
    let content: String = std::iter::repeat("()")
        .take(MAX_BRACKET * 10)
        .flat_map(|x| x.chars())
        .collect();
    const STREAM_CRUFT: usize = 33;
    let doc = format!(
        "%PDF-1.5
1 0 obj<</Type/Pages/Kids[5 0 R]/Count 1/Resources 3 0 R/MediaBox[0 0 595 842]>>endobj
2 0 obj<</Type/Font/Subtype/Type1/BaseFont/Courier>>endobj
3 0 obj<</Font<</F1 2 0 R>>>>endobj
5 0 obj<</Type/Page/Parent 1 0 R/Contents[4 0 R]>>endobj
6 0 obj<</Type/Catalog/Pages 1 0 R>>endobj
4 0 obj<</Length {}>>stream
BT
/F1 48 Tf
100 600 Td
({}) Tj
ET
endstream endobj\n",
        content.len() + STREAM_CRUFT,
        content
    );
    let doc = format!(
        "{}xref
0 7
0000000000 65535 f 
0000000009 00000 n 
0000000096 00000 n 
0000000155 00000 n 
0000000291 00000 n 
0000000191 00000 n 
0000000248 00000 n 
trailer
<</Root 6 0 R/Size 7>>
startxref
{}
%%EOF",
        doc,
        doc.len()
    );

    let _doc = Document::load_mem(doc.as_bytes()).unwrap();
}

#[test]
fn load_too_deep_brackets() {
    let content: Vec<u8> = std::iter::repeat(b'(')
        .take(MAX_BRACKET + 1)
        .chain(std::iter::repeat(b')').take(MAX_BRACKET + 1))
        .collect();
    let content = String::from_utf8(content).unwrap();
    const STREAM_CRUFT: usize = 33;
    let doc = format!(
        "%PDF-1.5
1 0 obj<</Type/Pages/Kids[5 0 R]/Count 1/Resources 3 0 R/MediaBox[0 0 595 842]>>endobj
2 0 obj<</Type/Font/Subtype/Type1/BaseFont/Courier>>endobj
3 0 obj<</Font<</F1 2 0 R>>>>endobj
5 0 obj<</Type/Page/Parent 1 0 R/Contents[7 0 R 4 0 R]>>endobj
6 0 obj<</Type/Catalog/Pages 1 0 R>>endobj
7 0 obj<</Length 45>>stream
BT /F1 48 Tf 100 600 Td (Hello World!) Tj ET
endstream
endobj
4 0 obj<</Length {}>>stream
BT
/F1 48 Tf
100 600 Td
({}) Tj
ET
endstream endobj\n",
        content.len() + STREAM_CRUFT,
        content
    );
    let doc = format!(
        "{}xref
0 7
0000000000 65535 f 
0000000009 00000 n 
0000000096 00000 n 
0000000155 00000 n 
0000000387 00000 n 
0000000191 00000 n 
0000000254 00000 n 
0000000297 00000 n 
trailer
<</Root 6 0 R/Size 7>>
startxref
{}
%%EOF",
        doc,
        doc.len()
    );

    let doc = Document::load_mem(doc.as_bytes()).unwrap();
    let pages = doc.get_pages().keys().cloned().collect::<Vec<_>>();
    assert_eq!("Hello World!\n", doc.extract_text(&pages).unwrap());
}
