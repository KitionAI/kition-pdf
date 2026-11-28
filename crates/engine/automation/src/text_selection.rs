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
        let anchor_page = page_index(first_number, "page")?;
        let head_page = page_index(args.opt_int("endPage")?.unwrap_or(first_number), "endPage")?;
        let anchor = End::given(args, "anchor", "anchorGlyph")?;
        let head = End::given(args, "focus", "focusGlyph")?;
        let column = match args.opt_str("mode")?.unwrap_or("range") {
            "range" => false,
            "column" => true,
            _ => return Err(Args::wrong("mode", "range or column")),
        };
        let unit = Unit::of(args, "unit", true)?.unwrap_or(Unit::Glyph);
        let step = args.opt_int("step")?.unwrap_or(0);
        let step_by = Unit::of(args, "stepBy", false)?.unwrap_or(Unit::Glyph);
        let answer_ends = args.opt_bool("ends")?.unwrap_or(false);
        let (first, last) = (anchor_page.min(head_page), anchor_page.max(head_page));
        if column {
            if anchor_page != head_page {
                return Err(ToolError::InvalidArgs("column selection requires page and endPage to be the same".into()));
            }
            if unit != Unit::Glyph || step != 0 || answer_ends || !matches!((&anchor, &head), (End::Point(_), End::Point(_))) {
                return Err(ToolError::InvalidArgs(
                    "unit, step, stepBy, ends, anchorGlyph and focusGlyph apply to range selection; a column takes anchor and focus points".into(),
                ));
            }
        }
        if last - first >= MAX_PAGES {
            return Err(ToolError::InvalidArgs("text selection spans more than 64 pages; select fewer pages".into()));
        }
        let pages: Vec<_> = (first..=last).collect();
        let texts = self.page_texts(id, &pages)?;
        let text_of = |page: usize| texts.get(page - first);
        if column {
            let (End::Point(from), End::Point(to)) = (anchor, head) else { return Ok(json!({"text":"","pages":[]})) };
            let Some(text) = text_of(first) else { return Err(failed("text selection could not read the page's text")) };
            let mut budget = Budget::default();
            let glyphs = text.glyphs_in([from[0], from[1], to[0], to[1]]);
            preflight(text, glyphs.iter().copied(), &budget, false)?;
            let (selected, rects) = (text.column_text(&glyphs), text.glyph_rects(&glyphs));
            budget.add(&selected, &rects, false)?;
            return Ok(json!({"text":selected,"pages":[{"page":first + 1,"text":selected,"rects":rects}]}));
        }
        let resolve = |page: usize, end: &End, key: &str| -> Result<Option<usize>> {
            let Some(text) = text_of(page) else { return Ok(None) };
            match end {
                End::Point([x, y]) => Ok(text.nearest(*x, *y)),
                End::Glyph(glyph) => usize::try_from(*glyph).ok().filter(|glyph| *glyph < text.glyphs.len()).map(Some).ok_or_else(|| {
                    ToolError::InvalidArgs(match text.glyphs.len() {
                        0 => format!("{key}: page {} has no text", page + 1),
                        len => format!("{key} must be between 0 and {} on page {}", len - 1, page + 1),
                    })
                }),
            }
        };
        let anchor_glyph = resolve(anchor_page, &anchor, "anchorGlyph")?;
        let head_glyph = match (resolve(head_page, &head, "focusGlyph")?, text_of(head_page)) {
            (Some(glyph), Some(text)) => Some(stepped(text, glyph, step, step_by)),
            (glyph, _) => glyph,
        };
        // Whole units outward from each end: the earlier end goes to its unit's start, the later to its end.
        let anchor_first = (anchor_page, anchor_glyph) <= (head_page, head_glyph);
        let widen = |page: usize, glyph: Option<usize>, to_start: bool| match (glyph, text_of(page)) {
            (Some(glyph), Some(text)) => Some(unit.widen(text, glyph, to_start)),
            (glyph, _) => glyph,
        };
        let anchor_glyph = widen(anchor_page, anchor_glyph, anchor_first);
        let head_glyph = widen(head_page, head_glyph, !anchor_first);
        let ((low_page, low), (high_page, high)) = if anchor_first {
            ((anchor_page, anchor_glyph), (head_page, head_glyph))
        } else {
            ((head_page, head_glyph), (anchor_page, anchor_glyph))
        };
        let mut budget = Budget::default();
        let mut combined = String::new();
        let mut output = Vec::new();
        for (page, text) in pages.into_iter().zip(texts.iter()) {
            let start = if page == low_page { low.unwrap_or(0) } else { 0 };
            let end = if page == high_page { high.map_or(0, |index| index.saturating_add(1)) } else { text.glyphs.len() };
            let end = end.max(start);
            preflight(text, start..end, &budget, !output.is_empty())?;
            let (selected, rects) = (text.text_of(start..end), text.line_rects(start..end));
            budget.add(&selected, &rects, !output.is_empty())?;
            if !output.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&selected);
            output.push(json!({"page":page + 1,"text":selected,"rects":rects}));
        }
        if !answer_ends {
            return Ok(json!({"text":combined,"pages":output}));
        }
        let end = |page: usize, glyph: Option<usize>| glyph.map_or(Value::Null, |glyph| json!({"page":page + 1,"glyph":glyph}));
        Ok(json!({"text":combined,"pages":output,"ends":{"anchor":end(anchor_page, anchor_glyph),"head":end(head_page, head_glyph)}}))
    }
}

