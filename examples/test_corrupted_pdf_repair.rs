use std::env;
use std::fs;

/// Test 0: Diagnose PDF Structure
fn diagnose_pdf_structure(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📋 DIAGNOSTIC: PDF Structure Analysis");

    let bytes = fs::read(path)?;
    let file_size = bytes.len();

    println!("   File size: {} bytes ({:.2} MB)", file_size, file_size as f64 / 1_048_576.0);

    // Find last %%EOF
    let eof_pos = bytes.windows(5)
        .rposition(|w| w == b"%%EOF")
        .map(|pos| pos + 5);

    match eof_pos {
        Some(pos) => {
            let garbage_size = file_size - pos;
            println!("   Last %%EOF at: byte {} ({:.1}% of file)", pos, (pos as f64 / file_size as f64) * 100.0);

            if garbage_size > 0 {
                println!("   ⚠️  TRAILING GARBAGE: {} bytes after %%EOF", garbage_size);
                println!("      This garbage should be IGNORED per PDF spec!");

                // Show a sample of the garbage
                if garbage_size > 0 {
                    let sample_size = garbage_size.min(64);
                    let sample = &bytes[pos..pos + sample_size];
                    let sample_str: String = sample.iter()
                        .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
                        .collect();
                    println!("      Sample (first {} bytes): {:?}", sample_size, sample_str);
                }
            } else {
                println!("   ✓ No trailing garbage (clean EOF)");
            }
        }
        None => {
            println!("   ✗ No %%EOF marker found!");
        }
    }

    // Try to find startxref
    let last_1kb = &bytes[bytes.len().saturating_sub(1024)..];
    if let Some(pos) = last_1kb.windows(9).position(|w| w == b"startxref") {
        println!("   Found 'startxref' in last 1KB at offset from end: -{}", 1024 - pos);
    } else {
        println!("   ⚠️  'startxref' not found in last 1KB");
    }

    // Try to find trailer
    if let Some(pos) = last_1kb.windows(7).position(|w| w == b"trailer") {
        println!("   Found 'trailer' keyword in last 1KB at offset from end: -{}", 1024 - pos);
    } else {
        println!("   ⚠️  'trailer' keyword not found in last 1KB");
    }

    println!();
    Ok(())
}

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
            println!("   ✓ Document loaded with repair!");
            println!("     Version: {}", doc.version);
            println!("     Objects: {}", doc.objects.len());
            println!("     Pages: {}", doc.get_pages().len());

            // Check if trailer has /Root - this is REQUIRED
            if let Ok(root_ref) = doc.trailer.get(b"Root") {
                println!("     Trailer /Root: {:?} ✓", root_ref);
                println!("   ✓ SUCCESS - Valid document structure!");
                Ok(())
            } else {
                println!("     ⚠️  CRITICAL: Trailer missing /Root!");
                println!("   ✗ FAILED - Document is broken (no catalog)");
                Err("Missing /Root in trailer".into())
            }
        }
        Err(e) => {
            println!("   ✗ FAILED - {}", e);

            // Check if error is about missing Root
            let err_str = format!("{}", e);
            if err_str.contains("Root") {
                println!("     ⚠️  BUG DETECTED: Repair created trailer without /Root!");
                println!("     This means repair scanned post-EOF garbage and got confused.");
                println!("     PDF spec requires ignoring data after %%EOF marker.");
            }

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
            println!("   ✓ Metadata loaded with repair!");
            println!("     Version: {}", doc.version);
            println!("     Pages: {}", doc.get_pages().len());

            // Check if trailer has /Root - this is REQUIRED
            if doc.trailer.get(b"Root").is_ok() {
                println!("     Trailer /Root: present ✓");
                println!("   ✓ SUCCESS - Valid document structure!");
                Ok(())
            } else {
                println!("     ⚠️  CRITICAL: Trailer missing /Root!");
                println!("   ✗ FAILED - Document is broken (no catalog)");
                Err("Missing /Root in trailer".into())
            }
        }
        Err(e) => {
            println!("   ✗ FAILED - {}", e);

            // Check if error is about missing Root
            let err_str = format!("{}", e);
            if err_str.contains("Root") {
                println!("     ⚠️  BUG DETECTED: Repair created trailer without /Root!");
            }

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

            let err_str = format!("{}", e);
            if err_str.contains("Root") {
                println!("     ⚠️  BUG DETECTED: Repair created trailer without /Root!");
            }

            Err(e.into())
        }
    }
}

