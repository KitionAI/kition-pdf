//! Bounded, read-only selection over the renderer's existing page-text geometry.

use crate::{Args, Automation, PageText, Result, ToolError, failed};
use serde_json::{Value, json};

const MAX_PAGES: usize = 64;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_RECTS: usize = 16_384;

fn point(args: &Args, key: &str) -> Result<[f32; 2]> {
    let values = args.nums::<2>(key)?.ok_or_else(|| Args::missing(key))?;
    if values.iter().any(|value| value.abs() > f64::from(f32::MAX)) {
        return Err(Args::wrong(key, "two finite coordinates representable as f32"));
    }
    Ok(values.map(|value| value as f32))
}

#[derive(Default)]
struct Budget {
    bytes: usize,
    rects: usize,
}

impl Budget {
    fn add(&mut self, text: &str, rects: &[[f32; 4]], separator: bool) -> Result<()> {
        let bytes = self.bytes.saturating_add(text.len()).saturating_add(usize::from(separator));
        let count = self.rects.saturating_add(rects.len());
        if bytes > MAX_TEXT_BYTES {
            return Err(failed("text selection exceeds 1 MiB UTF-8; select less text"));
        }
        if count > MAX_RECTS {
            return Err(failed("text selection exceeds 16384 rectangles; select less text"));
        }
        if rects.iter().flatten().any(|value| !value.is_finite()) {
            return Err(failed("text selection contains invalid page geometry"));
        }
        self.bytes = bytes;
        self.rects = count;
        Ok(())
    }
}

/// Both reconstruction modes add one byte between line groups (newline or tab).
/// A column also adds a space across skipped glyphs on the same line. Count exact
/// text bytes and rectangle runs before those methods allocate their output.
fn preflight(text: &PageText, indices: impl Iterator<Item = usize>, budget: &Budget, separator: bool) -> Result<()> {
    let mut bytes = budget.bytes.saturating_add(usize::from(separator));
    let mut rects = budget.rects;
    let mut previous: Option<(usize, u32)> = None;
    for index in indices {
        let (Some(glyph), Some(&line), Some(&space)) = (text.glyphs.get(index), text.line_of.get(index), text.space_before.get(index)) else {
            return Err(failed("text selection contains incomplete glyph metadata"));
        };
        let split = previous.is_none_or(|(prior, prior_line)| prior.checked_add(1) != Some(index) || prior_line != line);
        if split {
            rects = rects.saturating_add(1);
        }
        if previous.is_some() && (split || space) {
            bytes = bytes.saturating_add(1);
        }
        bytes = bytes.saturating_add(glyph.text.len());
        if bytes > MAX_TEXT_BYTES {
            return Err(failed("text selection exceeds 1 MiB UTF-8; select less text"));
        }
        if rects > MAX_RECTS {
            return Err(failed("text selection exceeds 16384 rectangles; select less text"));
        }
        previous = Some((index, line));
    }
    Ok(())
}

