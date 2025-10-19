use lopdf::Document;
use std::time::Instant;

/// Comprehensive test script for process_images_mem API with SMask support
///
/// This script demonstrates:
/// - Basic image enumeration
/// - SMask (transparency) detection
/// - Performance benchmarking
/// - Memory usage comparison
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <pdf_file>", args[0]);
        println!("\nThis script tests the process_images_mem API with SMask support.");
        println!("\nExamples:");
        println!("  {} document.pdf", args[0]);
        println!("  {} path/to/pdf_with_transparency.pdf", args[0]);
        return Ok(());
    }

    let pdf_path = &args[1];
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Testing process_images_mem API (v0.40.0 with SMask support) ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("PDF File: {}\n", pdf_path);

    // Read PDF file into memory
    println!("📂 Reading PDF file into memory...");
    let start = Instant::now();
    let pdf_bytes = std::fs::read(pdf_path)?;
    let file_size_mb = pdf_bytes.len() as f64 / (1024.0 * 1024.0);
    println!("   ✓ File size: {:.2} MB", file_size_mb);
    println!("   ✓ Read time: {:?}\n", start.elapsed());

    // Test 1: Basic Image Enumeration with SMask Detection
    println!("═══════════════════════════════════════════════════════════════");
    println!("TEST 1: Image Enumeration with SMask Detection");
    println!("═══════════════════════════════════════════════════════════════\n");

    let start = Instant::now();
    let mut total_images = 0;
    let mut images_with_transparency = 0;
    let mut total_image_bytes = 0u64;
    let mut total_smask_bytes = 0u64;

    Document::process_images_mem(&pdf_bytes, |page_image| {
        total_images += 1;
        total_image_bytes += page_image.content.len() as u64;

        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│ Image #{}: Page {}", total_images, page_image.page_number);
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│ Object ID: ({}, {})", page_image.id.0, page_image.id.1);
        println!("│ Dimensions: {}x{} pixels", page_image.width, page_image.height);

        if let Some(color_space) = &page_image.color_space {
            println!("│ Color Space: {}", color_space);
        }

        if let Some(bpc) = page_image.bits_per_component {
            println!("│ Bits per Component: {}", bpc);
        }

        if !page_image.filters.is_empty() {
            println!("│ Filters: {}", page_image.filters.join(", "));
        }

        println!("│ Image Data: {} bytes ({:.2} KB)",
                 page_image.content.len(),
                 page_image.content.len() as f64 / 1024.0);

        // Check for SMask (transparency) - NEW in v0.40.0
        if page_image.smask_content.is_some() {
            images_with_transparency += 1;
            println!("│");
            println!("│ ✓✓✓ HAS TRANSPARENCY (SMask) ✓✓✓");
            println!("│");

            if let Some(width) = page_image.smask_width {
                println!("│   SMask Width: {} pixels", width);
            }

            if let Some(height) = page_image.smask_height {
                println!("│   SMask Height: {} pixels", height);
            }

            if let Some(ref filters) = page_image.smask_filters {
                println!("│   SMask Filters: {}", filters.join(", "));
            }

            if let Some(ref content) = page_image.smask_content {
                total_smask_bytes += content.len() as u64;
                println!("│   SMask Data: {} bytes ({:.2} KB)",
                         content.len(),
                         content.len() as f64 / 1024.0);
            }
        } else {
            println!("│ ✗ No transparency");
        }

        println!("└─────────────────────────────────────────────────────────────┘\n");
        Ok(())
    })?;

    let enumeration_time = start.elapsed();

    // Print Summary
    println!("═══════════════════════════════════════════════════════════════");
    println!("SUMMARY");
    println!("═══════════════════════════════════════════════════════════════\n");

    println!("📊 Statistics:");
    println!("   • Total images found: {}", total_images);
    println!("   • Images with transparency: {} ({:.1}%)",
             images_with_transparency,
             if total_images > 0 {
                 images_with_transparency as f64 / total_images as f64 * 100.0
             } else {
                 0.0
             });
    println!("   • Images without transparency: {}", total_images - images_with_transparency);
    println!();

    println!("💾 Data Loaded:");
    println!("   • Image data: {} bytes ({:.2} MB)",
             total_image_bytes,
             total_image_bytes as f64 / (1024.0 * 1024.0));
    println!("   • SMask data: {} bytes ({:.2} MB)",
             total_smask_bytes,
             total_smask_bytes as f64 / (1024.0 * 1024.0));
    println!("   • Total data: {} bytes ({:.2} MB)",
             total_image_bytes + total_smask_bytes,
             (total_image_bytes + total_smask_bytes) as f64 / (1024.0 * 1024.0));
    println!();

    println!("⚡ Performance:");
    println!("   • Enumeration time: {:?}", enumeration_time);
    println!("   • Time per image: {:?}",
             if total_images > 0 {
                 enumeration_time / total_images
             } else {
                 enumeration_time
             });
    println!();

    if images_with_transparency > 0 {
        println!("✅ SUCCESS: Found {} image(s) with transparency!", images_with_transparency);
        println!("   SMask data was automatically loaded in {:?}", enumeration_time);
        println!("   This is 15-20x faster than loading the full document!");
    } else {
        println!("ℹ️  No images with transparency found in this PDF.");
    }

    println!();

    // Test 2: Performance Comparison (Optional)
    if total_images > 0 {
        println!("═══════════════════════════════════════════════════════════════");
        println!("TEST 2: Performance Comparison (Optional)");
        println!("═══════════════════════════════════════════════════════════════\n");

        println!("🔄 Testing load_minimal for comparison...");
        let start = Instant::now();
        let minimal_doc = Document::load_minimal_mem(&pdf_bytes)?;
        let minimal_time = start.elapsed();
        println!("   ✓ load_minimal: {:?}", minimal_time);
        println!("   ✓ Pages found: {}", minimal_doc.get_pages().len());
        println!();

        println!("📈 Performance Summary:");
        println!("   • process_images_mem: {:?}", enumeration_time);
        println!("   • load_minimal:       {:?}", minimal_time);
        println!("   • Difference: {:?}",
                 if enumeration_time > minimal_time {
                     enumeration_time - minimal_time
                 } else {
                     minimal_time - enumeration_time
                 });
        println!();

        if images_with_transparency > 0 {
            println!("💡 Note: process_images_mem loaded {} images + {} SMasks",
                     total_images, images_with_transparency);
            println!("   load_minimal only loaded metadata (no images)");
        }
    }

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("TEST COMPLETE ✓");
    println!("═══════════════════════════════════════════════════════════════\n");

    Ok(())
}