/// One end of a selection as given: a page-local point (the nearest glyph), or a glyph's index.
enum End {
    Point([f32; 2]),
    Glyph(i64),
}

impl End {
    fn given(args: &Args, point_key: &str, glyph_key: &str) -> Result<End> {
        match (args.opt_int(glyph_key)?, args.get(point_key).is_some()) {
            (Some(_), true) => Err(ToolError::InvalidArgs(format!("give {point_key} or {glyph_key}, not both"))),
            (Some(glyph), false) => Ok(End::Glyph(glyph)),
            (None, _) => Ok(End::Point(point(args, point_key)?)),
        }
    }
}

/// What a selection's ends widen to (double, triple and quadruple clicks) and what a step moves by.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Unit {
    Glyph,
    Word,
    Line,
    Page,
}

impl Unit {
    fn of(args: &Args, key: &str, page: bool) -> Result<Option<Unit>> {
        let Some(name) = args.opt_str(key)? else { return Ok(None) };
        match name {
            "glyph" => Ok(Some(Unit::Glyph)),
            "word" => Ok(Some(Unit::Word)),
            "line" => Ok(Some(Unit::Line)),
            "page" if page => Ok(Some(Unit::Page)),
            _ => Err(Args::wrong(key, if page { "glyph, word, line or page" } else { "glyph, word or line" })),
        }
    }

    /// The first (`to_start`) or last glyph of the unit around `glyph`.
    fn widen(self, text: &PageText, glyph: usize, to_start: bool) -> usize {
        let around = match self {
            Unit::Glyph => None,
            Unit::Word => text.word_at(glyph),
            Unit::Line => text.line_at(glyph),
            Unit::Page => Some((0, text.glyphs.len().saturating_sub(1))),
        };
        around.map_or(glyph, |(first, last)| if to_start { first } else { last })
    }
}

/// The head moved `step` glyphs, words or lines on its page (negative: back), stopping at the page's first and last glyph.
fn stepped(text: &PageText, head: usize, step: i64, by: Unit) -> usize {
    let count = text.glyphs.len();
    if count == 0 || step == 0 {
        return head;
    }
    let forward = step > 0;
    let times = usize::try_from(step.unsigned_abs()).unwrap_or(usize::MAX);
    let last = count - 1;
    match by {
        Unit::Glyph | Unit::Page => {
            let moved = if forward { head.saturating_add(times) } else { head.saturating_sub(times) };
            moved.min(last)
        }
        Unit::Word | Unit::Line => {
            let mut at = head.min(last);
            // A step past the page's end changes nothing more, so `count` steps reach any glyph.
            for _ in 0..times.min(count) {
                let next = if by == Unit::Word { word_step(text, at, forward) } else { line_step(text, at, forward) };
                if next == at {
                    break;
                }
                at = next;
            }
            at
        }
    }
}

/// The last glyph of the next word after `head`, or the first of the word before it.
fn word_step(text: &PageText, head: usize, forward: bool) -> usize {
    if forward {
        (head.saturating_add(1)..text.glyphs.len()).find_map(|glyph| text.word_at(glyph).map(|(_, last)| last).filter(|last| *last > head))
    } else {
        (0..head).rev().find_map(|glyph| text.word_at(glyph).map(|(first, _)| first).filter(|first| *first < head))
    }
    .unwrap_or(head)
}

/// The glyph as far into the next line (or the one before) as `head` is into its own.
fn line_step(text: &PageText, head: usize, forward: bool) -> usize {
    let Some((first, last)) = text.line_at(head) else { return head };
    let other =
        if forward { last.checked_add(1).and_then(|next| text.line_at(next)) } else { first.checked_sub(1).and_then(|before| text.line_at(before)) };
    other.map_or(head, |(start, end)| start.saturating_add(head - first).min(end))
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
