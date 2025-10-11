use lopdf::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Change this to your PDF path
    let pdf_path = "/Users/nicolasdao/Downloads/Stockland Proposal.pdf";

    println!("Extracting images from: {}\n", pdf_path);

    let mut count = 0;
    Document::process_images_with_callback(pdf_path, |img| {
        count += 1;
        println!("Image #{}: Page {}, {}x{} pixels, {} bytes",
                 count,
                 img.page_number,
                 img.width,
                 img.height,
                 img.content.len());

        if !img.filters.is_empty() {
            println!("  Filters: {:?}", img.filters);
        }

        Ok(())
    })?;

    println!("\nTotal images: {}", count);
    Ok(())
}