impl Automation {
    pub(crate) fn text_select(&mut self, args: &Args) -> Result<Value> {
        let document = self.doc(args)?;
        let (id, count) = (document.id, document.info.pages.len());
        let page_index = |number, key| {
            usize::try_from(number)
                .ok()
                .and_then(|page| page.checked_sub(1))
                .filter(|page| *page < count)
                .ok_or_else(|| ToolError::InvalidArgs(format!("{key} must be between 1 and {count}")))
        };
        let first_number = args.int("page")?;
        let mut first = page_index(first_number, "page")?;
        let mut last = page_index(args.opt_int("endPage")?.unwrap_or(first_number), "endPage")?;
        let mut anchor = point(args, "anchor")?;
        let mut focus = point(args, "focus")?;
        let column = match args.opt_str("mode")?.unwrap_or("range") {
            "range" => false,
            "column" => true,
            _ => return Err(Args::wrong("mode", "range or column")),
        };
        if column && first != last {
            return Err(ToolError::InvalidArgs("column selection requires page and endPage to be the same".into()));
        }
        if first > last {
            std::mem::swap(&mut first, &mut last);
            std::mem::swap(&mut anchor, &mut focus);
        }
        if last - first >= MAX_PAGES {
            return Err(ToolError::InvalidArgs("text selection spans more than 64 pages; select fewer pages".into()));
        }
        let pages: Vec<_> = (first..=last).collect();
        let texts = self.page_texts(id, &pages)?;
        let mut budget = Budget::default();
        let mut combined = String::new();
        let mut output = Vec::new();
        for (page, text) in pages.into_iter().zip(texts) {
            let (selected, rects) = if column {
                let glyphs = text.glyphs_in([anchor[0], anchor[1], focus[0], focus[1]]);
                preflight(&text, glyphs.iter().copied(), &budget, !output.is_empty())?;
                (text.column_text(&glyphs), text.glyph_rects(&glyphs))
            } else {
                let mut start = if page == first { text.nearest(anchor[0], anchor[1]).unwrap_or(0) } else { 0 };
                let mut end =
                    if page == last { text.nearest(focus[0], focus[1]).map_or(0, |index| index.saturating_add(1)) } else { text.glyphs.len() };
                if first == last && start >= end && !text.is_empty() {
                    (start, end) = (end.saturating_sub(1), start.saturating_add(1));
                }
                preflight(&text, start..end, &budget, !output.is_empty())?;
                (text.text_of(start..end), text.line_rects(start..end))
            };
            budget.add(&selected, &rects, !output.is_empty())?;
            if !output.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&selected);
            output.push(json!({"page":page + 1,"text":selected,"rects":rects}));
        }
        Ok(json!({"text":combined,"pages":output}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_pdf_render::TextGlyph;
    use std::sync::Arc;

    fn cached_selection(text: PageText) -> Result<Vec<crate::Content>> {
        let mut automation = Automation::new();
        let bytes = automation.session().create_blank(240., 200., 1).unwrap();
        let id = automation.session_mut().open("SelectionBudget.pdf", None, bytes, None).unwrap();
        let bytes = automation.session().get(id).unwrap().bytes.clone();
        automation.texts.insert(id, (bytes, vec![Some(Arc::new(text))]));
        automation.call("text.select", &json!({"doc":id.0,"page":1,"anchor":[0,0],"focus":[0,1_000_000]}))
    }

    #[test]
    fn selection_tool_refuses_oversized_text_and_geometry_without_partial_json() {
        let mut glyph = TextGlyph { text: "x".repeat(MAX_TEXT_BYTES + 1), rect: [0., 0., 1., 1.], quad: [[0., 0.]; 4], direction: [1., 0.] };
        let large_text = PageText { glyphs: vec![glyph.clone()], line_of: vec![0], space_before: vec![false] };
        assert!(cached_selection(large_text).unwrap_err().to_string().contains("1 MiB"));
        glyph.text = "x".into();
        let mut many_lines = PageText::default();
        for line in 0..=MAX_RECTS {
            many_lines.glyphs.push(TextGlyph { text: "x".into(), rect: [0., line as f32, 1., line as f32 + 0.5], ..glyph.clone() });
            many_lines.line_of.push(line as u32);
            many_lines.space_before.push(false);
        }
        assert!(cached_selection(many_lines).unwrap_err().to_string().contains("16384"));
    }

    #[test]
    fn selection_text_budget_counts_utf8_and_page_separators() {
        let mut budget = Budget::default();
        budget.add(&"a".repeat(MAX_TEXT_BYTES - 1), &[], false).unwrap();
        budget.add("", &[], true).unwrap();
        assert!(budget.add("", &[], true).unwrap_err().to_string().contains("1 MiB"));
        assert!(Budget::default().add(&"a".repeat(MAX_TEXT_BYTES + 1), &[], false).is_err());
        assert_eq!(budget.bytes, MAX_TEXT_BYTES);
    }

    #[test]
    fn selection_preflight_counts_word_line_and_column_gaps_before_reconstruction() {
        let glyph = TextGlyph { text: "x".into(), rect: [0., 0., 1., 1.], quad: [[0., 0.]; 4], direction: [1., 0.] };
        let mut text = PageText { glyphs: vec![glyph.clone(), glyph.clone(), glyph], line_of: vec![0; 3], space_before: vec![false; 3] };
        text.glyphs[0].text = "x".repeat(MAX_TEXT_BYTES - 1);
        assert!(preflight(&text, 0..2, &Budget::default(), false).is_ok(), "exact limit without separator");
        text.space_before[1] = true;
        assert!(preflight(&text, 0..2, &Budget::default(), false).is_err(), "word gap counts");
        text.space_before[1] = false;
        text.line_of[1] = 1;
        assert!(preflight(&text, 0..2, &Budget::default(), false).is_err(), "line separator counts");
        assert!(preflight(&text, [0, 2].into_iter(), &Budget::default(), false).is_err(), "column skip gap counts");
        assert!(preflight(&text, 0..1, &Budget { bytes: 1, rects: 0 }, true).is_err(), "prior pages and separator count");
    }

    #[test]
    fn selection_rectangle_budget_and_nonfinite_geometry_are_explicit_errors() {
        let mut budget = Budget::default();
        budget.add("", &vec![[0., 0., 1., 1.]; MAX_RECTS], false).unwrap();
        assert!(budget.add("", &[[0., 0., 1., 1.]], false).unwrap_err().to_string().contains("16384"));
        assert!(Budget::default().add("", &[[0., 0., f32::INFINITY, 1.]], false).is_err());
        assert_eq!(budget.rects, MAX_RECTS);
    }
}