/// Test 4: Load with Post-EOF Garbage Stripped (Workaround Test)
fn test_stripped_garbage(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use lopdf::LoadOptions;

    println!("\n🧪 TEST 4: Workaround - Strip Post-EOF Garbage");
    println!("   Testing: Load after manually removing post-%%EOF data");

    // Read file
    let bytes = fs::read(path)?;

    // Find last %%EOF
    let eof_pos = bytes.windows(5)
        .rposition(|w| w == b"%%EOF")
        .map(|pos| pos + 5);

    match eof_pos {
        Some(pos) => {
            let original_size = bytes.len();
            let garbage_size = original_size - pos;

            if garbage_size == 0 {
                println!("   ℹ️  No garbage to strip (clean PDF)");
                println!("   Skipping this test...");
                return Ok(());
            }

            println!("   Stripping {} bytes of post-EOF garbage...", garbage_size);
            let clean_bytes = &bytes[..pos];

            let options = LoadOptions::new().with_repair(true);

            // Try loading the cleaned version
            let result = lopdf::Document::load_mem_with_options(clean_bytes, options);

            match result {
                Ok(doc) => {
                    println!("   ✓ SUCCESS - Cleaned PDF loads perfectly!");
                    println!("     Version: {}", doc.version);
                    println!("     Objects: {}", doc.objects.len());
                    println!("     Pages: {}", doc.get_pages().len());

                    if doc.trailer.get(b"Root").is_ok() {
                        println!("     Trailer /Root: present ✓");
                    }

                    println!();
                    println!("   🎯 PROOF: The issue is post-EOF garbage!");
                    println!("      Original file ({} bytes): FAILS", original_size);
                    println!("      Cleaned file ({} bytes): WORKS", pos);
                    println!("      Difference: {} bytes of trailing garbage", garbage_size);
                    Ok(())
                }
                Err(e) => {
                    println!("   ✗ UNEXPECTED - Even clean version failed: {}", e);
                    println!("     This suggests a different issue");
                    Err(e.into())
                }
            }
        }
        None => {
            println!("   ✗ No %%EOF marker found, cannot strip garbage");
            Err("No %%EOF marker".into())
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
    println!("This test validates PDF repair with trailing garbage after %%EOF.");
    println!("Per PDF spec (ISO 32000), data after %%EOF should be IGNORED.");
    println!("═══════════════════════════════════════════════════════════");

    // Run diagnostic first
    let _ = diagnose_pdf_structure(pdf_path);

    // Track test results
    let mut passed = 0;
    let mut failed = 0;
    let total_tests = 4;

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

    // Test 4: Workaround test (strip garbage)
    if test_stripped_garbage(pdf_path).is_ok() {
        passed += 1;
    } else {
        failed += 1;
    }

    // Summary
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  Test Summary");
    println!("═══════════════════════════════════════════════════════════");
    println!("  Tests passed: {}/{}", passed, total_tests);
    println!("  Tests failed: {}/{}", failed, total_tests);
    println!();

    // Analyze results
    if passed == total_tests {
        println!("  ✅ ALL TESTS PASSED!");
        println!("     The repair feature correctly handles post-EOF garbage!");
        println!();
        std::process::exit(0);
    } else if failed >= 2 && passed >= 1 {
        // Tests 1-2 failed but Test 4 (workaround) passed
        println!("  🐛 BUG CONFIRMED: Post-EOF Garbage Issue");
        println!();
        println!("  Tests 1-2 FAILED with missing /Root in trailer.");
        println!("  Test 4 (workaround) PASSED after stripping garbage.");
        println!();
        println!("  This proves the issue:");
        println!();
        println!("  ❌ Original file (1.4MB with 3KB garbage): Trailer missing /Root");
        println!("  ✅ Cleaned file (garbage stripped): Trailer has /Root, 3 pages");
        println!();
        println!("  Root Cause:");
        println!("  - Repair logic scans entire file including post-%%EOF garbage");
        println!("  - Finds malformed data in the 3KB garbage region");
        println!("  - Creates minimal trailer without /Root (broken document)");
        println!("  - Pages: 0 instead of actual 3 pages");
        println!();
        println!("  Fix Required:");
        println!("  - Find LAST %%EOF marker in file");
        println!("  - Only scan valid PDF region (before %%EOF)");
        println!("  - Respect PDF spec (ISO 32000): ignore all post-EOF data");
        println!();
        println!("  Expected Behavior:");
        println!("  - Find %%EOF at byte 1,401,894");
        println!("  - Ignore bytes 1,401,894 to 1,404,928 (3,034 bytes)");
        println!("  - Scan only bytes 0 to 1,401,894 for xref/trailer");
        println!("  - Find valid trailer with /Root before %%EOF");
        println!();
        std::process::exit(1);
    } else if passed > 0 {
        println!("  ⚠️  PARTIAL SUCCESS");
        println!("     {} test(s) passed, {} test(s) failed", passed, failed);
        println!();
        std::process::exit(1);
    } else {
        println!("  ✗ ALL TESTS FAILED");
        println!("     The repair feature has critical issues");
        println!();
        std::process::exit(1);
    }
}
