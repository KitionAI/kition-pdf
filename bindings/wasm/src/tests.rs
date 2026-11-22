use super::*;
use kition_kit::testing::check_contract;

/// A two-page PDF.
fn fixture() -> Vec<u8> {
    let mut doc = PdfDocument::create("Report.pdf", 612, 792).unwrap();
    doc.execute("page_insert_blank", json!({ "at": 2 })).unwrap();
    doc.save(Some("Report.pdf")).unwrap()
}

#[test]
fn keeps_the_kit_contract() {
    check_contract::<PdfDocument>(&fixture(), "Report.pdf", |doc| doc.execute("page_rotate", json!({ "degrees": 90, "pages": [1] })).map(|_| ()))
        .unwrap();
}

/// Pixels that are not white paper.
fn marked_pixels(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>().0.iter().filter(|px| px[..3] != [255, 255, 255]).count()
}

#[test]
fn stacks_the_pages_on_a_pasteboard_and_renders_them_sharp() {
    let doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
    assert_eq!((doc.width(), doc.height()), (612 + 80, 40 + 792 + 24 + 792 + 40));
    assert_eq!(doc.render(10.0, 10.0, 1.0, 1, 1).unwrap(), PASTEBOARD);
    assert_eq!(doc.render(300.0, 400.0, 1.0, 1, 1).unwrap(), [255, 255, 255, 255], "white paper");
    assert_eq!(doc.render(300.0, 845.0, 1.0, 1, 1).unwrap(), PASTEBOARD, "the gap");
    // Zoomed in, a piece of the second page is rendered at that scale.
    assert_eq!(doc.render(300.0, 1000.0, 3.0, 2, 2).unwrap(), [255; 16]);
}

#[test]
fn failed_page_renders_return_errors_without_losing_edits_or_poisoning_other_pages() {
    let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
    doc.execute("doc_watermark", json!({ "pages": [1], "text": "DRAFT", "opacity": 1.0 })).unwrap();
    let saved = doc.save(None).unwrap();
    // Observe the immutable display and prepared-save state rather than saving again.
    let display_before = doc.doc().unwrap().display.clone();
    let prepared_before = doc.saved.borrow().as_ref().unwrap().clone();
    let revision = doc.revision();
    // Simulate a page the metadata inspector accepted but the renderer cannot resolve.
    // The real PageRenderer produces the error; no fake successful pixels are injected.
    let one_page = PdfDocument::create("Single.pdf", 612, 792).unwrap().save(None).unwrap();
    doc.with_renderer(|renderer| renderer.pages = PageRenderer::new(Arc::new(one_page), RenderConfig::default()));
    for scale in [1.0, 2.0, 0.5] {
        let error = doc.render(40.0, 856.0, scale, 16, 16).unwrap_err();
        assert!(error.contains("page 2") && error.contains("does not exist"), "{error}");
        assert!(error.contains("scale") && error.contains("Retry"), "{error}");
    }
    assert!(doc.render_area(40, 856, 72, 888, 16, 16).unwrap_err().contains("page 2"));
    assert!(!doc.renderer.borrow().as_ref().unwrap().full.contains_key(&1));
    assert_eq!(doc.render(40.0, 40.0, 1.0, 1, 1).unwrap(), [255; 4], "other pages remain usable");
    assert!(doc.dirty());
    assert_eq!(doc.revision(), revision);
    assert!(Arc::ptr_eq(&doc.doc().unwrap().display, &display_before), "rendering must retain the immutable display");
    assert!(doc.saved.borrow().as_ref().is_some_and(|bytes| Arc::ptr_eq(bytes, &prepared_before)), "rendering must retain the prepared save");
    assert_eq!(doc.saved.borrow().as_deref().map(Vec::as_slice), Some(saved.as_slice()), "prepared save bytes remain unchanged");
    *doc.renderer.borrow_mut() = None;
    assert_eq!(doc.render(40.0, 856.0, 1.0, 1, 1).unwrap(), [255; 4], "a failed page can be retried");
}

#[test]
fn truncated_page_is_not_reported_as_a_complete_render() {
    let mut renderer = PageRenderer::new(Arc::new(fixture()), RenderConfig::default());
    let mut page = renderer.render(RenderRequest { tile: Some(Tile { x: 0, y: 0, w: 1, h: 1 }), ..Default::default() });
    assert!(check_rendered_page(&page).is_ok());
    page.warnings.push(kition_pdf_render::RenderWarning::ContentTruncated);
    assert!(check_rendered_page(&page).unwrap_err().contains("content exceeded the rendering budget"));
}

