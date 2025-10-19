use lopdf::Document;
use std::time::Instant;

/// Side-by-side comparison of image loading methods
///
/// This script compares:
/// 1. Lazy loading with process_images_mem (v0.40.0 with SMask)
/// 2. Full document load then image extraction
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <pdf_file>", args[0]);
        println!("\nThis script compares two image loading approaches:");
        println!("  1. Lazy loading (process_images_mem - v0.40.0 with SMask)");
        println!("  2. Full document load (Document::load + manual extraction)");
        return Ok(());
    }

    let pdf_path = &args[1];

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     Comparing Image Loading Methods (v0.40.0)               ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("PDF File: {}\n", pdf_path);

    // Read PDF file once
    println!("📂 Reading PDF file into memory...");
    let start = Instant::now();
    let pdf_bytes = std::fs::read(pdf_path)?;
    let file_size_mb = pdf_bytes.len() as f64 / (1024.0 * 1024.0);
    let read_time = start.elapsed();
    println!("   ✓ File size: {:.2} MB", file_size_mb);
    println!("   ✓ Read time: {:?}\n", read_time);

    // =================================================================
    // METHOD 1: Lazy Loading with process_images_mem (v0.40.0)
    // =================================================================
    println!("═══════════════════════════════════════════════════════════════");
    println!("METHOD 1: Lazy Loading (process_images_mem)");
    println!("═══════════════════════════════════════════════════════════════\n");
    println!("Loading: Catalog → Pages → Images → SMasks");
    println!("Skipping: Content streams, Fonts, Annotations, Forms\n");

    let start = Instant::now();
    let mut lazy_image_count = 0;
    let mut lazy_smask_count = 0;
    let mut lazy_image_bytes = 0u64;
    let mut lazy_smask_bytes = 0u64;

    Document::process_images_mem(&pdf_bytes, |page_image| {
        lazy_image_count += 1;
        lazy_image_bytes += page_image.content.len() as u64;

        if let Some(ref smask) = page_image.smask_content {
            lazy_smask_count += 1;
            lazy_smask_bytes += smask.len() as u64;
        }

        Ok(())
    })?;

    let lazy_time = start.elapsed();

    println!("📊 Results:");
    println!("   • Images found: {}", lazy_image_count);
    println!("   • Images with SMask: {}", lazy_smask_count);
    println!("   • Image data: {:.2} MB", lazy_image_bytes as f64 / (1024.0 * 1024.0));
    println!("   • SMask data: {:.2} MB", lazy_smask_bytes as f64 / (1024.0 * 1024.0));
    println!("   • Total time: {:?}", lazy_time);
    if lazy_image_count > 0 {
        println!("   • Time per image: {:?}", lazy_time / lazy_image_count as u32);
    }
    println!();

    // =================================================================
    // METHOD 2: Full Document Load + Manual Extraction
    // =================================================================
    println!("═══════════════════════════════════════════════════════════════");
    println!("METHOD 2: Full Document Load");
    println!("═══════════════════════════════════════════════════════════════\n");
    println!("Loading: ENTIRE PDF (all objects, streams, fonts, etc.)");
    println!("Then: Manual image + SMask extraction\n");

    let start = Instant::now();

    // Load entire document
    println!("⏳ Loading full document...");
    let load_start = Instant::now();
    let doc = Document::load_mem(&pdf_bytes)?;
    let load_time = load_start.elapsed();
    println!("   ✓ Document loaded in {:?}", load_time);
    println!("   ✓ Total objects: {}", doc.objects.len());

    // Extract images manually
    println!("⏳ Extracting images...");
    let extract_start = Instant::now();
    let mut full_image_count = 0;
    let mut full_smask_count = 0;
    let mut full_image_bytes = 0u64;
    let mut full_smask_bytes = 0u64;

    for (page_num, page_id) in doc.get_pages() {
        // Get page resources
        if let Ok(page_dict) = doc.get_dictionary(page_id) {
            // Get Resources dictionary
            let resources_opt = page_dict
                .get(b"Resources")
                .ok()
                .and_then(|r| {
                    if let Ok(id) = r.as_reference() {
                        doc.get_dictionary(id).ok()
                    } else {
                        r.as_dict().ok()
                    }
                });

            if let Some(resources) = resources_opt {
                // Get XObject dictionary
                let xobject_opt = resources
                    .get(b"XObject")
                    .ok()
                    .and_then(|x| {
                        if let Ok(id) = x.as_reference() {
                            doc.get_dictionary(id).ok()
                        } else {
                            x.as_dict().ok()
                        }
                    });

                if let Some(xobject) = xobject_opt {
                    // Iterate through XObjects
                    for (_name, xobj_ref) in xobject.iter() {
                        if let Ok(xobj_id) = xobj_ref.as_reference() {
                            if let Ok(xobj) = doc.get_object(xobj_id) {
                                if let Ok(stream) = xobj.as_stream() {
                                    // Check if it's an image
                                    let is_image = stream.dict
                                        .get(b"Subtype")
                                        .ok()
                                        .and_then(|s| s.as_name().ok())
                                        .map(|name| name == b"Image")
                                        .unwrap_or(false);

                                    if is_image {
                                        full_image_count += 1;
                                        full_image_bytes += stream.content.len() as u64;

                                        // Check for SMask
                                        if let Ok(smask_ref) = stream.dict.get(b"SMask") {
                                            if let Ok(smask_id) = smask_ref.as_reference() {
                                                if let Ok(smask_obj) = doc.get_object(smask_id) {
                                                    if let Ok(smask_stream) = smask_obj.as_stream() {
                                                        full_smask_count += 1;
                                                        full_smask_bytes += smask_stream.content.len() as u64;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let extract_time = extract_start.elapsed();
    let full_time = start.elapsed();

    println!("   ✓ Images extracted in {:?}", extract_time);
    println!();

    println!("📊 Results:");
    println!("   • Images found: {}", full_image_count);
    println!("   • Images with SMask: {}", full_smask_count);
    println!("   • Image data: {:.2} MB", full_image_bytes as f64 / (1024.0 * 1024.0));
    println!("   • SMask data: {:.2} MB", full_smask_bytes as f64 / (1024.0 * 1024.0));
    println!("   • Document load time: {:?}", load_time);
    println!("   • Extraction time: {:?}", extract_time);
    println!("   • Total time: {:?}", full_time);
    if full_image_count > 0 {
        println!("   • Time per image: {:?}", full_time / full_image_count as u32);
    }
    println!();

    // =================================================================
    // COMPARISON
    // =================================================================
    println!("═══════════════════════════════════════════════════════════════");
    println!("COMPARISON");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Verify same results
    println!("✓ Data Verification:");
    if lazy_image_count == full_image_count {
        println!("   ✓ Same number of images: {}", lazy_image_count);
    } else {
        println!("   ✗ Different image counts: {} vs {}", lazy_image_count, full_image_count);
    }

    if lazy_smask_count == full_smask_count {
        println!("   ✓ Same number of SMasks: {}", lazy_smask_count);
    } else {
        println!("   ✗ Different SMask counts: {} vs {}", lazy_smask_count, full_smask_count);
    }
    println!();

    // Performance comparison
    println!("⚡ Performance Comparison:");
    println!("┌────────────────────────────┬──────────────┬──────────────┬──────────────┐");
    println!("│ Metric                     │ Lazy Loading │ Full Load    │ Difference   │");
    println!("├────────────────────────────┼──────────────┼──────────────┼──────────────┤");
    println!("│ Total time                 │ {:11.3?} │ {:11.3?} │ {:11.3?} │",
             lazy_time, full_time,
             if full_time > lazy_time { full_time - lazy_time } else { lazy_time - full_time });
    println!("│ Time per image             │ {:11.3?} │ {:11.3?} │ {:11.3?} │",
             if lazy_image_count > 0 { lazy_time / lazy_image_count as u32 } else { lazy_time },
             if full_image_count > 0 { full_time / full_image_count as u32 } else { full_time },
             std::time::Duration::from_secs(0));
    println!("└────────────────────────────┴──────────────┴──────────────┴──────────────┘");
    println!();

    // Speedup calculation
    if full_time > lazy_time {
        let speedup = full_time.as_secs_f64() / lazy_time.as_secs_f64();
        println!("🚀 Speedup: {:.2}x faster with lazy loading", speedup);
        println!("   Lazy loading is {:.1}% faster", (speedup - 1.0) * 100.0);
    } else {
        let slowdown = lazy_time.as_secs_f64() / full_time.as_secs_f64();
        println!("⚠️  Slowdown: {:.2}x slower with lazy loading", slowdown);
        println!("   Full load is {:.1}% faster", (slowdown - 1.0) * 100.0);
        println!("   This is unexpected - may indicate a small PDF or caching effects");
    }
    println!();

    // Memory usage
    println!("💾 Memory Efficiency:");
    let lazy_loaded_mb = (lazy_image_bytes + lazy_smask_bytes) as f64 / (1024.0 * 1024.0);
    let full_loaded_mb = file_size_mb; // Full load = entire file
    println!("   • Lazy loading data: {:.2} MB", lazy_loaded_mb);
    println!("   • Full load data: {:.2} MB (entire file)", full_loaded_mb);
    if full_loaded_mb > 0.0 {
        let memory_saved_pct = (1.0 - (lazy_loaded_mb / full_loaded_mb)) * 100.0;
        println!("   • Memory saved: {:.1}%", memory_saved_pct);
    }
    println!();

    // Objects loaded
    println!("📦 Objects Loaded:");
    println!("   • Full load: {} objects", doc.objects.len());
    println!("   • Lazy load: ~{} objects (estimated)",
             lazy_image_count + lazy_smask_count + 10); // images + smasks + structure
    if doc.objects.len() > 0 {
        let object_reduction = (1.0 - ((lazy_image_count + lazy_smask_count + 10) as f64 / doc.objects.len() as f64)) * 100.0;
        println!("   • Object reduction: {:.1}%", object_reduction);
    }
    println!();

    // Conclusion
    println!("═══════════════════════════════════════════════════════════════");
    println!("CONCLUSION");
    println!("═══════════════════════════════════════════════════════════════\n");

    if full_time > lazy_time {
        println!("✅ Lazy loading (process_images_mem) is the WINNER!");
        println!();
        println!("Benefits:");
        println!("  • Faster execution ({:.2}x speedup)", full_time.as_secs_f64() / lazy_time.as_secs_f64());
        println!("  • Less memory usage ({:.1}% reduction)",
                 if full_loaded_mb > 0.0 { (1.0 - (lazy_loaded_mb / full_loaded_mb)) * 100.0 } else { 0.0 });
        println!("  • Fewer objects loaded ({:.1}% reduction)",
                 if doc.objects.len() > 0 { (1.0 - ((lazy_image_count + lazy_smask_count + 10) as f64 / doc.objects.len() as f64)) * 100.0 } else { 0.0 });
        println!("  • Automatic SMask loading (v0.40.0)");
        println!();
        println!("Recommendation: Use process_images_mem for image extraction!");
    } else {
        println!("⚠️  Full load was faster in this test");
        println!();
        println!("Possible reasons:");
        println!("  • Very small PDF (< 1 MB) - overhead dominates");
        println!("  • Few images - not enough to amortize lazy loading cost");
        println!("  • Caching effects - run test multiple times");
        println!("  • PDF already in memory/cache");
        println!();
        println!("Recommendation: Use lazy loading for larger PDFs (> 5 MB)");
    }

    println!();
    println!("═══════════════════════════════════════════════════════════════\n");

    Ok(())
}
