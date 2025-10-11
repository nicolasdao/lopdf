use lopdf::{Document, LoadOptions, LoadProgress, ProgressInterval};
use std::sync::{Arc, Mutex};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Comprehensive Progress Tracking Test");
    println!("====================================\n");

    // Test 1: Verify all stages are reported
    println!("Test 1: Verify all stages are called");
    let stages_seen = Arc::new(Mutex::new(Vec::new()));
    let stages_clone = stages_seen.clone();

    let options = LoadOptions::new().with_progress(move |p: LoadProgress| {
        stages_clone.lock().unwrap().push(p.stage);
    });

    let _doc = Document::load_with_options("assets/example.pdf", options)?;
    let stages = stages_seen.lock().unwrap();
    println!("Stages reported: {:?}", *stages);
    assert!(stages.contains(&0), "Stage 0 (Reading file) not reported");
    assert!(stages.contains(&1), "Stage 1 (Finding PDF header) not reported");
    assert!(stages.contains(&2), "Stage 2 (Parsing version) not reported");
    assert!(stages.contains(&3), "Stage 3 (Parsing cross-reference table) not reported");
    assert!(stages.contains(&4), "Stage 4 (Parsing trailer) not reported");
    assert!(stages.contains(&5), "Stage 5 (Loading objects) not reported");
    assert!(stages.contains(&6), "Stage 6 (Complete) not reported");
    println!("✓ All stages reported correctly\n");

    // Test 2: Verify progress goes from 0 to 1.0
    println!("Test 2: Verify progress range");
    let min_max = Arc::new(Mutex::new((1.0f64, 0.0f64)));
    let min_max_clone = min_max.clone();

    let options = LoadOptions::new().with_progress(move |p| {
        let mut mm = min_max_clone.lock().unwrap();
        mm.0 = mm.0.min(p.progress);
        mm.1 = mm.1.max(p.progress);
    });

    let _doc = Document::load_with_options("assets/example.pdf", options)?;
    let (min, max) = *min_max.lock().unwrap();
    println!("Progress range: {:.3} to {:.3}", min, max);
    assert!(min <= 0.02, "Progress should start near 0");
    assert!(max >= 0.99, "Progress should reach 1.0");
    println!("✓ Progress range correct\n");

    // Test 3: Verify percentage interval works
    println!("Test 3: Verify percentage interval");
    let callback_count = Arc::new(Mutex::new(0));
    let callback_count_clone = callback_count.clone();

    let options = LoadOptions::new()
        .with_progress(move |_| {
            *callback_count_clone.lock().unwrap() += 1;
        })
        .with_progress_interval(ProgressInterval::Percentage(20.0));

    let _doc = Document::load_with_options("assets/example.pdf", options)?;
    let count = *callback_count.lock().unwrap();
    println!("Callbacks with 20% interval: {}", count);
    assert!(count > 0, "Should have at least some callbacks");
    assert!(count <= 15, "Should not have excessive callbacks");
    println!("✓ Percentage interval works correctly\n");

    // Test 4: Verify Items interval works
    println!("Test 4: Verify Items interval");
    let callback_count = Arc::new(Mutex::new(0));
    let callback_count_clone = callback_count.clone();

    let options = LoadOptions::new()
        .with_progress(move |_| {
            *callback_count_clone.lock().unwrap() += 1;
        })
        .with_progress_interval(ProgressInterval::Items(2));

    let _doc = Document::load_with_options("assets/example.pdf", options)?;
    let count = *callback_count.lock().unwrap();
    println!("Callbacks with Items(2) interval: {}", count);
    assert!(count > 0, "Should have at least some callbacks");
    println!("✓ Items interval works correctly\n");

    // Test 5: Test all three loading methods
    println!("Test 5: Test all loading methods");

    // load_with_options
    let options = LoadOptions::new().with_progress(|p| {
        if p.stage == 6 {
            print!("load_with_options: ");
        }
    });
    let doc1 = Document::load_with_options("assets/example.pdf", options)?;
    println!("✓");

    // load_from_with_options
    let file = std::fs::File::open("assets/example.pdf")?;
    let options = LoadOptions::new().with_progress(|p| {
        if p.stage == 6 {
            print!("load_from_with_options: ");
        }
    });
    let doc2 = Document::load_from_with_options(file, options)?;
    println!("✓");

    // load_mem_with_options
    let pdf_bytes = std::fs::read("assets/example.pdf")?;
    let options = LoadOptions::new().with_progress(|p| {
        if p.stage == 6 {
            print!("load_mem_with_options: ");
        }
    });
    let doc3 = Document::load_mem_with_options(&pdf_bytes, options)?;
    println!("✓");

    assert_eq!(doc1.version, doc2.version);
    assert_eq!(doc2.version, doc3.version);
    println!("✓ All loading methods work correctly\n");

    // Test 6: Verify backward compatibility (no progress callback)
    println!("Test 6: Backward compatibility");
    let options = LoadOptions::new(); // No progress callback
    let _doc = Document::load_with_options("assets/example.pdf", options)?;
    println!("✓ Works without progress callback\n");

    println!("=====================================");
    println!("All tests passed! ✓");

    Ok(())
}
