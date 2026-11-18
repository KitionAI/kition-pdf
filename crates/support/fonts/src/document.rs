//! The faces text written into PDFs can be drawn from: the kition-fonts faces built in
//! ([`crate::KITION_FONTS`]) and the faces a host adds at run time ([`add_document_font`], Kition
//! platform E9), as a web build has none built in. Text drawn from a face is written as Type 3
//! glyphs from its outlines, so the page carries every shape it shows.

use std::sync::RwLock;

use skrifa::instance::{Location, LocationRef, Size};
use skrifa::string::StringId;
use skrifa::{FontRef, MetadataProvider};

use crate::script::bounded_outline_at;
use crate::{GlyphError, GlyphOutline, KITION_FONTS, KitionFont};

/// The faces added at run time, in the order added. Each lives as long as the process (a host
/// adds a handful once, before documents open).
static ADDED: RwLock<Vec<&'static KitionFont>> = RwLock::new(Vec::new());

/// A face to draw with, and the weight to set a variable face at (`None`: its default).
#[derive(Clone, Copy)]
pub struct DocumentFace {
    pub face: &'static KitionFont,
    pub weight: Option<f32>,
}

impl std::fmt::Debug for DocumentFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentFace").field("family", &self.face.family).field("style", &self.face.style).field("weight", &self.weight).finish()
    }
}

impl PartialEq for DocumentFace {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.face, other.face) && self.weight.map(f32::to_bits) == other.weight.map(f32::to_bits)
    }
}

impl DocumentFace {
    /// Whether the face has a glyph for `c`.
    pub fn covers(&self, c: char) -> bool {
        FontRef::new(self.face.bytes).is_ok_and(|font| font.charmap().map(c).is_some())
    }

    fn location(&self, font: &FontRef<'_>) -> Location {
        match self.weight {
            Some(weight) => font.axes().location([("wght", weight)]),
            None => Location::default(),
        }
    }

    /// The advance of `c` in em, if the face has it.
    pub fn advance(&self, c: char) -> Option<f64> {
        let font = FontRef::new(self.face.bytes).ok()?;
        let gid = font.charmap().map(c)?;
        let location = self.location(&font);
        let upem = f64::from(font.metrics(Size::unscaled(), LocationRef::from(&location)).units_per_em.max(1));
        let advance = font.glyph_metrics(Size::unscaled(), LocationRef::from(&location)).advance_width(gid)?;
        Some(f64::from(advance) / upem).filter(|w| w.is_finite() && *w >= 0.0)
    }

    /// The outline of `c` in em (y up), with its advance.
    pub fn glyph(&self, c: char) -> Result<GlyphOutline, GlyphError> {
        let font = FontRef::new(self.face.bytes).map_err(|_| GlyphError::Missing)?;
        let gid = font.charmap().map(c).ok_or(GlyphError::Missing)?;
        let width = self.advance(c).ok_or(GlyphError::Missing)?;
        let location = self.location(&font);
        bounded_outline_at(&font, gid, width, LocationRef::from(&location))
    }

    /// A PostScript-style name for the face: its family and style without spaces.
    pub fn font_name(&self) -> String {
        let style = match self.weight {
            Some(weight) if weight >= 600.0 => "Bold",
            _ => self.face.style,
        };
        format!("{}-{}", self.face.family, style).replace(' ', "")
    }
}

/// Every document face: the built-in kition-fonts faces, then the added ones.
pub fn document_fonts() -> Vec<&'static KitionFont> {
    let mut faces: Vec<&'static KitionFont> = KITION_FONTS.iter().collect();
    faces.extend(ADDED.read().unwrap_or_else(std::sync::PoisonError::into_inner).iter().copied());
    faces
}

fn names(font: &FontRef<'_>) -> (String, String) {
    let name = |ids: &[StringId]| ids.iter().find_map(|id| font.localized_strings(*id).english_or_first().map(|s| s.to_string()));
    let family = name(&[StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]).unwrap_or_default();
    let style = name(&[StringId::TYPOGRAPHIC_SUBFAMILY_NAME, StringId::SUBFAMILY_NAME]).unwrap_or_else(|| "Regular".into());
    (family, style)
}

