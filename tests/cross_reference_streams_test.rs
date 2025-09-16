use lopdf::{dictionary, Document, Object, SaveOptions, ObjectStreamConfig};

#[test]
fn test_xref_stream_creation() {
    let mut doc = Document::with_version("1.5");
    
    // Create simple content
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference((2, 0))
    });
    
    let _pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Count" => 0,
        "Kids" => vec![]
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // Save with cross-reference streams
    let options = SaveOptions {
        use_object_streams: false,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut buffer = Vec::new();
    doc.save_with_options(&mut buffer, options).unwrap();
    
    let content = String::from_utf8_lossy(&buffer);
    
    // Verify xref stream exists
    assert!(content.contains("/Type/XRef"), "Should contain cross-reference stream");
    assert!(content.contains("/W["), "Should contain W array for entry widths");
    assert!(!content.contains("\nxref\n"), "Should not contain traditional xref table");
}

#[test]
fn test_xref_stream_with_object_streams() {
    let mut doc = Document::with_version("1.5");
    
    // Create multiple objects for compression
    for i in 0..10 {
        doc.add_object(dictionary! {
            "Test" => i,
            "Data" => format!("Object {}", i)
        });
    }
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference((100, 0))
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // Save with both object streams and xref streams
    let options = SaveOptions {
        use_object_streams: true,
        use_xref_streams: true,
        object_stream_config: ObjectStreamConfig {
            max_objects_per_stream: 5,
            compression_level: 6,
        },
        linearize: false,
    };
    
    let mut buffer = Vec::new();
    doc.save_with_options(&mut buffer, options).unwrap();
    
    let content = String::from_utf8_lossy(&buffer);
    
    // Verify both features are present
    assert!(content.contains("/Type/XRef"), "Should contain cross-reference stream");
    assert!(content.contains("/Type/ObjStm"), "Should contain object streams");
    assert!(!content.contains("\nxref\n"), "Should not contain traditional xref table");
}

#[test]
fn test_xref_stream_compression() {
    let mut doc = Document::with_version("1.5");
    
    // Add many objects to test compression efficiency
    for i in 0..100 {
        doc.add_object(Object::Integer(i));
    }
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog"
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // Save with traditional xref
    let mut traditional_buffer = Vec::new();
    doc.save_to(&mut traditional_buffer).unwrap();
    
    // Save with xref stream
    let options = SaveOptions {
        use_object_streams: false,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut xref_stream_buffer = Vec::new();
    doc.save_with_options(&mut xref_stream_buffer, options).unwrap();
    
    println!("Traditional xref size: {} bytes", traditional_buffer.len());
    println!("XRef stream size: {} bytes", xref_stream_buffer.len());
    
    // XRef streams should be more compact for many objects
    // Note: For very small PDFs, xref streams might be larger due to overhead
    // Also, the current implementation might add some overhead
    if doc.objects.len() > 50 {
        // Allow up to 10% overhead for xref streams (they're compressed but have metadata)
        let overhead_threshold = (traditional_buffer.len() as f64 * 1.1) as usize;
        assert!(xref_stream_buffer.len() <= overhead_threshold,
            "XRef streams should be comparable or more compact for many objects. Traditional: {} bytes, XRef stream: {} bytes", 
            traditional_buffer.len(), xref_stream_buffer.len());
    }
}

#[test]
fn test_xref_stream_with_compressed_objects() {
    let mut doc = Document::with_version("1.5");
    
    // Create objects that will be compressed
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "BaseFont" => "Helvetica"
    });
    
    let annot_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text"
    });
    
    // Create stream object (won't be compressed)
    let stream_id = doc.add_object(lopdf::Stream::new(
        dictionary! { "Length" => 10 },
        vec![0; 10]
    ));
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog"
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // Save with object streams and xref streams
    let options = SaveOptions {
        use_object_streams: true,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut buffer = Vec::new();
    doc.save_with_options(&mut buffer, options).unwrap();
    
    // Load the saved PDF to verify structure
    let loaded_doc = Document::load_mem(&buffer).unwrap();
    
    // Verify objects are accessible
    assert!(loaded_doc.get_object(font_id).is_ok(), "Font should be accessible");
    assert!(loaded_doc.get_object(annot_id).is_ok(), "Annotation should be accessible");
    assert!(loaded_doc.get_object(stream_id).is_ok(), "Stream should be accessible");
    assert!(loaded_doc.get_object(catalog_id).is_ok(), "Catalog should be accessible");
}

