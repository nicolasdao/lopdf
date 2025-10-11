use lopdf::Document;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <pdf_file>", args[0]);
        println!("\nThis example demonstrates lazy image loading with immediate processing.");
        println!("Images are displayed as they're found rather than loading all first.");
        return Ok(());
    }

    let pdf_path = &args[1];

    println!("=== Example 1: Simple callback processing ===");
    println!("Processing images from: {}\n", pdf_path);

    let mut image_count = 0;
    Document::process_images_with_callback(pdf_path, |page_image| {
        image_count += 1;
        println!("Image #{}: Page {}, {}x{} pixels",
                 image_count,
                 page_image.page_number,
                 page_image.width,
                 page_image.height);

        if let Some(color_space) = &page_image.color_space {
            println!("  Color space: {}", color_space);
        }

        if !page_image.filters.is_empty() {
            println!("  Filters: {}", page_image.filters.join(", "));
        }

        if let Some(bpc) = page_image.bits_per_component {
            println!("  Bits per component: {}", bpc);
        }

        println!("  Content size: {} bytes", page_image.content.len());

        // Example: Save JPEG images directly
        if page_image.filters.contains(&"DCTDecode".to_string()) {
            let filename = format!("extracted_p{}_img{}.jpg",
                                 page_image.page_number,
                                 page_image.id.0);
            std::fs::write(&filename, &page_image.content)?;
            println!("  Saved as: {}", filename);
        }

        println!();
        Ok(())
    })?;

    println!("Total images extracted: {}\n", image_count);

    // Example 2: Concurrent operations using memory buffer
    println!("=== Example 2: Concurrent operations ===");
    println!("Loading PDF once, processing concurrently...\n");

    use std::time::Instant;

    // Read file once
    let start = Instant::now();
    let pdf_bytes = Arc::new(std::fs::read(pdf_path)?);
    println!("File read in: {:?}", start.elapsed());

    // Spawn image processing in background thread
    let bytes_clone = Arc::clone(&pdf_bytes);
    let handle = std::thread::spawn(move || {
        let start = Instant::now();
        let mut count = 0;
        Document::process_images_mem(&bytes_clone, |_img| {
            count += 1;
            Ok(())
        }).unwrap();
        (count, start.elapsed())
    });

    // Meanwhile, load minimal metadata
    let start = Instant::now();
    let minimal = Document::load_minimal_mem(&pdf_bytes)?;
    let minimal_time = start.elapsed();

    // Wait for image processing
    let (image_count, image_time) = handle.join().unwrap();

    println!("Minimal load completed in: {:?}", minimal_time);
    println!("Image processing completed in: {:?}", image_time);
    println!("PDF version: {}", minimal.version);
    println!("Page count: {}", minimal.get_pages().len());
    println!("Image count: {}", image_count);
    println!("\nBoth operations ran concurrently using the same buffer!");

    Ok(())
}