#[test]
fn edits_through_the_automation_tools_and_saves_incrementally() {
    let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
    let before = fixture().len();
    // A watermark drawn on the first page.
    let blank = marked_pixels(&doc.render(40.0, 40.0, 1.0, 612, 792).unwrap());
    doc.execute("doc_watermark", json!({ "pages": [1], "text": "DRAFT", "opacity": 1.0 })).unwrap();
    assert!(doc.dirty());
    assert!(marked_pixels(&doc.render(40.0, 40.0, 1.0, 612, 792).unwrap()) > blank + 50, "the watermark is drawn");
    doc.execute("page_delete", json!({ "pages": [2] })).unwrap();
    assert_eq!(doc.inspect().unwrap()["pages"].as_array().map(Vec::len), Some(1));
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["pages"].as_array().map(Vec::len), Some(2));
    let saved = doc.save(None).unwrap();
    assert!(saved.len() > before && saved.starts_with(&fixture()[..8]), "an incremental save keeps the original bytes first");
    doc.mark_saved();
    assert!(!doc.dirty());
    assert!(doc.disabled_commands().iter().all(|id| id != "edit.undo"), "undo history survives a save");
}

#[test]
fn leaves_out_tools_that_reach_files_printers_or_any_command() {
    let doc = PdfDocument::create("New.pdf", 0, 0).unwrap();
    let ids: Vec<String> = doc.commands().into_iter().map(|command| command.id).collect();
    for left_out in
        ["doc_open", "doc_save", "doc_combine", "page_insert_file", "printers", "doc_print", "sign_trust", "command_run", "comments_summarize"]
    {
        assert!(!ids.iter().any(|id| id == left_out), "{left_out}");
    }
    assert!(ids.iter().any(|id| id == "edit.undo") && ids.iter().any(|id| id == "page_rotate"));
    let mut doc = doc;
    assert!(doc.execute("doc_save", json!({ "path": "/tmp/x.pdf" })).is_err());
    assert!(doc.execute("doc_watermark", json!({ "file": "/etc/hosts" })).unwrap_err().contains("names a file"));
    let watermark = doc.commands().into_iter().find(|command| command.id == "doc_watermark").unwrap();
    assert!(watermark.schema.unwrap().get("file").is_none(), "file parameters are not offered");
}

#[test]
fn text_selection_is_exposed_as_a_read_only_sdk_command() {
    let mut doc = PdfDocument::open("Selection.pdf", &fixture()).unwrap();
    let command = doc.commands().into_iter().find(|command| command.id == "text_select").unwrap();
    assert!(!command.mutates);
    let schema = command.schema.unwrap();
    assert!(schema.get("doc").is_none(), "the SDK supplies its active document");
    for key in ["page", "end_page", "anchor", "focus", "mode"] {
        assert!(schema.get(key).is_some(), "selection parameter {key} is exposed");
    }
    let revision = doc.revision();
    let bytes = doc.doc().unwrap().bytes.clone();
    let result = doc.execute("text_select", json!({"page":1,"end_page":2,"anchor":[1,1],"focus":[2,2]})).unwrap();
    assert_eq!(result["text"], "\n");
    assert_eq!(result["pages"].as_array().unwrap().len(), 2);
    assert_eq!(doc.revision(), revision);
    assert!(!doc.dirty());
    assert!(Arc::ptr_eq(&doc.doc().unwrap().bytes, &bytes));
}

/// Whether the replica recorded the last edit as a rebase (a full rewrite of the file).
fn rebased(doc: &PdfDocument) -> bool {
    doc.auto.session().kdm(doc.id).is_some_and(|kdm| kdm.last_written().rebased)
}

/// Runs `command` and answers the lease it asked for and whether the replica rebased.
fn run(doc: &mut PdfDocument, command: &str, params: Value) -> (String, bool) {
    let scope = doc.lease_scope(command);
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"));
    (scope, rebased(doc))
}