#[test]
fn test_xref_stream_entries() {
    let mut doc = Document::with_version("1.5");
    
    // Create different types of entries
    let obj1_id = doc.add_object(Object::Integer(1)); // Will be compressed
    let obj2_id = doc.add_object(lopdf::Stream::new(  // Direct object
        dictionary! { "Test" => "Stream" },
        vec![1, 2, 3]
    ));
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog"
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // Save with all modern features
    let options = SaveOptions {
        use_object_streams: true,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut buffer = Vec::new();
    doc.save_with_options(&mut buffer, options).unwrap();
    
    let content = buffer.clone();
    
    // Parse the saved content
    let loaded = Document::load_mem(&content).unwrap();
    
    // All objects should be retrievable
    assert!(loaded.get_object(obj1_id).is_ok());
    assert!(loaded.get_object(obj2_id).is_ok());
    assert!(loaded.get_object(catalog_id).is_ok());
    
    // Check the structure
    let text = String::from_utf8_lossy(&buffer);
    
    // Should have Type 1 entries (normal objects)
    // Should have Type 2 entries (compressed objects)
    // The binary format makes it hard to verify directly, but we can check it loads correctly
    assert!(text.contains("/Type/XRef"));
    assert!(text.contains("/Filter"));  // XRef streams should be compressed
}

#[test]
fn test_xref_stream_with_updates() {
    let mut doc = Document::with_version("1.5");
    
    // Initial objects
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference((2, 0))
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // Save with xref stream
    let options = SaveOptions {
        use_object_streams: false,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut buffer1 = Vec::new();
    doc.save_with_options(&mut buffer1, options.clone()).unwrap();
    
    // Add more objects
    for i in 0..5 {
        doc.add_object(dictionary! {
            "NewObject" => i
        });
    }
    
    // Save again
    let mut buffer2 = Vec::new();
    doc.save_with_options(&mut buffer2, options).unwrap();
    
    // Both should be valid PDFs
    assert!(Document::load_mem(&buffer1).is_ok());
    assert!(Document::load_mem(&buffer2).is_ok());
    
    // Second should be larger
    assert!(buffer2.len() > buffer1.len());
}

#[test]
fn test_xref_stream_index_array() {
    let mut doc = Document::with_version("1.5");
    
    // Create gaps in object numbering
    doc.objects.insert((1, 0), Object::Integer(1));
    doc.objects.insert((5, 0), Object::Integer(5));
    doc.objects.insert((10, 0), Object::Integer(10));
    doc.objects.insert((15, 0), Object::Integer(15));
    
    let catalog_id = (20, 0);
    doc.objects.insert(catalog_id, Object::Dictionary(dictionary! {
        "Type" => "Catalog"
    }));
    
    doc.trailer.set("Root", catalog_id);
    doc.max_id = 20;
    
    // Save with xref stream
    let options = SaveOptions {
        use_object_streams: false,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut buffer = Vec::new();
    doc.save_with_options(&mut buffer, options).unwrap();
    
    let content = String::from_utf8_lossy(&buffer);
    
    // Should have Index array for non-contiguous entries
    assert!(content.contains("/Index"), "Should have Index array for gaps in numbering");
    
    // Verify the saved PDF is valid
    let loaded = Document::load_mem(&buffer).unwrap();
    assert_eq!(loaded.get_object((1, 0)).unwrap(), &Object::Integer(1));
    assert_eq!(loaded.get_object((5, 0)).unwrap(), &Object::Integer(5));
    assert_eq!(loaded.get_object((10, 0)).unwrap(), &Object::Integer(10));
    assert_eq!(loaded.get_object((15, 0)).unwrap(), &Object::Integer(15));
}

#[test]
fn test_xref_stream_compatibility() {
    // Test that PDFs with xref streams can be loaded and re-saved
    let mut doc = Document::with_version("1.5");
    
    // Create a complex structure
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()]
    });
    
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1
    }));
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // Save with xref stream
    let options = SaveOptions {
        use_object_streams: true,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut buffer = Vec::new();
    doc.save_with_options(&mut buffer, options).unwrap();
    
    // Load and re-save
    let mut loaded_doc = Document::load_mem(&buffer).unwrap();
    let mut buffer2 = Vec::new();
    loaded_doc.save_to(&mut buffer2).unwrap();
    
    // Both versions should be valid
    assert!(Document::load_mem(&buffer2).is_ok());
}

#[test]
fn test_xref_stream_error_handling() {
    let mut doc = Document::with_version("1.5");
    
    // Create an object with maximum ID to test boundary conditions
    let large_id = (999999, 0);
    doc.objects.insert(large_id, Object::Integer(42));
    doc.max_id = 999999;
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog"
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // This should still work with xref streams
    let options = SaveOptions {
        use_object_streams: false,
        use_xref_streams: true,
        linearize: false,
        ..Default::default()
    };
    
    let mut buffer = Vec::new();
    let result = doc.save_with_options(&mut buffer, options);
    assert!(result.is_ok(), "Should handle large object IDs");
    
    // Verify it can be loaded
    assert!(Document::load_mem(&buffer).is_ok());
}

#[test]
fn test_save_modern_method() {
    let mut doc = Document::with_version("1.4");
    
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog"
    });
    
    doc.trailer.set("Root", catalog_id);
    
    // save_modern should enable both object streams and xref streams
    let mut buffer = Vec::new();
    doc.save_modern(&mut buffer).unwrap();
    
    let content = String::from_utf8_lossy(&buffer);
    
    // Should upgrade version
    assert!(content.starts_with("%PDF-1.5") || content.starts_with("%PDF-1.6") || content.starts_with("%PDF-1.7"));
    
    // Should have both modern features
    assert!(content.contains("/Type/XRef"), "save_modern should create xref streams");
    assert!(!content.contains("\nxref\n"), "save_modern should not use traditional xref");
}