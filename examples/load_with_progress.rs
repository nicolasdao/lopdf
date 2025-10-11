use lopdf::{Document, LoadOptions, ProgressInterval};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test 1: Load with progress callback showing percentage
    println!("Test 1: Loading with progress callback");
    println!("=====================================");

    let options = LoadOptions::new()
        .with_progress(|p| {
            println!(
                "[{:3}%] Stage {}: {} ({}/{} items)",
                (p.progress * 100.0) as u8,
                p.stage,
                p.stage_name,
                p.items_processed,
                p.items_total
            );
        })
        .with_progress_interval(ProgressInterval::Percentage(10.0));

    let doc = Document::load_with_options("assets/example.pdf", options)?;
    println!("✓ Document loaded successfully!");
    println!("  Version: {}", doc.version);
    println!("  Objects: {}", doc.objects.len());
    println!();

    // Test 2: Load with progress callback every N items
    println!("Test 2: Loading with progress every 5 items");
    println!("===========================================");

    let options = LoadOptions::new()
        .with_progress(|p| {
            if p.stage == 5 || p.stage == 6 {
                println!(
                    "[{:3}%] {}: {}/{} objects processed",
                    (p.progress * 100.0) as u8,
                    p.stage_name,
                    p.items_processed,
                    p.items_total
                );
            }
        })
        .with_progress_interval(ProgressInterval::Items(5));

    let doc = Document::load_with_options("assets/example.pdf", options)?;
    println!("✓ Document loaded successfully!");
    println!("  Version: {}", doc.version);
    println!();

    // Test 3: Load from memory with progress
    println!("Test 3: Loading from memory with progress");
    println!("=========================================");

    let pdf_bytes = std::fs::read("assets/example.pdf")?;
    let options = LoadOptions::new()
        .with_progress(|p| {
            print!("\r[{:3}%] {} ", (p.progress * 100.0) as u8, p.stage_name);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        });

    let doc = Document::load_mem_with_options(&pdf_bytes, options)?;
    println!("\n✓ Document loaded successfully from memory!");
    println!("  Pages: {}", doc.get_pages().len());
    println!();

    Ok(())
}