/// The editor takes the whole document's lease (`*`, standard 2.0 M21) exactly for the edits the
/// replica records as a rebase: those rewrite the file in full, and collaborators' edits made
/// against the old file meanwhile would be dropped. Every other edit merges and takes none.
#[test]
fn asks_for_the_whole_documents_lease_exactly_when_an_edit_rewrites_the_file() {
    let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
    doc.version().unwrap();
    assert_eq!(run(&mut doc, "page_rotate", json!({ "degrees": 90, "pages": [1] })), (String::new(), false));
    assert_eq!(run(&mut doc, "doc_watermark", json!({ "pages": [1], "text": "DRAFT" })), (String::new(), false));
    assert_eq!(run(&mut doc, "redact_mark", json!({ "page": 1, "rect": [72, 72, 200, 120] })), (String::new(), false));
    assert_eq!(doc.lease_scope("edit.undo"), "", "undoing an edit that merges");

    assert_eq!(run(&mut doc, "redact_apply", json!({})), ("*".to_owned(), true));
    assert_eq!(doc.lease_scope("edit.undo"), "*", "undoing the rewrite goes back to the old file");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(doc.lease_scope("edit.redo"), "*", "redoing it rewrites again");
    doc.execute("edit.redo", json!({})).unwrap();

    assert_eq!(doc.lease_scope("pdfa_convert"), "", "an unencrypted file converts in place");
    assert_eq!(run(&mut doc, "doc_protect", json!({ "open_password": "open" })), ("*".to_owned(), true));
    assert_eq!(doc.lease_scope("pdfa_convert"), "*", "converting removes the encryption");
    assert_eq!(run(&mut doc, "doc_unprotect", json!({})), ("*".to_owned(), true));
    assert_eq!(run(&mut doc, "page_insert_blank", json!({ "at": 1 })), (String::new(), false));
    assert_eq!(doc.lease_scope("no_such_command"), "");

    // Sanitizing rewrites the file too (on one with something to remove).
    let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
    doc.version().unwrap();
    assert_eq!(
        run(&mut doc, "comment_add", json!({ "page": 1, "type": "rectangle", "rect": [72, 72, 96, 96], "contents": "Hidden?" })),
        (String::new(), false)
    );
    assert_eq!(run(&mut doc, "doc_remove_hidden", json!({})), ("*".to_owned(), true));
}

#[test]
fn probe() {
    let doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
    println!("{} x {}", doc.width(), doc.height());
    println!("{}", doc.inspect().unwrap());
    let commands = doc.commands();
    println!("{} commands; menus {:?}", commands.len(), commands.iter().filter_map(|c| c.menu.clone()).collect::<std::collections::BTreeSet<_>>());
    println!("unfiled {:?}", commands.iter().filter(|c| c.menu.is_none()).map(|c| c.id.clone()).collect::<Vec<_>>());
    for command in &commands {
        if let Some(schema) = &command.schema {
            kition_kit::schema::validate(schema).unwrap_or_else(|error| panic!("{}: {error}", command.id));
        }
    }
}

#[test]
fn paints_the_backdrop_around_the_pages() {
    use kition_kit::KitDocument;
    let mut doc = PdfDocument::create("Blank.pdf", 612, 792).unwrap();
    doc.set_backdrop("#f6f5f4").unwrap();
    assert!(doc.set_backdrop("green").is_err());
    // The canvas corner lies outside the first page.
    let corner = doc.render(0.0, 0.0, 1.0, 4, 4).unwrap();
    assert_eq!(&corner[..4], &[0xf6, 0xf5, 0xf4, 255]);
}

#[test]
fn takes_a_font_for_chinese_text_a_pdf_does_not_embed() {
    assert_eq!(add_font(b"not a font"), 0);
    // A public font, so a missing checkout fails the test instead of skipping it.
    let font = std::env::var_os("KITION_FONTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kition-fonts"))
        .join("fonts/droid-sans-fallback/DroidSansFallbackFull.ttf");
    let bytes = std::fs::read(&font).unwrap_or_else(|e| {
        panic!(
            "{}: {e}. This test needs Droid Sans Fallback from the public kition-fonts: clone \
             https://github.com/KitionAI/kition-fonts next to this repository, or set KITION_FONTS_DIR to a checkout",
            font.display()
        )
    });
    // Adobe-GB1, Adobe-CNS1, and Adobe-Japan1; it has no Hangul.
    assert_eq!(add_font(&bytes), 3);
}

#[test]
fn the_catalog_flags_as_mutating_exactly_what_changes_the_document() {
    let doc = PdfDocument::create("Flags.pdf", 0, 0).unwrap();
    let mutates = |id: &str| doc.commands().into_iter().find(|command| command.id == id).unwrap_or_else(|| panic!("no {id}")).mutates;
    for id in ["edit.undo", "edit.redo", "doc_background", "doc_remove_hidden", "doc_flatten"] {
        assert!(mutates(id), "{id} changes the document");
    }
    for id in ["doc_info", "text_extract", "comment_list", "render_preview", "doc_inspect"] {
        assert!(!mutates(id), "{id} does not change the document");
    }
}

#[test]
fn commands_flagged_as_not_mutating_never_commit_to_the_replica() {
    let bytes = fixture();
    let commands = PdfDocument::open("Report.pdf", &bytes).unwrap().commands();
    for command in commands.iter().filter(|command| !command.mutates) {
        let mut doc = PdfDocument::open("Report.pdf", &bytes).unwrap();
        let before = doc.version().unwrap();
        let _ = doc.execute(&command.id, json!({}));
        assert_eq!(doc.version().unwrap(), before, "{} committed to the replica", command.id);
        assert!(!doc.dirty(), "{} made the document unsaved", command.id);
    }
}
