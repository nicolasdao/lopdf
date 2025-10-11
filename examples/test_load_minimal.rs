use lopdf::Document;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <pdf_file>", args[0]);
        std::process::exit(1);
    }
    let pdf_path = &args[1];

    println!("Testing PDF: {}\n", pdf_path);

    // Test load_minimal()
    println!("=== Testing load_minimal() ===");
    let start = Instant::now();
    let doc_minimal = Document::load_minimal(pdf_path)?;
    let minimal_duration = start.elapsed();

    println!("Version: {}", doc_minimal.version);
    println!("Page count: {}", doc_minimal.get_pages().len());
    println!("Objects loaded: {}", doc_minimal.objects.len());

    // Try to access info dictionary
    if let Ok(info_id) = doc_minimal.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Ok(info) = doc_minimal.get_dictionary(info_id) {
            println!("Info dictionary found with {} entries", info.len());

            // Print some common metadata fields
            if let Ok(title_obj) = info.get(b"Title") {
                if let Ok(title) = title_obj.as_str() {
                    println!("  Title: {}", String::from_utf8_lossy(title));
                }
            }
            if let Ok(author_obj) = info.get(b"Author") {
                if let Ok(author) = author_obj.as_str() {
                    println!("  Author: {}", String::from_utf8_lossy(author));
                }
            }
        }
    }

    println!("Time taken: {:?}\n", minimal_duration);

    // Test regular load()
    println!("=== Testing load() ===");
    let start = Instant::now();
    let doc_full = Document::load(pdf_path)?;
    let full_duration = start.elapsed();

    println!("Version: {}", doc_full.version);
    println!("Page count: {}", doc_full.get_pages().len());
    println!("Objects loaded: {}", doc_full.objects.len());
    println!("Time taken: {:?}\n", full_duration);

    // Compare results
    println!("=== Comparison ===");
    println!("Speedup: {:.2}x faster", full_duration.as_secs_f64() / minimal_duration.as_secs_f64());
    println!("Objects reduction: {} → {} ({:.1}% reduction)",
        doc_full.objects.len(),
        doc_minimal.objects.len(),
        (1.0 - doc_minimal.objects.len() as f64 / doc_full.objects.len() as f64) * 100.0
    );

    // Verify correctness
    assert_eq!(doc_minimal.version, doc_full.version, "Version mismatch!");
    assert_eq!(doc_minimal.get_pages().len(), doc_full.get_pages().len(), "Page count mismatch!");
    println!("\n✓ Results match!");

    Ok(())
}
