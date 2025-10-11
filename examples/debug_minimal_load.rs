use lopdf::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <pdf_file>", args[0]);
        std::process::exit(1);
    }
    let pdf_path = &args[1];

    println!("Testing minimal load on: {}\n", pdf_path);

    // Test load_minimal()
    let doc_minimal = Document::load_minimal(pdf_path)?;

    println!("Version: {}", doc_minimal.version);
    println!("Objects loaded: {}", doc_minimal.objects.len());
    println!("\nObjects:");
    for (id, obj) in doc_minimal.objects.iter() {
        let type_name = obj.type_name().unwrap_or(b"Unknown");
        println!("  {} {} R: {}", id.0, id.1, String::from_utf8_lossy(type_name));
    }

    // Try to access catalog
    if let Ok(catalog_id) = doc_minimal.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        println!("\nCatalog ID: {} {} R", catalog_id.0, catalog_id.1);
        if let Ok(catalog) = doc_minimal.get_dictionary(catalog_id) {
            let keys: Vec<String> = catalog.iter().map(|(k, _)| String::from_utf8_lossy(k).to_string()).collect();
            println!("Catalog keys: {:?}", keys);

            if let Ok(pages_id) = catalog.get(b"Pages").and_then(|o| o.as_reference()) {
                println!("\nPages root ID: {} {} R", pages_id.0, pages_id.1);
                if let Ok(pages) = doc_minimal.get_dictionary(pages_id) {
                    let keys: Vec<String> = pages.iter().map(|(k, _)| String::from_utf8_lossy(k).to_string()).collect();
                    println!("Pages keys: {:?}", keys);
                    if let Ok(count) = pages.get(b"Count").and_then(|o| o.as_i64()) {
                        println!("Count field: {}", count);
                    }
                    if let Ok(kids) = pages.get(b"Kids").and_then(|o| o.as_array()) {
                        println!("Kids array length: {}", kids.len());
                    }
                }
            }
        }
    }

    println!("\nPage count from get_pages(): {}", doc_minimal.get_pages().len());

    Ok(())
}
