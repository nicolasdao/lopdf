use std::env;

/// Test 1: Full Document Load
fn test_full_load(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use lopdf::LoadOptions;

    println!("\n🔍 TEST 1: Full Document Load");
    println!("   Testing: Document::load_with_options() with repair=true");

    let options = LoadOptions::new().with_repair(true);

    #[cfg(not(feature = "async"))]
    let result = lopdf::Document::load_with_options(path, options);

    #[cfg(feature = "async")]
    let result = {
        use tokio::runtime::Builder;
        Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                lopdf::Document::load_with_options(path, options).await
            })
    };

    match result {
        Ok(doc) => {
            println!("   ✓ SUCCESS - Document loaded with repair!");
            println!("     Version: {}", doc.version);
            println!("     Objects: {}", doc.objects.len());
            println!("     Pages: {}", doc.get_pages().len());
            Ok(())
        }
        Err(e) => {
            println!("   ✗ FAILED - {}", e);
            println!("     This should succeed with repair enabled");
            Err(e.into())
        }
    }
}

/// Test 2: Minimal Load (Fast Metadata Extraction)
fn test_minimal_load(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use lopdf::LoadOptions;

    println!("\n🔍 TEST 2: Minimal Load (Fast Metadata)");
    println!("   Testing: Document::load_minimal_with_options() with repair=true");

    let options = LoadOptions::new().with_repair(true);

    #[cfg(not(feature = "async"))]
    let result = lopdf::Document::load_minimal_with_options(path, options);

    #[cfg(feature = "async")]
    let result = {
        use tokio::runtime::Builder;
        Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                lopdf::Document::load_minimal_with_options(path, options).await
            })
    };

    match result {
        Ok(doc) => {
            println!("   ✓ SUCCESS - Metadata loaded with repair!");
            println!("     Version: {}", doc.version);
            println!("     Pages: {}", doc.get_pages().len());
            Ok(())
        }
        Err(e) => {
            println!("   ✗ FAILED - {}", e);
            println!("     This should succeed with repair enabled");
            Err(e.into())
        }
    }
}

/// Test 3: Image Extraction
fn test_image_extraction(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use lopdf::LoadOptions;

    println!("\n🔍 TEST 3: Image Extraction (Lazy Loading)");
    println!("   Testing: Document::process_images_with_options() with repair=true");

    let options = LoadOptions::new().with_repair(true);
    let mut image_count = 0;

    let result = lopdf::Document::process_images_with_options(path, options, |page_image| {
        image_count += 1;
        println!("   Found image {} on page {}: {}x{}",
                 image_count,
                 page_image.page_number,
                 page_image.width,
                 page_image.height);
        Ok(())
    });

    match result {
        Ok(_) => {
            println!("   ✓ SUCCESS - {} images extracted with repair!", image_count);
            Ok(())
        }
        Err(e) => {
            println!("   ✗ FAILED - {}", e);
            println!("     This should succeed with repair enabled");
            Err(e.into())
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <corrupted_pdf_file>", args[0]);
        eprintln!();
        eprintln!("This tool tests PDF repair functionality on corrupted PDFs.");
        eprintln!("It will test three loading methods:");
        eprintln!("  1. Full document load (Document::load)");
        eprintln!("  2. Minimal load for metadata (Document::load_minimal)");
        eprintln!("  3. Image extraction (Document::process_images_with_callback)");
        eprintln!();
        eprintln!("Before repair implementation: Expect failures");
        eprintln!("After repair implementation: Expect success with repair option");
        std::process::exit(1);
    }

    let pdf_path = &args[1];

    println!("═══════════════════════════════════════════════════════════");
    println!("  Corrupted PDF Repair Test Suite");
    println!("═══════════════════════════════════════════════════════════");
    println!("PDF: {}", pdf_path);
    println!();
    println!("This test demonstrates the issue with corrupted PDFs");
    println!("(missing/invalid startxref) and validates the repair fix.");
    println!("═══════════════════════════════════════════════════════════");

    // Track test results
    let mut passed = 0;
    let mut failed = 0;

    // Test 1: Full Load
    if test_full_load(pdf_path).is_ok() {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 2: Minimal Load
    if test_minimal_load(pdf_path).is_ok() {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 3: Image Extraction
    if test_image_extraction(pdf_path).is_ok() {
        passed += 1;
    } else {
        failed += 1;
    }

    // Summary
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  Test Summary");
    println!("═══════════════════════════════════════════════════════════");
    println!("  Tests passed: {}/3", passed);
    println!("  Tests failed: {}/3", failed);
    println!();

    if passed > 0 {
        println!("  🎉 {} test(s) passed with repair feature!", passed);
        if failed > 0 {
            println!("  ⚠️  {} test(s) still failing - need to implement:", failed);
            println!("     - load_minimal_with_options() with repair support");
            println!("     - process_images_with_options() with repair support");
        } else {
            println!("  ✓ ALL TESTS PASSED!");
            println!("     The repair feature is fully working!");
        }
        println!();
        std::process::exit(if failed > 0 { 1 } else { 0 });
    } else {
        println!("  ✗ ALL TESTS FAILED");
        println!("     The repair feature is not working as expected");
        println!();
        std::process::exit(1);
    }
}
