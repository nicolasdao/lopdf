# lopdf

[![Crates.io](https://img.shields.io/crates/v/lopdf.svg)](https://crates.io/crates/lopdf)
[![CI](https://github.com/J-F-Liu/lopdf/actions/workflows/ci.yml/badge.svg)](https://github.com/J-F-Liu/lopdf/actions/workflows/ci.yml)
[![Docs]( https://docs.rs/lopdf/badge.svg)](https://docs.rs/lopdf)

A Rust library for PDF document manipulation.

A useful reference for understanding the PDF file format and the
eventual usage of this library is the
[PDF 1.7 Reference Document](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).
The PDF 2.0 specification is available [here](https://www.pdfa.org/announcing-no-cost-access-to-iso-32000-2-pdf-2-0/).

## Table of Contents

- [Requirements](#requirements)
- [Example Code](#example-code)
  - [Create PDF document](#create-pdf-document)
  - [Merge PDF documents](#merge-pdf-documents)
  - [Modify PDF document](#modify-pdf-document)
  - [Load PDF with Progress Tracking](#load-pdf-with-progress-tracking)
    - [Synchronous version (default)](#synchronous-version-default)
    - [Async version (for WASM or with async feature)](#async-version-for-wasm-or-with-async-feature)
    - [WASM Example](#wasm-example)
  - [Handling Corrupted PDFs with Repair](#handling-corrupted-pdfs-with-repair)
  - [Fast Metadata Extraction with load_minimal()](#fast-metadata-extraction-with-load_minimal)
  - [Extract Images with Lazy Loading](#extract-images-with-lazy-loading)
    - [SMask Support for Transparency (v0.40.0)](#smask-support-for-transparency-v0400)
    - [Running All Three Operations Concurrently](#running-all-three-operations-concurrently)
  - [Save PDF with Object Streams (Modern Format)](#save-pdf-with-object-streams-modern-format)
  - [Complete Example: Creating and Saving with Object Streams](#complete-example-creating-and-saving-with-object-streams)
- [Object Streams Support](#object-streams-support)
  - [Key Benefits](#key-benefits)
  - [Creating Object Streams Directly](#creating-object-streams-directly)
  - [Object Eligibility](#object-eligibility)
  - [Cross-reference Streams](#cross-reference-streams)
  - [SaveOptions Reference](#saveoptions-reference)
- [FAQ](#faq)

## Requirements

- **Rust 1.85 or later** - Required for Rust 2024 edition features and object streams support
- To check your Rust version: `rustc --version`
- To update Rust: `rustup update`

## Example Code

* Create PDF document

```rust
use lopdf::dictionary;
use lopdf::{Document, Object, Stream};
use lopdf::content::{Content, Operation};

// `with_version` specifes the PDF version this document complies with.
let mut doc = Document::with_version("1.5");
// Object IDs are used for cross referencing in PDF documents.
// `lopdf` helps keep track of them for us. They are simple integers.
// Calls to `doc.new_object_id` and `doc.add_object` return an object ID.

// "Pages" is the root node of the page tree.
let pages_id = doc.new_object_id();

// Fonts are dictionaries. The "Type", "Subtype" and "BaseFont" tags
// are straight out of the PDF spec.
//
// The dictionary macro is a helper that allows complex
// key-value relationships to be represented in a simpler
// visual manner, similar to a match statement.
// A dictionary is implemented as an IndexMap of Vec<u8>, and Object
let font_id = doc.add_object(dictionary! {
    // type of dictionary
    "Type" => "Font",
    // type of font, type1 is simple postscript font
    "Subtype" => "Type1",
    // basefont is postscript name of font for type1 font.
    // See PDF reference document for more details
    "BaseFont" => "Courier",
});

// Font dictionaries need to be added into resource
// dictionaries in order to be used.
// Resource dictionaries can contain more than just fonts,
// but normally just contains fonts.
// Only one resource dictionary is allowed per page tree root.
let resources_id = doc.add_object(dictionary! {
    // Fonts are actually triplely nested dictionaries. Fun!
    "Font" => dictionary! {
        // F1 is the font name used when writing text.
        // It must be unique in the document. It does not
        // have to be F1
        "F1" => font_id,
    },
});

// `Content` is a wrapper struct around an operations struct that contains
// a vector of operations. The operations struct contains a vector of
// that match up with a particular PDF operator and operands.
// Refer to the PDF spec for more details on the operators and operands
// Note, the operators and operands are specified in a reverse order
// from how they actually appear in the PDF file itself.
let content = Content {
    operations: vec![
        // BT begins a text element. It takes no operands.
        Operation::new("BT", vec![]),
        // Tf specifies the font and font size.
        // Font scaling is complicated in PDFs.
        // Refer to the spec for more info.
        // The `into()` methods convert the types into
        // an enum that represents the basic object types in PDF documents.
        Operation::new("Tf", vec!["F1".into(), 48.into()]),
        // Td adjusts the translation components of the text matrix.
        // When used for the first time after BT, it sets the initial
        // text position on the page.
        // Note: PDF documents have Y=0 at the bottom. Thus 600 to print text near the top.
        Operation::new("Td", vec![100.into(), 600.into()]),
        // Tj prints a string literal to the page. By default, this is black text that is
        // filled in. There are other operators that can produce various textual effects and
        // colors
        Operation::new("Tj", vec![Object::string_literal("Hello World!")]),
        // ET ends the text element.
        Operation::new("ET", vec![]),
    ],
};

// Streams are a dictionary followed by a (possibly encoded) sequence of bytes.
// What that sequence of bytes represents, depends on the context.
// The stream dictionary is set internally by lopdf and normally doesn't
// need to be manually manipulated. It contains keys such as
// Length, Filter, DecodeParams, etc.
let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

// Page is a dictionary that represents one page of a PDF file.
// Its required fields are "Type", "Parent" and "Contents".
let page_id = doc.add_object(dictionary! {
    "Type" => "Page",
    "Parent" => pages_id,
    "Contents" => content_id,
});

// Again, "Pages" is the root of the page tree. The ID was already created
// at the top of the page, since we needed it to assign to the parent element
// of the page dictionary.
//
// These are just the basic requirements for a page tree root object.
// There are also many additional entries that can be added to the dictionary,
// if needed. Some of these can also be defined on the page dictionary itself,
// and not inherited from the page tree root.
let pages = dictionary! {
    // Type of dictionary
    "Type" => "Pages",
    // Vector of page IDs in document. Normally would contain more than one ID
    // and be produced using a loop of some kind.
    "Kids" => vec![page_id.into()],
    // Page count
    "Count" => 1,
    // ID of resources dictionary, defined earlier
    "Resources" => resources_id,
    // A rectangle that defines the boundaries of the physical or digital media.
    // This is the "page size".
    "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
};

// Using `insert()` here, instead of `add_object()` since the ID is already known.
doc.objects.insert(pages_id, Object::Dictionary(pages));

// Creating document catalog.
// There are many more entries allowed in the catalog dictionary.
let catalog_id = doc.add_object(dictionary! {
    "Type" => "Catalog",
    "Pages" => pages_id,
});

// The "Root" key in trailer is set to the ID of the document catalog,
// the remainder of the trailer is set during `doc.save()`.
doc.trailer.set("Root", catalog_id);
doc.compress();

// Store file in current working directory.
// Note: Line is excluded when running tests
if false {
    // Traditional save
    doc.save("example.pdf").unwrap();
    
    // Or save with object streams for smaller file size
    let mut file = std::fs::File::create("example_compressed.pdf").unwrap();
    doc.save_modern(&mut file).unwrap();
}
```

* Merge PDF documents

```rust
use lopdf::dictionary;

use std::collections::BTreeMap;

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream, Bookmark};

pub fn generate_fake_document() -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 48.into()]),
            Operation::new("Td", vec![100.into(), 600.into()]),
            Operation::new("Tj", vec![Object::string_literal("Hello World!")]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc
}

fn main() -> std::io::Result<()> {
    // Generate a stack of Documents to merge.
    let documents = vec![
        generate_fake_document(),
        generate_fake_document(),
        generate_fake_document(),
        generate_fake_document(),
    ];

    // Define a starting `max_id` (will be used as start index for object_ids).
    let mut max_id = 1;
    let mut pagenum = 1;
    // Collect all Documents Objects grouped by a map
    let mut documents_pages = BTreeMap::new();
    let mut documents_objects = BTreeMap::new();
    let mut document = Document::with_version("1.5");

    for mut doc in documents {
        let mut first = false;
        doc.renumber_objects_with(max_id);

        max_id = doc.max_id + 1;

        documents_pages.extend(
            doc
                    .get_pages()
                    .into_iter()
                    .map(|(_, object_id)| {
                        if !first {
                            let bookmark = Bookmark::new(String::from(format!("Page_{}", pagenum)), [0.0, 0.0, 1.0], 0, object_id);
                            document.add_bookmark(bookmark, None);
                            first = true;
                            pagenum += 1;
                        }

                        (
                            object_id,
                            doc.get_object(object_id).unwrap().to_owned(),
                        )
                    })
                    .collect::<BTreeMap<ObjectId, Object>>(),
        );
        documents_objects.extend(doc.objects);
    }

    // "Catalog" and "Pages" are mandatory.
    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    // Process all objects except "Page" type
    for (object_id, object) in documents_objects.iter() {
        // We have to ignore "Page" (as are processed later), "Outlines" and "Outline" objects.
        // All other objects should be collected and inserted into the main Document.
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                // Collect a first "Catalog" object and use it for the future "Pages".
                catalog_object = Some((
                    if let Some((id, _)) = catalog_object {
                        id
                    } else {
                        *object_id
                    },
                    object.clone(),
                ));
            }
            b"Pages" => {
                // Collect and update a first "Pages" object and use it for the future "Catalog"
                // We have also to merge all dictionaries of the old and the new "Pages" object
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref object)) = pages_object {
                        if let Ok(old_dictionary) = object.as_dict() {
                            dictionary.extend(old_dictionary);
                        }
                    }

                    pages_object = Some((
                        if let Some((id, _)) = pages_object {
                            id
                        } else {
                            *object_id
                        },
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            b"Page" => {}     // Ignored, processed later and separately
            b"Outlines" => {} // Ignored, not supported yet
            b"Outline" => {}  // Ignored, not supported yet
            _ => {
                document.objects.insert(*object_id, object.clone());
            }
        }
    }

    // If no "Pages" object found, abort.
    if pages_object.is_none() {
        println!("Pages root not found.");

        return Ok(());
    }

    // Iterate over all "Page" objects and collect into the parent "Pages" created before
    for (object_id, object) in documents_pages.iter() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", pages_object.as_ref().unwrap().0);

            document
                    .objects
                    .insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    // If no "Catalog" found, abort.
    if catalog_object.is_none() {
        println!("Catalog root not found.");

        return Ok(());
    }

    let catalog_object = catalog_object.unwrap();
    let pages_object = pages_object.unwrap();

    // Build a new "Pages" with updated fields
    if let Ok(dictionary) = pages_object.1.as_dict() {
        let mut dictionary = dictionary.clone();

        // Set new pages count
        dictionary.set("Count", documents_pages.len() as u32);

        // Set new "Kids" list (collected from documents pages) for "Pages"
        dictionary.set(
            "Kids",
            documents_pages
                    .into_iter()
                    .map(|(object_id, _)| Object::Reference(object_id))
                    .collect::<Vec<_>>(),
        );

        document
                .objects
                .insert(pages_object.0, Object::Dictionary(dictionary));
    }

    // Build a new "Catalog" with updated fields
    if let Ok(dictionary) = catalog_object.1.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", pages_object.0);
        dictionary.remove(b"Outlines"); // Outlines not supported in merged PDFs

        document
                .objects
                .insert(catalog_object.0, Object::Dictionary(dictionary));
    }

    document.trailer.set("Root", catalog_object.0);

    // Update the max internal ID as wasn't updated before due to direct objects insertion
    document.max_id = document.objects.len() as u32;

    // Reorder all new Document objects
    document.renumber_objects();

    // Set any Bookmarks to the First child if they are not set to a page
    document.adjust_zero_pages();

    // Set all bookmarks to the PDF Object tree then set the Outlines to the Bookmark content map.
    if let Some(n) = document.build_outline() {
        if let Ok(Object::Dictionary(dict)) = document.get_object_mut(catalog_object.0) {
            dict.set("Outlines", Object::Reference(n));
        }
    }

    document.compress();

    // Save the merged PDF.
    // Store file in current working directory.
    // Note: Line is excluded when running doc tests
    if false {
        document.save("merged.pdf").unwrap();
    }

    Ok(())
}
```

* Modify PDF document

```rust
use lopdf::Document;

// For this example to work a parser feature needs to be enabled
#[cfg(not(feature = "async"))]
#[cfg(feature = "nom_parser")]
{
    let mut doc = Document::load("assets/example.pdf").unwrap();

    doc.version = "1.4".to_string();
    doc.replace_text(1, "Hello World!", "Modified text!", None);
    // Store file in current working directory.
    // Note: Line is excluded when running tests
    if false {
        doc.save("modified.pdf").unwrap();
    }
}

#[cfg(feature = "async")]
#[cfg(feature = "nom_parser")]
{
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("Failed to create runtime")
        .block_on(async move {
            let mut doc = Document::load("assets/example.pdf").await.unwrap();
            
            doc.version = "1.4".to_string();
            doc.replace_text(1, "Hello World!", "Modified text!", None);
            // Store file in current working directory.
            // Note: Line is excluded when running tests
            if false {
                doc.save("modified.pdf").unwrap();
            }
    });
}

// For this example to work a parser feature needs to be enabled
#[cfg(not(feature = "async"))]
#[cfg(feature = "nom_parser")]
{
    let mut doc = Document::load("assets/example.pdf").unwrap();

    doc.version = "1.4".to_string();
    
    // Replace exact text matches
    doc.replace_text(1, "Hello World!", "Modified text!", None);
    
    // Replace partial text matches (new method)
    let count = doc.replace_partial_text(1, "Hello", "Hi", None).unwrap();
    println!("Replaced {} occurrences", count);
    
    // Store file in current working directory.
    // Note: Line is excluded when running tests
    if false {
        doc.save("modified.pdf").unwrap();
    }
}

#[cfg(feature = "async")]
#[cfg(feature = "nom_parser")]
{
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("Failed to create runtime")
        .block_on(async move {
            let mut doc = Document::load("assets/example.pdf").await.unwrap();
            
            doc.version = "1.4".to_string();
            
            // Replace exact text matches
            doc.replace_text(1, "Hello World!", "Modified text!", None);
            
            // Replace partial text matches (new method)
            let count = doc.replace_partial_text(1, "Hello", "Hi", None).unwrap();
            println!("Replaced {} occurrences", count);
            
            // Store file in current working directory.
            // Note: Line is excluded when running tests
            if false {
                doc.save("modified.pdf").unwrap();
            }
    });
}
```

* Load PDF with Progress Tracking

You can track the loading progress of PDF documents using the `load_with_options` methods. This is useful for providing user feedback when loading large PDFs.

**Synchronous version (default):**

```rust,no_run
use lopdf::{Document, LoadOptions, ProgressInterval};

#[cfg(not(any(target_arch = "wasm32", feature = "async")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Basic progress tracking with percentage updates
    let options = LoadOptions::new()
        .with_progress(|p| {
            println!("{}%: {}", (p.progress * 100.0) as u8, p.stage_name);
        });

    let doc = Document::load_with_options("input.pdf", options)?;

    // Load with custom progress interval (report every 5%)
    let options = LoadOptions::new()
        .with_progress(|p| {
            if p.stage == 5 {  // Stage 5 is loading objects
                println!("Loading objects: {}/{}", p.items_processed, p.items_total);
            }
        })
        .with_progress_interval(ProgressInterval::Percentage(5.0));

    let doc = Document::load_with_options("input.pdf", options)?;

    // Load from memory with progress tracking
    let pdf_bytes = std::fs::read("input.pdf")?;
    let options = LoadOptions::new()
        .with_progress(|p| {
            eprintln!("[{}%] {}", (p.progress * 100.0) as u8, p.stage_name);
        });

    let doc = Document::load_mem_with_options(&pdf_bytes, options)?;

    Ok(())
}
```

**Async version (for WASM or with `async` feature):**

When targeting WASM or using the `async` feature, `load_mem_with_options` becomes async and yields control back to the event loop periodically. This allows UI updates in browsers and prevents freezing.

```rust,no_run
use lopdf::{Document, LoadOptions, ProgressInterval};

#[cfg(any(target_arch = "wasm32", feature = "async"))]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Progress tracking with async - yields every 10 objects for UI responsiveness
    let pdf_bytes = std::fs::read("input.pdf")?;
    let options = LoadOptions::new()
        .with_progress(|p| {
            eprintln!("[{}%] {}", (p.progress * 100.0) as u8, p.stage_name);
        })
        .with_progress_interval(ProgressInterval::Percentage(5.0));

    // Note the .await - function is async
    let doc = Document::load_mem_with_options(&pdf_bytes, options).await?;

    Ok(())
}
```

**WASM Example:**

```rust
use wasm_bindgen::prelude::*;
use lopdf::{Document, LoadOptions};

#[wasm_bindgen]
pub async fn load_pdf_with_progress(bytes: &[u8]) -> Result<(), JsValue> {
    let options = LoadOptions::new()
        .with_progress(|p| {
            // This callback fires AND the browser can repaint the UI!
            web_sys::console::log_1(
                &format!("{}%: {}", (p.progress * 100.0) as u8, p.stage_name).into()
            );
        });

    // Async version yields to JS event loop every 10 objects
    let doc = Document::load_mem_with_options(bytes, options).await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    Ok(())
}
```

**Progress Stages:**
- Stage 0: Reading file (1%)
- Stage 1: Finding PDF header (1%)
- Stage 2: Parsing version (1%)
- Stage 3: Parsing cross-reference table (35%)
- Stage 4: Parsing trailer (5%)
- Stage 5: Loading objects (55%) - Yields to event loop every 10 objects when async
- Stage 6: Complete (100%)

**Progress Intervals:**
- `ProgressInterval::Percentage(f64)` - Report every N percent (e.g., `5.0` for every 5%)
- `ProgressInterval::Items(usize)` - Report every N items processed (e.g., `10` for every 10 objects)

**Available Methods:**
- `Document::load_with_options(path, options)` - Load from file path with progress (sync)
- `Document::load_from_with_options(source, options)` - Load from reader with progress (sync)
- `Document::load_mem_with_options(buffer, options)` - Load from memory with progress (async when WASM or `async` feature enabled, sync otherwise)

**Key Features:**
- **WASM Support**: Async version prevents UI freezing by yielding to JavaScript event loop
- **Real-time Updates**: Progress callbacks fire AND UI can repaint between yields
- **Backward Compatible**: Sync API preserved when neither WASM nor `async` feature is enabled
- **Strategic Yielding**: Yields after major stages and every 10 objects during loading

All existing loading methods (`load()`, `load_from()`, `load_mem()`) remain unchanged and continue to work without progress tracking.

* Handling Corrupted PDFs with Repair

lopdf can automatically repair PDFs with damaged structure (e.g., missing or invalid `startxref` marker) by reconstructing the cross-reference table. This is similar to qpdf's `--check` behavior and is useful for handling PDFs from older or buggy generators.

**Problem:** PDFs from older generators (mPDF 5.x, TCPDF, etc.) often have corrupted xref tables, causing load failures:
```
Error: failed parsing cross reference table: invalid start value
```

**Solution:** Enable repair mode with `LoadOptions::with_repair(true)`:

```rust,no_run
use lopdf::{Document, LoadOptions};

// Full document load with repair
let options = LoadOptions::new()
    .with_repair(true)
    .with_progress(|p| {
        println!("{}%: {}", (p.progress * 100.0) as u8, p.stage_name);
    });

let doc = Document::load_with_options("corrupted.pdf", options)?;
println!("Loaded {} pages", doc.get_pages().len());
# Ok::<(), lopdf::Error>(())
```

**Repair works with all loading methods:**

```rust,no_run
use lopdf::{Document, LoadOptions};

let options = LoadOptions::new().with_repair(true);

// Method 1: Full document load
let doc = Document::load_with_options("corrupted.pdf", options.clone())?;

// Method 2: Fast metadata extraction
let minimal = Document::load_minimal_with_options("corrupted.pdf", options.clone())?;
println!("Pages: {}", minimal.get_pages().len());

// Method 3: Image extraction
Document::process_images_with_options("corrupted.pdf", options, |img| {
    println!("Found {}x{} image on page {}", img.width, img.height, img.page_number);
    Ok(())
})?;
# Ok::<(), lopdf::Error>(())
```

**How it works:**
1. Scans entire PDF for indirect object definitions (e.g., `5 0 obj`)
2. Builds cross-reference table from discovered object offsets
3. Attempts to locate trailer dictionary
4. Proceeds with normal parsing

**When to use:**
- PDF compression tools processing user-uploaded files
- Document management systems ingesting PDFs from multiple sources
- Batch processing pipelines that shouldn't fail on minor corruption
- Web applications handling PDFs from third-party APIs

**Notes:**
- Repair is opt-in (disabled by default) to maintain strict parsing behavior
- Emits warnings when repair is attempted (use `RUST_LOG=warn` to see)
- Approximately 5-10% of real-world PDFs have minor structural damage
- Repaired PDFs should be validated before use in production

* Fast Metadata Extraction with `load_minimal()`

For quickly extracting basic PDF information without loading the entire document, use the `load_minimal()` API:

```rust,no_run
use lopdf::Document;

#[cfg(not(feature = "async"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Fast metadata extraction (only loads ~3-20 objects)
    let doc = Document::load_minimal("large_document.pdf")?;

    println!("Version: {}", doc.version);
    println!("Page count: {}", doc.get_pages().len());

    // Access metadata if available
    if let Ok(info_id) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Ok(info) = doc.get_dictionary(info_id) {
            // Extract title, author, etc.
        }
    }

    Ok(())
}

#[cfg(feature = "async")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Note: load_minimal is currently only available for synchronous loading
    println!("This example requires the async feature to be disabled");
    Ok(())
}
```

**Performance**: 2-20x faster than `load()` depending on PDF size.

**What's loaded**:
- PDF version, trailer, catalog
- Pages tree structure
- Page objects (without content streams)
- Info dictionary

**What's NOT loaded**:
- Page content, fonts, images, resources
- Annotations, forms, embedded files

**Limitations**:
- Does not support PDFs with object-stream-compressed structural objects
- Returns incomplete document (content operations will fail)

**Use cases**: PDF indexing, file browsers, batch metadata extraction, quick validation

For more details, see [LOAD_MINIMAL.md](LOAD_MINIMAL.md).

* Extract Images with Lazy Loading

For extracting images from PDFs without loading the entire document, use the `process_images_with_callback()` API. This method loads only the structural objects and image XObjects, making it 5-20x faster than loading the full document.

Images are processed via a callback function that's invoked immediately as each image is discovered, enabling progressive/streaming display:

```rust,no_run
use lopdf::Document;

// Process images with immediate callback
Document::process_images_with_callback("large.pdf", |page_image| {
    println!("Page {}: Found {}x{} image",
             page_image.page_number,
             page_image.width,
             page_image.height);

    // Image data is immediately available
    if page_image.filters.contains(&"DCTDecode".to_string()) {
        // JPEG data - can save directly
        std::fs::write(
            format!("page_{}_img_{}.jpg", page_image.page_number, page_image.id.0),
            &page_image.content
        )?;
    }

    Ok(())
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

**PageImage Structure:**

Each callback receives a `PageImage` with:
- `page_number`: Page number (1-indexed)
- `page_id`: PDF object ID of the page
- `id`: PDF object ID of the image
- `width`, `height`: Image dimensions in pixels
- `color_space`: Color space (e.g., "DeviceRGB", "DeviceGray")
- `filters`: Compression filters (e.g., "DCTDecode" for JPEG, "FlateDecode" for zlib)
- `bits_per_component`: Bits per color component (typically 8 or 16)
- `content`: Raw image bytes (owned, potentially compressed)
- `dict`: Complete stream dictionary with all metadata
- `smask_content`: Raw SMask (transparency) stream data (new in v0.40.0)
- `smask_width`, `smask_height`: SMask dimensions in pixels (new in v0.40.0)
- `smask_filters`: SMask compression filters (new in v0.40.0)

#### SMask Support for Transparency (v0.40.0)

Starting with v0.40.0, `process_images_mem` automatically loads SMask (transparency/alpha channel) data alongside images, enabling efficient transparency handling without loading the entire PDF document.

**Key Features:**
- **Automatic SMask detection** - No API changes required
- **Efficient loading** - Only loads necessary SMask objects (~10-20% overhead)
- **15-20x faster** than full document load for PDFs with transparency
- **Memory efficient** - Only loads structural objects + images + SMasks (~5-15% of document)

**Example: Detecting and Using Transparency Data**

```rust,no_run
use lopdf::Document;

// Load PDF into memory
let pdf_bytes = std::fs::read("document.pdf")?;

Document::process_images_mem(&pdf_bytes, |page_image| {
    println!("Image: {}x{}", page_image.width, page_image.height);

    // Check for transparency (SMask) - new in v0.40.0
    if let Some(smask_content) = &page_image.smask_content {
        println!("  ✓ Has transparency!");
        println!("    Alpha channel: {}x{}",
                 page_image.smask_width.unwrap(),
                 page_image.smask_height.unwrap());
        println!("    SMask data: {} bytes", smask_content.len());

        if let Some(filters) = &page_image.smask_filters {
            println!("    SMask compression: {}", filters.join(", "));
        }

        // The SMask content can now be used to:
        // - Composite with the image to create RGBA data
        // - Save as separate alpha channel
        // - Apply transparency effects
    } else {
        println!("  ✗ No transparency");
    }

    Ok(())
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

**What's Loaded:**
- Image XObjects (same as before)
- SMask objects referenced by images (new in v0.40.0)
- SMask stream content (new in v0.40.0)

**What's NOT Loaded:**
- Page content streams
- Fonts and font data
- Annotations, forms, embedded files

**Performance Characteristics:**
- PDFs without transparency: No overhead (same as v0.39.0)
- PDFs with transparency: +10-20% overhead, still 15-20x faster than full load
- Example: 250MB PDF with transparency → 2-3 seconds (vs 45 seconds for full load)

See [`examples/test_smask_extraction.rs`](examples/test_smask_extraction.rs) for a complete example.

**Concurrent Operations:**

For maximum efficiency when you need both metadata and images, use the `_mem` variants with a shared buffer:

```rust,no_run
use lopdf::Document;
use std::sync::Arc;

// Read file once
let pdf_bytes = Arc::new(std::fs::read("file.pdf")?);

// Run two operations concurrently
let bytes1 = Arc::clone(&pdf_bytes);
let handle = std::thread::spawn(move || {
    Document::process_images_mem(&bytes1, |img| {
        println!("Found image on page {}", img.page_number);
        Ok(())
    })
});

// Load metadata concurrently
let minimal = Document::load_minimal_mem(&pdf_bytes)?;

// Wait for image processing
handle.join().unwrap()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

#### Running All Three Operations Concurrently

For web applications that need full document data, metadata, and images simultaneously:

```rust,no_run
use lopdf::{Document, LoadOptions};
use std::sync::Arc;

// Read file once
let pdf_bytes = Arc::new(std::fs::read("file.pdf")?);

// Spawn 3 concurrent operations on the same buffer
let bytes1 = Arc::clone(&pdf_bytes);
let h1 = std::thread::spawn(move || {
    // Full document load with all objects
    Document::load_mem(&bytes1)
});

let bytes2 = Arc::clone(&pdf_bytes);
let h2 = std::thread::spawn(move || {
    // Fast metadata extraction only
    Document::load_minimal_mem(&bytes2)
});

let bytes3 = Arc::clone(&pdf_bytes);
let h3 = std::thread::spawn(move || {
    // Stream all images with callback
    let mut count = 0;
    Document::process_images_mem(&bytes3, |img| {
        count += 1;
        // Process each image immediately (save, display, etc.)
        println!("Image {}: Page {}, {}x{}", count, img.page_number, img.width, img.height);
        Ok(())
    })?;
    Ok::<_, lopdf::Error>(count)
});

// Wait for all three to complete
let full_doc = h1.join().unwrap()?;
let minimal = h2.join().unwrap()?;
let image_count = h3.join().unwrap()?;

println!("✓ Full doc: {} objects loaded", full_doc.objects.len());
println!("✓ Minimal: {} pages", minimal.get_pages().len());
println!("✓ Extracted {} images", image_count);

// All three operations shared the same 100MB buffer!
// Total memory: ~100MB (not 300MB)
// All operations ran in parallel on different CPU cores
# Ok::<(), Box<dyn std::error::Error>>(())
```

**Key Benefits:**
- **Single file read** - Only one disk I/O operation
- **Shared memory** - All operations use the same buffer (Arc has zero-copy sharing)
- **True parallelism** - Three CPU threads processing simultaneously
- **Efficient for web apps** - Get everything you need in parallel

**Performance:** 5-20x faster than `load()` + `get_page_images()` for large PDFs.

**Available Methods:**
- `Document::process_images_with_callback(path, callback)` - Load from file path
- `Document::process_images_from(source, callback)` - Load from reader
- `Document::process_images_mem(buffer, callback)` - Load from memory (best for concurrent ops)

See [`examples/extract_images_lazy.rs`](examples/extract_images_lazy.rs) for complete examples.

* Save PDF with Object Streams (Modern Format)

Object streams allow multiple non-stream objects to be compressed together, significantly reducing file size.

```rust,no_run
use lopdf::{Document, SaveOptions};

#[cfg(not(feature = "async"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load existing PDF
    let mut doc = Document::load("input.pdf")?;

    // Save with modern features (object streams + cross-reference streams)
    // This typically reduces file size by 11-38%
    let mut file = std::fs::File::create("output.pdf")?;
    doc.save_modern(&mut file)?;

    // For more control, use SaveOptions
    let options = SaveOptions::builder()
        .use_object_streams(true)        // Enable object streams
        .use_xref_streams(true)          // Enable cross-reference streams
        .max_objects_per_stream(200)     // Max objects per stream (default: 100)
        .compression_level(9)            // Compression level 0-9 (default: 6)
        .build();

    let mut file2 = std::fs::File::create("output_custom.pdf")?;
    doc.save_with_options(&mut file2, options)?;
    
    Ok(())
}

#[cfg(feature = "async")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // For async feature, you need to use tokio runtime
    println!("This example requires the async feature to be disabled");
    Ok(())
}
```

### Complete Example: Creating and Saving with Object Streams

```rust
use lopdf::{Document, SaveOptions};
use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create or load a document
    let mut doc = Document::with_version("1.5");
    // ... add content to document ...

    // Method 1: Quick modern save (recommended)
    let mut file = File::create("output.pdf")?;
    doc.save_modern(&mut file)?;

    // Method 2: Custom settings for maximum compression
    let options = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .max_objects_per_stream(200)
        .compression_level(9)
        .build();

    let mut file2 = File::create("output_max_compressed.pdf")?;
    doc.save_with_options(&mut file2, options)?;

    // Compare file sizes (if traditional file exists)
    if std::path::Path::new("output_traditional.pdf").exists() {
        let traditional_size = std::fs::metadata("output_traditional.pdf")?.len();
        let modern_size = std::fs::metadata("output.pdf")?.len();
        let reduction = 100.0 - (modern_size as f64 / traditional_size as f64 * 100.0);
        println!("Size reduction: {:.1}%", reduction);
    }
    
    Ok(())
}
```

For more examples, see:
- [`examples/object_streams.rs`](examples/object_streams.rs) - Creating PDFs with object streams
- [`examples/compress_existing_pdf.rs`](examples/compress_existing_pdf.rs) - Compress existing PDFs
- [`examples/analyze_object_streams.rs`](examples/analyze_object_streams.rs) - Analyze object stream usage

## Object Streams Support

lopdf now includes full support for creating and reading PDF object streams (PDF 1.5+ feature). Object streams provide significant file size reduction by compressing multiple non-stream objects together.

### Key Benefits

- **File size reduction**: 11-61% smaller PDFs depending on content
- **Modern PDF compliance**: Full PDF 1.5+ specification support
- **Backward compatibility**: All existing APIs remain unchanged
- **Performance**: <2ms to check 1000 objects for compression eligibility

### Creating Object Streams Directly

```rust
use lopdf::{Object, ObjectStream, dictionary};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an object stream with custom settings
    let mut obj_stream = ObjectStream::builder()
        .max_objects(100)      // Maximum objects per stream
        .compression_level(6)  // zlib compression level (0-9)
        .build();

    // Add objects to the stream
    obj_stream.add_object((1, 0), Object::Integer(42))?;
    obj_stream.add_object((2, 0), Object::Name(b"Example".to_vec()))?;
    obj_stream.add_object((3, 0), Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica"
    }))?;

    // Convert to a stream object
    let stream = obj_stream.to_stream_object()?;
    # Ok::<(), Box<dyn std::error::Error>>(())
# }
```

### Object Eligibility

Not all objects can be compressed into object streams. The following objects are **excluded**:

- Stream objects (content streams, image streams, etc.)
- Cross-reference streams (Type = XRef)
- Object streams themselves (Type = ObjStm)
- Encryption dictionary (when referenced by trailer's Encrypt entry)
- Objects with generation number > 0
- Document catalog in linearized PDFs only

All other objects, including structural objects (Catalog, Pages, Page) and trailer-referenced objects (except encryption), can be compressed.

### Cross-reference Streams

When using `save_modern()` or enabling `use_xref_streams(true)`, lopdf creates binary cross-reference streams instead of traditional ASCII cross-reference tables. This provides additional space savings and is part of the PDF 1.5+ specification.

### SaveOptions Reference

The `SaveOptions` builder provides fine-grained control over PDF compression:

```rust
use lopdf::SaveOptions;

let options = SaveOptions::builder()
    .use_object_streams(true)        // Enable object streams (default: false)
    .use_xref_streams(true)          // Enable xref streams (default: false)
    .max_objects_per_stream(200)     // Max objects per stream (default: 100)
    .compression_level(9)            // zlib level 0-9 (default: 6)
    .build();
```

## FAQ

* Why does the library keep everything in memory as high-level objects until finally serializing the entire document?

  Normally, a PDF document won't be very large, ranging from tens of KB to hundreds of MB. Memory size is not a bottle neck for today's computer.
  By keeping the whole document in memory, the stream length can be pre-calculated, no need to use a reference object for the Length entry.
  The resulting PDF file is smaller for distribution and faster for PDF consumers to process.

  Producing is a one-time effort, while consuming is many more.

* How do object streams affect memory usage?

  Object streams actually help reduce memory usage during document creation. When enabled, multiple small objects are grouped and compressed together, reducing the overall memory footprint. The compression happens during the save operation, so the in-memory representation remains the same until `save_with_options()` or `save_modern()` is called.

* What PDF versions support object streams?

  Object streams were introduced in PDF 1.5. When using `save_modern()` or object streams, lopdf automatically ensures the document version is at least 1.5. For maximum compatibility with older PDF readers, you can use the traditional `save()` method.

* Can I analyze existing PDFs to see if they use object streams?

  Yes! lopdf can read and parse object streams from existing PDFs. Use the `Document::load()` method to open any PDF, and lopdf will automatically handle object streams if present. See the examples directory for analysis tools.
