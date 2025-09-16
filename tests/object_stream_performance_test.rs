use lopdf::{dictionary, Document, Object, ObjectStream, SaveOptions};
use std::time::Instant;

#[test]
fn test_can_be_compressed_performance() {
    // Create a document with many objects
    let mut doc = Document::with_version("1.5");
    let mut object_ids = Vec::new();
    
    // Create 1000 objects
    for i in 0..1000 {
        let obj_id = doc.add_object(dictionary! {
            "Type" => format!("TestObject{}", i),
            "Index" => i as i64,
            "Data" => format!("This is test object number {}", i)
        });
        object_ids.push(obj_id);
    }
    
    // Add some to trailer
    doc.trailer.set("Root", object_ids[0]);
    doc.trailer.set("Info", object_ids[1]);
    doc.trailer.set("Custom1", object_ids[2]);
    doc.trailer.set("Custom2", object_ids[3]);
    
    // Measure performance of can_be_compressed checks
    let start = Instant::now();
    let mut compressible_count = 0;
    
    for &id in &object_ids {
        if let Some(obj) = doc.objects.get(&id) {
            if ObjectStream::can_be_compressed(id, obj, &doc) {
                compressible_count += 1;
            }
        }
    }
    
    let duration = start.elapsed();
    
    println!("Checked {} objects in {:?}", object_ids.len(), duration);
    println!("Compressible objects: {}", compressible_count);
    
    // All objects should be compressible (none are encryption dicts)
    assert_eq!(compressible_count, object_ids.len(), 
               "All non-encryption objects should be compressible");
    
    // Performance check: should complete in reasonable time
    assert!(duration.as_millis() < 100, 
            "Performance check took too long: {:?}", duration);
}

#[test]
fn test_save_performance_with_trailer_objects() {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    
    // Create many pages
    let mut page_ids = Vec::new();
    for i in 0..100 {
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference((1000 + i, 0))
        });
        page_ids.push(page_id);
    }
    
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
        "Count" => page_ids.len() as i64
    }));
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id
    });
    
    let info_id = doc.add_object(dictionary! {
        "Title" => "Performance Test PDF",
        "PageCount" => page_ids.len() as i64
    });
    
    // Add many custom entries to trailer
    doc.trailer.set("Root", catalog_id);
    doc.trailer.set("Info", info_id);
    for i in 0..10 {
        doc.trailer.set(format!("Custom{}", i).as_bytes(), Object::Integer(i));
    }
    
    // Measure save performance
    let options = SaveOptions::builder()
        .use_object_streams(true)
        .build();
    
    let start = Instant::now();
    let mut output = Vec::new();
    doc.save_with_options(&mut output, options).unwrap();
    let save_duration = start.elapsed();
    
    println!("Saved {} page PDF in {:?}", page_ids.len(), save_duration);
    println!("Output size: {} bytes", output.len());
    
    // Should complete quickly even with many objects
    assert!(save_duration.as_millis() < 500, 
            "Save took too long: {:?}", save_duration);
    
    // Verify compression worked
    let content = String::from_utf8_lossy(&output);
    assert!(content.contains("/ObjStm"), "Object streams should be created");
}

#[test]
fn test_encryption_check_performance() {
    // Test the specific encryption dictionary check performance
    let mut doc = Document::with_version("1.5");
    
    // Create an encryption dictionary
    let encrypt_id = doc.add_object(dictionary! {
        "Filter" => "Standard",
        "V" => 2
    });
    
    // Set it in trailer
    doc.trailer.set("Encrypt", encrypt_id);
    
    // Create many other objects
    let mut other_ids = Vec::new();
    for i in 0..1000 {
        let id = doc.add_object(Object::Integer(i));
        other_ids.push(id);
    }
    
    // Time the encryption check
    let start = Instant::now();
    
    // Check encryption dictionary
    let encrypt_obj = doc.objects.get(&encrypt_id).unwrap();
    let encrypt_compressible = ObjectStream::can_be_compressed(encrypt_id, encrypt_obj, &doc);
    
    // Check many non-encryption objects
    for &id in &other_ids[..100] {  // Check first 100
        if let Some(obj) = doc.objects.get(&id) {
            let _ = ObjectStream::can_be_compressed(id, obj, &doc);
        }
    }
    
    let duration = start.elapsed();
    
    println!("Encryption check performance: {:?} for 101 objects", duration);
    
    assert!(!encrypt_compressible, "Encryption dict should not be compressible");
    assert!(duration.as_micros() < 1000, "Check should be very fast");
}

#[test]
#[ignore] // Run with: cargo test --ignored -- benchmark_object_stream_scaling
fn benchmark_object_stream_scaling() {
    println!("\n=== Object Stream Scaling Test ===");
    
    let object_counts = vec![10, 50, 100, 500, 1000, 5000];
    
    for count in object_counts {
        let mut doc = Document::with_version("1.5");
        
        // Create many objects
        for i in 0..count {
            doc.add_object(dictionary! {
                "Type" => "TestObject",
                "ID" => i as i64,
                "Data" => format!("Object {} with some test data", i),
                "Nested" => dictionary! {
                    "Value" => i * 2,
                    "String" => format!("Nested {}", i)
                }
            });
        }
        
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog"
        });
        doc.trailer.set("Root", catalog_id);
        
        // Time save with object streams
        let start = Instant::now();
        let mut buffer = Vec::new();
        doc.save_modern(&mut buffer).unwrap();
        let duration = start.elapsed();
        
        let objects_per_second = count as f64 / duration.as_secs_f64();
        
        println!("Objects: {:5} | Time: {:8.3}ms | Objects/sec: {:10.0} | Size: {:8} bytes", 
                 count, duration.as_secs_f64() * 1000.0, objects_per_second, buffer.len());
    }
}

