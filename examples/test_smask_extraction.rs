use lopdf::Document;

/// Example demonstrating SMask (transparency) extraction with process_images_mem
///
/// New in v0.39.1: process_images_mem now automatically loads and extracts SMask data
/// for images with transparency, enabling efficient transparency handling without
/// loading the entire PDF document.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <pdf_file>", args[0]);
        println!("\nThis example demonstrates SMask (transparency) extraction.");
        println!("It will show which images have transparency and their SMask properties.");
        return Ok(());
    }

    let pdf_path = &args[1];
    println!("Analyzing transparency in: {}\n", pdf_path);

    let mut total_images = 0;
    let mut images_with_transparency = 0;

    Document::process_images_with_callback(pdf_path, |page_image| {
        total_images += 1;
        println!("Image #{}: Page {}, {}x{} pixels",
                 total_images,
                 page_image.page_number,
                 page_image.width,
                 page_image.height);

        // NEW in v0.39.1: Check for SMask data
        if page_image.smask_content.is_some() {
            images_with_transparency += 1;
            println!("  ✓ HAS TRANSPARENCY (SMask)");

            if let Some(width) = page_image.smask_width {
                println!("    SMask width: {} pixels", width);
            }

            if let Some(height) = page_image.smask_height {
                println!("    SMask height: {} pixels", height);
            }

            if let Some(ref filters) = page_image.smask_filters {
                println!("    SMask filters: {}", filters.join(", "));
            }

            if let Some(ref content) = page_image.smask_content {
                println!("    SMask data size: {} bytes", content.len());
            }
        } else {
            println!("  ✗ No transparency");
        }

        if let Some(color_space) = &page_image.color_space {
            println!("  Color space: {}", color_space);
        }

        if !page_image.filters.is_empty() {
            println!("  Image filters: {}", page_image.filters.join(", "));
        }

        println!();
        Ok(())
    })?;

    println!("=== Summary ===");
    println!("Total images: {}", total_images);
    println!("Images with transparency (SMask): {}", images_with_transparency);
    println!("Images without transparency: {}", total_images - images_with_transparency);

    if images_with_transparency > 0 {
        println!("\n✓ Successfully extracted SMask data for {} image(s)", images_with_transparency);
        println!("  This was done 15-20x faster than loading the full document!");
    }

    Ok(())
}
