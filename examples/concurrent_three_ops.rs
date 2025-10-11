use lopdf::Document;
use std::sync::Arc;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <pdf_file>", args[0]);
        println!("\nDemonstrates running 3 concurrent PDF operations:");
        println!("  1. Full document load (all objects)");
        println!("  2. Minimal metadata extraction");
        println!("  3. Lazy image extraction");
        println!("\nAll three share the same memory buffer!");
        return Ok(());
    }

    let pdf_path = &args[1];

    println!("=== Concurrent PDF Processing Demo ===");
    println!("File: {}\n", pdf_path);

    // Read file once
    let read_start = Instant::now();
    let pdf_bytes = Arc::new(std::fs::read(pdf_path)?);
    let read_time = read_start.elapsed();
    println!("File read: {:?} ({} bytes)\n", read_time, pdf_bytes.len());

    // Spawn 3 concurrent operations on the same buffer
    let start = Instant::now();

    // Operation 1: Full document load
    let bytes1 = Arc::clone(&pdf_bytes);
    let h1 = std::thread::spawn(move || {
        let op_start = Instant::now();
        let result = Document::load_mem(&bytes1);
        let op_time = op_start.elapsed();
        (result, op_time)
    });

    // Operation 2: Minimal metadata extraction
    let bytes2 = Arc::clone(&pdf_bytes);
    let h2 = std::thread::spawn(move || {
        let op_start = Instant::now();
        let result = Document::load_minimal_mem(&bytes2);
        let op_time = op_start.elapsed();
        (result, op_time)
    });

    // Operation 3: Stream all images with callback
    let bytes3 = Arc::clone(&pdf_bytes);
    let h3 = std::thread::spawn(move || {
        let op_start = Instant::now();
        let mut count = 0;
        let mut total_bytes = 0usize;
        let result = Document::process_images_mem(&bytes3, |img| {
            count += 1;
            total_bytes += img.content.len();
            Ok(())
        });
        let op_time = op_start.elapsed();
        (result.map(|_| (count, total_bytes)), op_time)
    });

    // Wait for all three to complete
    let (full_result, full_time) = h1.join().unwrap();
    let (minimal_result, minimal_time) = h2.join().unwrap();
    let (images_result, images_time) = h3.join().unwrap();

    let total_time = start.elapsed();

    println!("=== Results ===\n");

    // Operation 1 results
    match full_result {
        Ok(full_doc) => {
            println!("✓ Full Document Load:");
            println!("  Time: {:?}", full_time);
            println!("  Version: {}", full_doc.version);
            println!("  Total objects: {}", full_doc.objects.len());
            println!("  Pages: {}", full_doc.get_pages().len());
        }
        Err(e) => println!("✗ Full load failed: {}", e),
    }

    println!();

    // Operation 2 results
    match minimal_result {
        Ok(minimal) => {
            println!("✓ Minimal Metadata:");
            println!("  Time: {:?}", minimal_time);
            println!("  Version: {}", minimal.version);
            println!("  Pages: {}", minimal.get_pages().len());
            println!("  Objects loaded: {}", minimal.objects.len());
        }
        Err(e) => println!("✗ Minimal load failed: {}", e),
    }

    println!();

    // Operation 3 results
    match images_result {
        Ok((count, total_bytes)) => {
            println!("✓ Image Extraction:");
            println!("  Time: {:?}", images_time);
            println!("  Images found: {}", count);
            println!("  Total image data: {:.2} MB", total_bytes as f64 / 1_048_576.0);
        }
        Err(e) => println!("✗ Image extraction failed: {}", e),
    }

    println!();
    println!("=== Performance Summary ===");
    println!("Total parallel execution time: {:?}", total_time);
    println!("File read once: {} bytes", pdf_bytes.len());
    println!("Memory efficiency: 3 operations sharing same buffer (Arc)");

    let sequential_time_estimate = full_time + minimal_time + images_time + read_time;
    println!("\nEstimated sequential time: {:?}", sequential_time_estimate);
    println!("Actual parallel time: {:?}", total_time + read_time);

    if total_time < sequential_time_estimate {
        let speedup = sequential_time_estimate.as_secs_f64() / (total_time + read_time).as_secs_f64();
        println!("Speedup: {:.2}x faster", speedup);
    }

    Ok(())
}