#[test] 
#[ignore] // Run with: cargo test --ignored -- benchmark_compression_ratio
fn benchmark_compression_ratio() {
    println!("\n=== Object Stream Compression Ratio ===");
    
    // Create documents with different content types
    let test_cases = vec![
        ("Text-heavy", create_text_heavy_doc()),
        ("Numeric data", create_numeric_doc()),
        ("Mixed content", create_mixed_doc()),
        ("Repetitive", create_repetitive_doc()),
    ];
    
    for (name, mut doc) in test_cases {
        // Save without object streams
        let mut traditional = Vec::new();
        doc.save_to(&mut traditional).unwrap();
        
        // Save with object streams
        let mut modern = Vec::new();
        doc.save_modern(&mut modern).unwrap();
        
        let ratio = modern.len() as f64 / traditional.len() as f64;
        let reduction = (1.0 - ratio) * 100.0;
        
        println!("{:15} - Traditional: {:6} bytes, Modern: {:6} bytes, Reduction: {:.1}%",
                 name, traditional.len(), modern.len(), reduction);
    }
}

fn create_text_heavy_doc() -> Document {
    let mut doc = Document::with_version("1.4");
    
    for i in 0..100 {
        doc.add_object(dictionary! {
            "Type" => "Annotation",
            "Contents" => format!("This is a long text annotation number {} with detailed content", i),
            "Subject" => "Document Review",
            "Author" => format!("Reviewer {}", i % 10),
            "CreationDate" => "D:20250101120000Z"
        });
    }
    
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog" });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn create_numeric_doc() -> Document {
    let mut doc = Document::with_version("1.4");
    
    for i in 0..100 {
        doc.add_object(dictionary! {
            "Values" => vec![
                Object::Integer(i),
                Object::Real((i as f64 * 3.14159) as f32),
                Object::Integer(i * 100),
                Object::Real((i as f64).sqrt() as f32),
            ],
            "Matrix" => vec![1.0.into(), 0.0.into(), 0.0.into(), 1.0.into(), 
                           (i as f64).into(), (i as f64 * 2.0).into()]
        });
    }
    
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog" });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn create_mixed_doc() -> Document {
    let mut doc = Document::with_version("1.4");
    
    for i in 0..100 {
        if i % 3 == 0 {
            doc.add_object(dictionary! {
                "Type" => "Font",
                "BaseFont" => format!("CustomFont{}", i),
                "Encoding" => "WinAnsiEncoding"
            });
        } else if i % 3 == 1 {
            doc.add_object(dictionary! {
                "Type" => "ExtGState",
                "CA" => 0.5,
                "ca" => 0.8,
                "BM" => "/Normal"
            });
        } else {
            doc.add_object(vec![
                Object::Integer(i),
                Object::Name(format!("Name{}", i).into_bytes()),
                Object::String(format!("String{}", i).into_bytes(), lopdf::StringFormat::Literal)
            ]);
        }
    }
    
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog" });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn create_repetitive_doc() -> Document {
    let mut doc = Document::with_version("1.4");
    
    // Create objects with repetitive content
    for i in 0..100 {
        doc.add_object(dictionary! {
            "Type" => "Pattern",
            "PatternType" => 1,
            "PaintType" => 1,
            "TilingType" => 1,
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "XStep" => 100,
            "YStep" => 100,
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec())]
            }
        });
    }
    
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog" });
    doc.trailer.set("Root", catalog_id);
    doc
}

#[test]
#[ignore] // Run with: cargo test --ignored -- benchmark_memory_efficiency
fn benchmark_memory_efficiency() {
    println!("\n=== Memory Efficiency Test ===");
    
    // Test different object stream configurations
    let configs = vec![
        (10, 1000),   // Few large streams
        (100, 100),   // Balanced
        (1000, 10),   // Many small streams
    ];
    
    for (num_streams, objects_per_stream) in configs {
        let mut doc = Document::with_version("1.5");
        let total_objects = num_streams * objects_per_stream;
        
        // Add objects
        for i in 0..total_objects {
            doc.add_object(dictionary! {
                "ID" => i as i64,
                "Data" => Object::String(vec![0u8; 100], lopdf::StringFormat::Literal) // 100 bytes per object
            });
        }
        
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog" });
        doc.trailer.set("Root", catalog_id);
        
        // Configure object streams
        let options = SaveOptions {
            use_object_streams: true,
            use_xref_streams: true,
            object_stream_config: lopdf::ObjectStreamConfig {
                max_objects_per_stream: objects_per_stream,
                compression_level: 6,
            },
            linearize: false,
        };
        
        let start = Instant::now();
        let mut buffer = Vec::new();
        doc.save_with_options(&mut buffer, options).unwrap();
        let duration = start.elapsed();
        
        println!("Config: {} streams × {} objects | Time: {:.3}ms | Size: {} bytes | Bytes/object: {:.1}",
                 num_streams, objects_per_stream, 
                 duration.as_secs_f64() * 1000.0,
                 buffer.len(),
                 buffer.len() as f64 / total_objects as f64);
    }
}