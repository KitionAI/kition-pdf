//! Seeded random multi-session convergence: random edits (comments added, edited and deleted,
//! form fills, pages rotated, moved, inserted and deleted, now and then a full rewrite) on two or
//! three sessions with random exchanges and undos; after a final exchange every replica has the
//! same state hash and the same view, and every view saves to a file that opens.

mod common;

use common::{assert_converged, edit, first, fixture, page_count, peer, replace, rewritten, send, sync_all};
use kdm_model::PdfSession;
use kition_pdf_cos::Document;

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// One random edit of `doc`, as the engine's commands make them. Edits a document can refuse
/// (a page that went away meanwhile) are skipped, as a refused command is.
fn random_edit(doc: &mut Document, rng: &mut Rng, n: usize) {
    let pages = page_count(doc);
    let comments = kition_pdf_annot::summaries(doc).into_iter().filter(|s| s.subtype != "Widget").collect::<Vec<_>>();
    match rng.below(9) {
        0 | 1 if pages > 0 => {
            let text = format!("note {n}");
            let note = kition_pdf_annot::NewAnnotation {
                page: rng.below(pages),
                shape: kition_pdf_annot::Shape::Note { at: [100.0, 100.0], icon: kition_pdf_annot::NoteIcon::Comment },
                style: kition_pdf_annot::Style::default(),
                contents: text.clone(),
                author: "Test".into(),
            };
            let _ = kition_pdf_annot::add_annotation(doc, &note, &common::meta(&text));
        }
        2 if !comments.is_empty() => {
            let c = &comments[rng.below(comments.len())];
            let _ = kition_pdf_annot::set_contents(doc, c.page, c.index, &format!("edited {n}"), &common::meta(&format!("m{n}")));
        }
        3 if !comments.is_empty() => {
            let c = &comments[rng.below(comments.len())];
            let _ = kition_pdf_annot::delete_annotation(doc, c.page, c.index);
        }
        4 if !kition_pdf_forms::fields(doc).is_empty() => {
            let _ = kition_pdf_forms::set_value(doc, "Name", &kition_pdf_forms::FieldValue::Text(format!("value {n}")));
        }
        5 if pages > 0 => {
            let _ = kition_pdf_organize::rotate_pages(doc, &[rng.below(pages)], 90);
        }
        6 if pages > 1 => {
            let _ = kition_pdf_organize::move_pages(doc, &[rng.below(pages)], rng.below(pages));
        }
        7 if pages > 1 => {
            let _ = kition_pdf_organize::delete_pages(doc, &[rng.below(pages)]);
        }
        8 if pages < 8 => {
            let _ = kition_pdf_organize::insert_blank_page(doc, rng.below(pages + 1), 612.0, 792.0);
        }
        _ => {}
    }
}

fn run(seed: u64, peers: usize, steps: usize) {
    let mut rng = Rng(seed);
    let a = first(&fixture(3));
    let mut sessions: Vec<PdfSession> = vec![a];
    for _ in 1..peers {
        let p = peer(&sessions[0]);
        sessions.push(p);
    }
    for n in 0..steps {
        let i = rng.below(peers);
        match rng.below(100) {
            0..=2 => {
                // A full rewrite: a rebase every peer must follow.
                replace(&mut sessions[i], rewritten);
            }
            3..=9 => {
                let _ = sessions[i].undo();
            }
            10..=12 => {
                let _ = sessions[i].redo();
            }
            13..=29 => {
                let j = rng.below(peers);
                if i != j {
                    let (from, to) = if i < j {
                        let (left, right) = sessions.split_at_mut(j);
                        (&left[i], &mut right[0])
                    } else {
                        let (left, right) = sessions.split_at_mut(i);
                        (&right[0], &mut left[j])
                    };
                    send(from, to);
                }
            }
            _ => {
                edit(&mut sessions[i], |doc| random_edit(doc, &mut rng, n));
            }
        }
        if rng.chance(2) {
            sessions[i].retain_steps(rng.below(20), rng.below(5));
        }
    }
    sync_all(&mut sessions);
    for pair in sessions.windows(2) {
        assert_converged(&pair[0], &pair[1]);
    }
}

#[test]
fn two_sessions_converge() {
    for seed in [1, 7, 42] {
        run(seed, 2, 160);
    }
}

#[test]
fn three_sessions_converge() {
    for seed in [3, 11] {
        run(seed, 3, 200);
    }
}
