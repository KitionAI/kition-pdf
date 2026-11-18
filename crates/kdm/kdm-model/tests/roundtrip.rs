//! Round trips: a PDF written to a replica reads back as the same document. The base comes
//! back byte for byte, and every edit object for object, for the repository's PDFs, an edited
//! document, and a local corpus (`KITION_PDF_CORPUS=<dir>`, every `.pdf` under it).

mod common;

use std::sync::Arc;

use common::{add_note, comments, corpus_pdfs, field_value, fill, first, fixture, page_count, peer, repo_pdfs, view};
use kdm_model::{PdfModel, PdfSession};
use kition_kit::kdm::ActorId;
use kition_kit::session::{KitModel, MemoryBlobs};
use kition_pdf_cos::{Document, ObjRef, SaveOptions, write_incremental};

/// `doc` through a replica and a peer opened from its snapshot: the peer's view.
fn through_replica(doc: &Document) -> PdfSession {
    let a = first(doc);
    peer(&a)
}

/// The peer's view is `doc`: the same base bytes, the same edits (base objects by number, added
/// objects through their keys), the same trailer, and it saves to a file that opens.
fn assert_same(doc: &Document, back: &PdfSession) {
    let view = back.document();
    assert_eq!(view.bytes().as_slice(), doc.bytes().as_slice(), "the base comes back byte for byte");
    assert_eq!(view.overlay_entries().count(), doc.overlay_entries().count(), "every edit comes back");
    let max = view.next_num();
    for (num, generation, object) in doc.overlay_entries() {
        if num <= back.model().ids().base_max() {
            assert_eq!(view.generation(num), generation, "object {num}'s generation");
            let theirs = view.try_get(num).ok().map(|o| (*o).clone());
            match object {
                Some(object) if !has_new_refs(&object, back.model().ids().base_max()) => {
                    assert_eq!(theirs.as_ref(), Some(&*object), "object {num}");
                }
                _ => {}
            }
        }
    }
    assert!(max > back.model().ids().base_max());
    assert_eq!(comments(view), comments(doc), "the comments");
    assert_eq!(page_count(view), page_count(doc), "the pages");
    let saved = write_incremental(view, &SaveOptions::default()).unwrap_or_else(|e| panic!("save: {e}"));
    Document::open(Arc::new(saved)).unwrap_or_else(|e| panic!("reopen: {e}"));
}

/// Whether `object` references an object added since the base (numbered differently by each
/// replica, so compared through [`view`] instead).
fn has_new_refs(object: &kition_pdf_cos::Object, max: u32) -> bool {
    use kition_pdf_cos::Object;
    match object {
        Object::Ref(r) => r.num > max,
        Object::Array(items) => items.iter().any(|o| has_new_refs(o, max)),
        Object::Dict(d) => d.iter().any(|(_, o)| has_new_refs(o, max)),
        Object::Stream(s) => s.dict.iter().any(|(_, o)| has_new_refs(o, max)),
        _ => false,
    }
}

#[test]
fn a_fresh_document_comes_back_byte_for_byte() {
    let doc = fixture(3);
    let back = through_replica(&doc);
    assert_eq!(back.document().bytes().as_slice(), doc.bytes().as_slice());
    assert!(!back.document().is_modified(), "no edits");
    let exported = back.model().export(back.replica(), "x.pdf", back.blobs()).unwrap_or_else(|e| panic!("export: {e}"));
    assert_eq!(exported, doc.bytes().as_slice(), "an unedited document exports as its file");
}

#[test]
fn an_edited_document_comes_back_object_for_object() {
    let mut doc = fixture(3);
    add_note(&mut doc, 0, "first page");
    add_note(&mut doc, 2, "last page");
    fill(&mut doc, "Ada");
    kition_pdf_organize::rotate_pages(&mut doc, &[1], 90).unwrap_or_else(|e| panic!("rotate: {e}"));
    kition_pdf_organize::set_info(&mut doc, "Title", "Round trip").unwrap_or_else(|e| panic!("info: {e}"));
    let a = first(&doc);
    let b = peer(&a);
    assert_same(&doc, &b);
    assert_eq!(field_value(b.document()), ["Ada"]);
    assert_eq!(kition_pdf_organize::info(b.document(), "Title").as_deref(), Some("Round trip"));
    assert_eq!(view(&a), view(&b), "the same view, keyed the same");
    // Every reference between added objects resolves on the peer as on the original.
    for (num, _, object) in b.document().overlay_entries() {
        if let Some(object) = object {
            visit_refs(&object, &mut |r| {
                if r.num > b.model().ids().base_max() {
                    assert!(b.document().is_edited(r.num), "object {num} references added object {} the peer has", r.num);
                }
            });
        }
    }
}

fn visit_refs(object: &kition_pdf_cos::Object, f: &mut impl FnMut(ObjRef)) {
    use kition_pdf_cos::Object;
    match object {
        Object::Ref(r) => f(*r),
        Object::Array(items) => items.iter().for_each(|o| visit_refs(o, f)),
        Object::Dict(d) => d.iter().for_each(|(_, o)| visit_refs(o, f)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, o)| visit_refs(o, f)),
        _ => {}
    }
}

#[test]
fn the_model_imports_and_loads_a_file() {
    let doc = fixture(2);
    let mut blobs = MemoryBlobs::default();
    let actor = ActorId::random();
    let replica = PdfModel::import("x.pdf", doc.bytes(), &mut blobs, actor).unwrap_or_else(|e| panic!("import: {e}"));
    let model = PdfModel::load(&replica, &blobs).unwrap_or_else(|e| panic!("load: {e}"));
    assert_eq!(model.doc.bytes().as_slice(), doc.bytes().as_slice());
}

#[test]
fn the_repository_pdfs_round_trip() {
    let pdfs = repo_pdfs();
    assert!(!pdfs.is_empty(), "the repository's PDFs are found");
    for path in pdfs {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let doc = Document::open(Arc::new(bytes)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let back = through_replica(&doc);
        assert_same(&doc, &back);
        // And with an edit on top.
        let mut edited = doc.clone();
        if page_count(&edited) > 0 {
            add_note(&mut edited, 0, "corpus note");
        }
        let back = through_replica(&edited);
        assert_same(&edited, &back);
    }
}

/// Every PDF of `KITION_PDF_CORPUS` that opens comes back the same, unedited and with a comment.
#[test]
fn the_local_corpus_round_trips() {
    let pdfs = corpus_pdfs();
    if pdfs.is_empty() {
        eprintln!("KITION_PDF_CORPUS not set or empty: skipped");
        return;
    }
    let mut checked = 0;
    for path in pdfs {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = Document::open(Arc::new(bytes)) else { continue };
        let back = through_replica(&doc);
        assert_eq!(back.document().bytes().as_slice(), doc.bytes().as_slice(), "{}", path.display());
        let mut edited = doc.clone();
        if page_count(&edited) > 0 && kition_pdf_annot::add_annotation(&mut edited, &note(), &common::meta("corpus")).is_ok() {
            let back = through_replica(&edited);
            assert_eq!(comments(back.document()), comments(&edited), "{}", path.display());
        }
        checked += 1;
    }
    eprintln!("{checked} corpus PDFs round-tripped");
}

fn note() -> kition_pdf_annot::NewAnnotation {
    kition_pdf_annot::NewAnnotation {
        page: 0,
        shape: kition_pdf_annot::Shape::Note { at: [50.0, 50.0], icon: kition_pdf_annot::NoteIcon::Comment },
        style: kition_pdf_annot::Style::default(),
        contents: "corpus".into(),
        author: "Test".into(),
    }
}
