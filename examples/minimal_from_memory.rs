/// Example: Fast metadata extraction from in-memory PDF
///
/// Usage: cargo run --example minimal_from_memory --features default -- <pdf_file>

use lopdf::Document;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <pdf_file>", args[0]);
        std::process::exit(1);
    }
    let pdf_path = &args[1];

    println!("=== In-Memory PDF Metadata Extraction ===\n");

    // Step 1: Read PDF into memory
    println!("Loading PDF into memory...");
    let start = Instant::now();
    let pdf_bytes = std::fs::read(pdf_path)?;
    let read_time = start.elapsed();
    println!("Read {} bytes in {:?}\n", pdf_bytes.len(), read_time);

    // Step 2: Extract metadata from memory (fast!)
    println!("Extracting metadata from memory...");
    let start = Instant::now();
    let doc = Document::load_minimal_mem(&pdf_bytes)?;
    let extract_time = start.elapsed();

    // Display results
    println!("PDF Version: {}", doc.version);
    println!("Page Count: {}", doc.get_pages().len());
    println!("Objects Loaded: {}", doc.objects.len());

    // Try to get metadata
    if let Ok(info_id) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Ok(info) = doc.get_dictionary(info_id) {
            println!("\nMetadata fields found: {}", info.len());
        }
    }

    println!("\nPerformance:");
    println!("  Read time: {:?}", read_time);
    println!("  Extract time: {:?}", extract_time);
    println!("  Total time: {:?}", read_time + extract_time);

    println!("\n✓ Metadata extracted from memory!");

    Ok(())
}