/// The scripts (ISO 15924) a face covers, from one telling character of each.
fn scripts(font: &FontRef<'_>) -> Vec<&'static str> {
    let has = |c: char| font.charmap().map(c).is_some();
    let mut out = Vec::new();
    for (script, probe) in
        [("Latn", 'A'), ("Cyrl", 'Ж'), ("Grek", 'Ω'), ("Arab", 'ب'), ("Hans", '们'), ("Hant", '們'), ("Jpan", 'あ'), ("Kore", '한')]
    {
        if has(probe) {
            out.push(script);
        }
    }
    out
}

/// Adds a font file (TrueType or OpenType) as a document face; answers how many faces it added:
/// 0 for bytes that are no font, one without a family name, or a face already there (by family and
/// style).
pub fn add_document_font(bytes: Vec<u8>) -> usize {
    let (family, style, scripts) = {
        let Ok(font) = FontRef::new(&bytes) else { return 0 };
        let (family, style) = names(&font);
        (family, style, scripts(&font))
    };
    if family.is_empty() {
        return 0;
    }
    let mut added = ADDED.write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let known = |f: &KitionFont| f.family.eq_ignore_ascii_case(&family) && f.style.eq_ignore_ascii_case(&style);
    if KITION_FONTS.iter().any(known) || added.iter().any(|f| known(f)) {
        return 0;
    }
    let face = KitionFont {
        family: Box::leak(family.into_boxed_str()),
        style: Box::leak(style.into_boxed_str()),
        scripts: Box::leak(scripts.into_boxed_slice()),
        bytes: Box::leak(bytes.into_boxed_slice()),
    };
    added.push(Box::leak(Box::new(face)));
    1
}

fn is_variable(face: &KitionFont) -> bool {
    FontRef::new(face.bytes).is_ok_and(|font| font.axes().iter().any(|axis| axis.tag() == skrifa::Tag::new(b"wght")))
}

/// The document face of `family` (case ignored) for the style: the face of that style, else a
/// variable face set at weight 700 for bold, else the family's regular face. `None` when no
/// document face has the family.
pub fn document_font(family: &str, bold: bool, italic: bool) -> Option<DocumentFace> {
    let faces: Vec<&'static KitionFont> = document_fonts().into_iter().filter(|f| f.family.eq_ignore_ascii_case(family)).collect();
    let styled = |name: &str| faces.iter().copied().find(|f| f.style.eq_ignore_ascii_case(name));
    let wanted = match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    };
    if let Some(face) = styled(wanted) {
        return Some(DocumentFace { face, weight: None });
    }
    let base = if italic { styled("Italic").or_else(|| styled("Regular")) } else { styled("Regular") }.or_else(|| faces.first().copied())?;
    let weight = (bold && is_variable(base)).then_some(700.0);
    Some(DocumentFace { face: base, weight })
}

/// The face that stands in for a standard-14 family (`Helvetica`, `Times` or `Courier`) when the
/// host added it: Arimo, Tinos and Cousine have their metrics.
pub fn metric_face(standard: &str, bold: bool, italic: bool) -> Option<DocumentFace> {
    let family = match standard {
        "Helvetica" => "Arimo",
        "Times" => "Tinos",
        "Courier" => "Cousine",
        _ => return None,
    };
    document_font(family, bold, italic)
}

