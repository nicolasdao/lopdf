/// Example demonstrating fast PDF metadata extraction using load_minimal()
///
/// This example shows how to quickly extract basic PDF information
/// (version, page count, metadata) without loading the entire document.
///
/// Usage: cargo run --example minimal_metadata_extraction --features default -- <pdf_file>

use lopdf::Document;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <pdf_file>", args[0]);
        eprintln!("\nExample: {} document.pdf", args[0]);
        std::process::exit(1);
    }
    let pdf_path = &args[1];

    println!("=== Fast PDF Metadata Extraction ===\n");
    println!("File: {}\n", pdf_path);

    // Load only metadata (fast!)
    let start = Instant::now();
    let doc = Document::load_minimal(pdf_path)?;
    let duration = start.elapsed();

    // Extract metadata
    println!("PDF Version: {}", doc.version);
    println!("Page Count: {}", doc.get_pages().len());
    println!("Objects Loaded: {}", doc.objects.len());

    // Try to extract Info dictionary metadata
    if let Ok(info_id) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Ok(info) = doc.get_dictionary(info_id) {
            println!("\nMetadata:");

            // Common metadata fields
            let fields: Vec<(&[u8], &str)> = vec![
                (b"Title".as_slice(), "Title"),
                (b"Author".as_slice(), "Author"),
                (b"Subject".as_slice(), "Subject"),
                (b"Creator".as_slice(), "Creator"),
                (b"Producer".as_slice(), "Producer"),
                (b"CreationDate".as_slice(), "Created"),
                (b"ModDate".as_slice(), "Modified"),
            ];

            for &(key, label) in &fields {
                if let Ok(value_obj) = info.get(key) {
                    if let Ok(value) = value_obj.as_str() {
                        println!("  {}: {}", label, String::from_utf8_lossy(value));
                    }
                }
            }
        }
    }

    println!("\nTime taken: {:?}", duration);
    println!("✓ Metadata extracted successfully!");

    Ok(())
}