/// East Asian wide characters (the CJK faces' own).
fn wide(c: char) -> bool {
    matches!(u32::from(c), 0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

/// Kana: hiragana, katakana (full and half width) and their extensions, without the marks Chinese
/// text uses too (the middle dot ・ and the prolonged sound mark ー).
fn is_kana(c: char) -> bool {
    matches!(u32::from(c), 0x3041..=0x3096 | 0x309D..=0x309F | 0x30A1..=0x30FA | 0x30FD..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF6F | 0xFF71..=0xFF9D | 0x1B000..=0x1B16F)
}

/// Whether `text` is Japanese: it has kana. Han characters alone don't tell Japanese from Chinese.
pub fn is_japanese(text: &str) -> bool {
    text.chars().any(is_kana)
}

/// A document face with a glyph for `c` in text that [`is_japanese`] or not: for a wide character
/// a Chinese, Japanese or Korean face first, then any face, in [`document_fonts`] order. A
/// built-in Japanese face draws only Japanese text: other text takes its Han characters from the
/// faces the host adds (its Chinese font), so Chinese is never drawn in Japanese glyph forms.
pub fn face_for(c: char, japanese: bool) -> Option<DocumentFace> {
    let added = ADDED.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    candidates(KITION_FONTS, &added, wide(c), japanese).into_iter().map(|face| DocumentFace { face, weight: None }).find(|face| face.covers(c))
}

/// The faces [`face_for`] tries, in order: the built-in faces (without the Japanese ones unless the
/// text is `japanese`), then the `added` ones; for a `wide` character the CJK faces first.
fn candidates<'a>(built_in: &'a [KitionFont], added: &[&'a KitionFont], wide: bool, japanese: bool) -> Vec<&'a KitionFont> {
    let faces = built_in.iter().filter(|f| japanese || !f.covers("Jpan")).chain(added.iter().copied());
    let cjk = |f: &&KitionFont| ["Hans", "Hant", "Jpan", "Kore"].iter().any(|s| f.covers(s));
    if wide { faces.clone().filter(cjk).chain(faces.filter(|f| !cjk(f))).collect() } else { faces.collect() }
}

/// Whether text may break before or after `c` without a space (each wide character is a word).
pub fn breaks_around(c: char) -> bool {
    wide(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_not_a_font_add_nothing() {
        assert_eq!(add_document_font(b"not a font".to_vec()), 0);
    }

    /// A built-in Japanese face draws only Japanese text (text with kana): Han characters of other
    /// text are left to the faces a host adds, so Chinese is never drawn in Japanese glyph forms
    /// (`regressions/issue-japanese-faces-for-japanese-text`).
    #[test]
    fn built_in_japanese_faces_are_candidates_only_for_japanese_text() {
        static BYTES: &[u8] = b"fake";
        let built_in = [
            KitionFont { family: "BIZ UDPGothic", style: "Regular", scripts: &["Jpan"], bytes: BYTES },
            KitionFont { family: "FakeNaskh", style: "Regular", scripts: &["Arab"], bytes: BYTES },
        ];
        let droid = KitionFont { family: "Droid Sans Fallback", style: "Regular", scripts: &["Latn", "Hans", "Hant", "Jpan"], bytes: BYTES };
        let host_japanese = KitionFont { family: "FakeMincho", style: "Regular", scripts: &["Jpan"], bytes: BYTES };
        let added = [&droid, &host_japanese];
        let families = |wide: bool, japanese: bool| -> Vec<&str> { candidates(&built_in, &added, wide, japanese).iter().map(|f| f.family).collect() };
        // Japanese text: the built-in Japanese face leads the CJK faces.
        assert_eq!(families(true, true), ["BIZ UDPGothic", "Droid Sans Fallback", "FakeMincho", "FakeNaskh"]);
        assert_eq!(families(false, true), ["BIZ UDPGothic", "FakeNaskh", "Droid Sans Fallback", "FakeMincho"]);
        // Other text: only the faces the host added, and the built-in faces of other scripts.
        assert_eq!(families(true, false), ["Droid Sans Fallback", "FakeMincho", "FakeNaskh"]);
        assert_eq!(families(false, false), ["FakeNaskh", "Droid Sans Fallback", "FakeMincho"]);

        for text in ["日本語のテキスト", "カタカナ", "ｶﾀｶﾅ", "見本の中"] {
            assert!(is_japanese(text), "{text:?}");
        }
        // Han characters alone don't tell Japanese from Chinese, nor do the marks both use.
        for text in ["Café 中", "見本商会　御中", "中文・ー", "Привет", ""] {
            assert!(!is_japanese(text), "{text:?}");
        }
    }

    #[test]
    fn wide_characters_break_anywhere() {
        assert!(breaks_around('中') && breaks_around('あ') && !breaks_around('a') && !breaks_around('é'));
    }
}
