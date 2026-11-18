//! Fonts from the optional kition-fonts build input (<https://github.com/KitionAI/kition-fonts>).
//!
//! `build.rs` embeds the Japanese, Arabic and Telugu faces of a kition-fonts checkout when Kition
//! PDF is built with `KITION_FONTS_DIR=<checkout>`; otherwise [`KITION_FONTS`] is empty and
//! everything here returns nothing. Callers must work either way. From the public kition-fonts a
//! build embeds BIZ UDPGothic, BIZ UDMincho, Shippori Mincho and Noto Sans Arabic (its Chinese,
//! Latin and emoji faces are run-time fonts a host adds); [`check_test_faces`] makes the tests
//! that need a face a checkout lacks say so.

/// A font from the optional kition-fonts build input (empty unless built with `KITION_FONTS_DIR`).
pub struct KitionFont {
    pub family: &'static str,
    pub style: &'static str,
    /// ISO 15924 scripts the font is for, e.g. `"Jpan"`.
    pub scripts: &'static [&'static str],
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/kition_fonts.rs"));

impl KitionFont {
    /// Whether the font is meant for `script` (ISO 15924, e.g. `"Jpan"`).
    pub fn covers(&self, script: &str) -> bool {
        self.scripts.contains(&script)
    }

    /// A unique name for registering the face with a font system ("BIZ UDPGothic Bold").
    pub fn name(&self) -> String {
        format!("{} {}", self.family, self.style)
    }
}

/// The `Jpan` kition-fonts faces in the order the interface prefers them: BIZ UDPGothic (a UI
/// face) first, then the rest in manifest order. Empty without kition-fonts.
pub fn ui_japanese_fonts() -> Vec<&'static KitionFont> {
    let mut fonts: Vec<&KitionFont> = KITION_FONTS.iter().filter(|f| f.covers("Jpan")).collect();
    // Stable: manifest order within each group.
    fonts.sort_by_key(|f| f.family != "BIZ UDPGothic");
    fonts
}

/// The `Hans` kition-fonts faces for Simplified Chinese interface text, in manifest order.
/// Empty when built without kition-fonts (Chinese text then shows the font system's
/// replacement glyph, the same degraded mode as Japanese without kition-fonts).
pub fn ui_chinese_fonts() -> Vec<&'static KitionFont> {
    KITION_FONTS.iter().filter(|f| f.covers("Hans")).collect()
}

/// The `Arab` kition-fonts faces for Arabic-script interface text (file names, document titles),
/// in manifest order. Empty when built without kition-fonts or when it has no Arabic face.
pub fn ui_arabic_fonts() -> Vec<&'static KitionFont> {
    arabic(KITION_FONTS.iter())
}

/// The face for Arabic text written into PDFs: the first `Arab` face. `None` when built without
/// kition-fonts or when its revision has no Arabic face.
pub fn document_arabic_font() -> Option<&'static KitionFont> {
    ui_arabic_fonts().into_iter().next()
}

fn arabic<'a>(faces: impl IntoIterator<Item = &'a KitionFont>) -> Vec<&'a KitionFont> {
    faces.into_iter().filter(|f| f.covers("Arab")).collect()
}

/// The `Telu` kition-fonts faces for Telugu interface text (the Telugu catalog, file names,
/// document titles), in manifest order. Empty when built without kition-fonts or when it has no
/// Telugu face.
pub fn ui_telugu_fonts() -> Vec<&'static KitionFont> {
    KITION_FONTS.iter().filter(|f| f.covers("Telu")).collect()
}

/// Interface CJK faces in fallback order for the UI language: Simplified Chinese first when
/// `prefer_hans`, otherwise Japanese first (the historical default).
///
/// The order matters beyond glyph shapes. egui renders each character with the FIRST face that
/// has its glyph, so with Japanese first a Simplified-only character (e.g. U+6B22 欢, absent
/// from Japanese faces) lands in a different face than its neighbours; the mixed vertical
/// metrics then sink it below the line. Preferring the UI language's face keeps one line in
/// one face with one baseline.
pub fn ui_cjk_fonts(prefer_hans: bool) -> Vec<&'static KitionFont> {
    order_cjk(KITION_FONTS.iter(), prefer_hans)
}

fn order_cjk<'a>(faces: impl IntoIterator<Item = &'a KitionFont>, prefer_hans: bool) -> Vec<&'a KitionFont> {
    let mut out: Vec<&'a KitionFont> = faces.into_iter().filter(|f| f.covers("Hans") || f.covers("Jpan")).collect();
    let first = if prefer_hans { "Hans" } else { "Jpan" };
    // The preferred script's faces first (a face covering both counts as preferred); within each
    // group the Japanese UI face (BIZ UDPGothic, not a serif) leads, otherwise manifest order
    // (the sort is stable).
    out.sort_by_key(|f| (!f.covers(first), f.family != "BIZ UDPGothic"));
    out
}

/// The face for Japanese text written into PDFs (serif document text): Shippori Mincho, then
/// BIZ UDMincho, then any other regular `Jpan` face. `None` without kition-fonts.
pub fn document_japanese_font() -> Option<&'static KitionFont> {
    document_face(KITION_FONTS, true, false)
}

/// A real Japanese document face matching serif/sans and weight where available.
/// Sans text prefers BIZ UDPGothic Bold or Regular. Missing weights fall back to Regular;
/// serif text keeps the document Mincho preference. No synthetic weight or slant is applied.
/// `None` without kition-fonts. The small web input currently has only Gothic Regular.
pub fn document_japanese_font_for_style(serif: bool, bold: bool) -> Option<&'static KitionFont> {
    document_face(KITION_FONTS, serif, bold)
}

/// Every `Jpan` face that can stand in for the document text, the best match for the style first
/// (the face [`document_japanese_font_for_style`] returns), then the others. Not every face has
/// every glyph (their coverage of e.g. Cyrillic differs), so a caller can move on to the next one.
/// Empty without kition-fonts.
pub fn document_japanese_fonts_for_style(serif: bool, bold: bool) -> Vec<&'static KitionFont> {
    document_faces(KITION_FONTS, serif, bold)
}

fn document_face(faces: &[KitionFont], serif: bool, bold: bool) -> Option<&KitionFont> {
    document_faces(faces, serif, bold).into_iter().next()
}

fn document_faces(faces: &[KitionFont], serif: bool, bold: bool) -> Vec<&KitionFont> {
    let jpan = || faces.iter().filter(|f| f.covers("Jpan"));
    let mut preferred = Vec::new();
    if !serif {
        let style = if bold { "Bold" } else { "Regular" };
        preferred.extend(jpan().find(|f| f.family == "BIZ UDPGothic" && f.style == style));
        preferred.extend(jpan().find(|f| f.family == "BIZ UDPGothic" && f.style == "Regular"));
    }
    for family in ["Shippori Mincho", "BIZ UDMincho"] {
        preferred.extend(jpan().find(|f| f.family == family && f.style == "Regular"));
    }
    let mut out: Vec<&KitionFont> = Vec::new();
    for face in preferred.into_iter().chain(jpan().filter(|f| f.style == "Regular")).chain(jpan()) {
        if !out.iter().any(|f| std::ptr::eq(*f, face)) {
            out.push(face);
        }
    }
    out
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn find(family: &str, style: &str) -> Option<&'static [u8]> {
    let mut i = 0;
    while i < KITION_FONTS.len() {
        let f = &KITION_FONTS[i];
        if str_eq(f.family, family) && str_eq(f.style, style) {
            return Some(f.bytes);
        }
        i += 1;
    }
    None
}

/// Shippori Mincho Regular from kition-fonts, the preferred face for Japanese document text.
/// `None` when Kition PDF was built without kition-fonts.
pub static SHIPPORI_MINCHO: Option<&[u8]> = find("Shippori Mincho", "Regular");

/// What a test of the built-in faces needs from the kition-fonts build input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestFace {
    /// The face of this family and style.
    Face(&'static str, &'static str),
    /// Any face for this script (ISO 15924: `"Jpan"`, `"Arab"`).
    Script(&'static str),
}

/// The faces the tests of the built-in faces are written for, so a failure can say where to get
/// one: (family, style, script, licence, upstream).
const TEST_FACE_SOURCES: [(&str, &str, &str, &str, &str); 4] = [
    ("Shippori Mincho", "Regular", "Jpan", "OFL-1.1", "https://github.com/fontdasu/ShipporiMincho"),
    ("BIZ UDPGothic", "Regular", "Jpan", "OFL-1.1", "https://github.com/googlefonts/morisawa-biz-ud-gothic"),
    ("BIZ UDMincho", "Regular", "Jpan", "OFL-1.1", "https://github.com/googlefonts/morisawa-biz-ud-mincho"),
    ("Noto Sans Arabic", "Regular", "Arab", "OFL-1.1", "https://github.com/notofonts/arabic"),
];

/// For a test that needs built-in faces: `Ok` when this build has every face in `needs`, or when
/// it was built without `KITION_FONTS_DIR` (it has no built-in face by design, and the test
/// checks that build instead). `Err` when it was built with `KITION_FONTS_DIR` and lacks some: the
/// message names each and where to get it, and the test fails with it. Fonts are public, so a
/// missing one is a failure, never a skip.
pub fn check_test_faces(needs: &[TestFace]) -> Result<(), String> {
    check_faces(KITION_FONTS, KITION_FONTS_DIR_SET, needs)
}

fn check_faces(faces: &[KitionFont], dir_set: bool, needs: &[TestFace]) -> Result<(), String> {
    if !dir_set {
        return Ok(());
    }
    let has = |need: &TestFace| match *need {
        TestFace::Face(family, style) => faces.iter().any(|f| f.family == family && f.style == style),
        TestFace::Script(script) => faces.iter().any(|f| f.covers(script)),
    };
    let source = |(family, style, _, licence, upstream): &(&str, &str, &str, &str, &str)| format!("{family} {style} ({licence}, {upstream})");
    let missing: Vec<String> = needs
        .iter()
        .filter(|need| !has(need))
        .map(|need| match *need {
            TestFace::Face(family, style) => match TEST_FACE_SOURCES.iter().find(|s| s.0 == family && s.1 == style) {
                Some(s) => source(s),
                None => format!("{family} {style}"),
            },
            TestFace::Script(script) => {
                let known: Vec<String> = TEST_FACE_SOURCES.iter().filter(|s| s.2 == script).map(source).collect();
                if known.is_empty() {
                    format!("a face for the {script} script")
                } else {
                    format!("a face for the {script} script, such as {}", known.join(" or "))
                }
            }
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let (it, face) = if missing.len() == 1 { ("it", "the face") } else { ("them", "each face") };
    Err(format!(
        "this test needs {}: this build was given a kition-fonts checkout (KITION_FONTS_DIR) that lacks {it}. Add {face} \
         to that checkout (the file, its licence and a fonts/manifest.txt line), or build without KITION_FONTS_DIR to test \
         the build that has no built-in faces",
        missing.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_fallback_order_follows_the_ui_language() {
        // Synthetic faces: order_cjk must not depend on the real build input.
        static BYTES: &[u8] = b"fake";
        static LATN: &[&str] = &["Latn"];
        static JPAN_LATN: &[&str] = &["Jpan", "Latn"];
        static HANS_LATN: &[&str] = &["Hans", "Latn"];
        let biz_bold = KitionFont { family: "BIZ UDPGothic", style: "Bold", scripts: JPAN_LATN, bytes: BYTES };
        let biz = KitionFont { family: "BIZ UDPGothic", style: "Regular", scripts: JPAN_LATN, bytes: BYTES };
        let hans = KitionFont { family: "FakeHans", style: "Regular", scripts: HANS_LATN, bytes: BYTES };
        let inter = KitionFont { family: "Inter", style: "Regular", scripts: LATN, bytes: BYTES };
        let faces = [biz_bold, biz, hans, inter];
        // Chinese mode: the Hans group first (manifest order), then Japanese.
        let zh: Vec<&str> = order_cjk([&faces[0], &faces[2], &faces[1]], true).iter().map(|f| f.family).collect();
        assert_eq!(zh, ["FakeHans", "BIZ UDPGothic", "BIZ UDPGothic"]);
        // Japanese mode keeps the historical default: the Japanese UI face first.
        let ja: Vec<&str> = order_cjk([&faces[0], &faces[2], &faces[1]], false).iter().map(|f| f.family).collect();
        assert_eq!(ja, ["BIZ UDPGothic", "BIZ UDPGothic", "FakeHans"]);
        // Latin-only faces are never interface CJK fallbacks.
        assert!(order_cjk(&faces, true).iter().all(|f| f.covers("Hans") || f.covers("Jpan")));
        // A serif listed first in the manifest never leads the Japanese group, in either mode.
        let mincho = KitionFont { family: "Shippori Mincho", style: "Regular", scripts: JPAN_LATN, bytes: BYTES };
        let zh: Vec<&str> = order_cjk([&mincho, &faces[2], &faces[1]], true).iter().map(|f| f.family).collect();
        assert_eq!(zh, ["FakeHans", "BIZ UDPGothic", "Shippori Mincho"]);
        let ja: Vec<&str> = order_cjk([&mincho, &faces[2], &faces[1]], false).iter().map(|f| f.family).collect();
        assert_eq!(ja, ["BIZ UDPGothic", "Shippori Mincho", "FakeHans"]);
    }

    #[test]
    fn document_faces_match_style_without_inventing_missing_weights() {
        let faces = [
            KitionFont { family: "Shippori Mincho", style: "Regular", scripts: &["Jpan"], bytes: b"serif" },
            KitionFont { family: "BIZ UDPGothic", style: "Regular", scripts: &["Jpan"], bytes: b"sans" },
            KitionFont { family: "BIZ UDPGothic", style: "Bold", scripts: &["Jpan"], bytes: b"bold" },
            KitionFont { family: "BIZ UDPGothic", style: "Bold", scripts: &["Latn"], bytes: b"not-japanese" },
        ];
        assert_eq!(document_face(&faces, false, false).unwrap().bytes, b"sans");
        assert_eq!(document_face(&faces, false, true).unwrap().bytes, b"bold");
        assert_eq!(document_face(&faces, true, false).unwrap().bytes, b"serif");
        assert_eq!(document_face(&faces, true, true).unwrap().bytes, b"serif");
        assert_eq!(document_face(&faces[..2], false, true).unwrap().bytes, b"sans");
        assert_eq!(document_face(&faces[..1], false, true).unwrap().bytes, b"serif");
        // Every Japanese face is a candidate, the style's match first and none twice.
        let order: Vec<&[u8]> = document_faces(&faces, false, true).iter().map(|f| f.bytes).collect();
        assert_eq!(order, [b"bold".as_slice(), b"sans", b"serif"]);
        let order: Vec<&[u8]> = document_faces(&faces, true, false).iter().map(|f| f.bytes).collect();
        assert_eq!(order, [b"serif".as_slice(), b"sans", b"bold"]);
        assert!(document_face(&faces[3..], false, true).is_none());
        assert!(document_face(&[], false, true).is_none());
    }

    #[test]
    fn arabic_faces_are_picked_by_script_in_manifest_order() {
        // Synthetic faces: the filter must not depend on the real build input.
        static BYTES: &[u8] = b"fake";
        static ARAB_LATN: &[&str] = &["Arab", "Latn"];
        static JPAN: &[&str] = &["Jpan"];
        let naskh = KitionFont { family: "FakeNaskh", style: "Regular", scripts: ARAB_LATN, bytes: BYTES };
        let biz = KitionFont { family: "BIZ UDPGothic", style: "Regular", scripts: JPAN, bytes: BYTES };
        let kufi = KitionFont { family: "FakeKufi", style: "Regular", scripts: ARAB_LATN, bytes: BYTES };
        let faces = [naskh, biz, kufi];
        let ar: Vec<&str> = arabic(&faces).iter().map(|f| f.family).collect();
        assert_eq!(ar, ["FakeNaskh", "FakeKufi"]);
        // An Arabic face is never a CJK fallback, and the other way round.
        assert!(order_cjk(&faces, false).iter().all(|f| f.family == "BIZ UDPGothic"));
        assert_eq!(ui_arabic_fonts().len(), KITION_FONTS.iter().filter(|f| f.covers("Arab")).count());
    }

    #[test]
    fn test_faces_fail_by_name_only_when_the_build_was_given_kition_fonts() {
        static BYTES: &[u8] = b"fake";
        let faces = [
            KitionFont { family: "BIZ UDPGothic", style: "Regular", scripts: &["Jpan", "Latn"], bytes: BYTES },
            KitionFont { family: "FakeNaskh", style: "Regular", scripts: &["Arab"], bytes: BYTES },
        ];
        let needs = [TestFace::Face("Shippori Mincho", "Regular"), TestFace::Script("Jpan"), TestFace::Script("Arab")];
        // Built without KITION_FONTS_DIR: no face is expected.
        assert_eq!(check_faces(&[], false, &needs), Ok(()));
        // Built with it: the missing face is named, with its licence and upstream.
        let err = check_faces(&faces, true, &needs).unwrap_err();
        assert!(err.contains("Shippori Mincho Regular (OFL-1.1, https://github.com/fontdasu/ShipporiMincho)"), "{err}");
        assert!(!err.contains("Jpan script") && !err.contains("Arab script"), "{err}");
        assert!(err.contains("KITION_FONTS_DIR"), "{err}");
        // A missing script lists the faces the tests are written for.
        let err = check_faces(&[], true, &[TestFace::Script("Arab")]).unwrap_err();
        assert!(err.contains("a face for the Arab script, such as Noto Sans Arabic Regular (OFL-1.1, https://github.com/notofonts/arabic)"), "{err}");
        assert_eq!(check_faces(&faces, true, &needs[1..]), Ok(()));
    }

    /// Whatever the kition-fonts checkout holds, only its Japanese, Arabic and Telugu faces are
    /// built in (`regressions/issue-pdf-fonts-build-input`).
    #[test]
    fn built_in_faces_are_japanese_arabic_or_telugu_and_never_chinese() {
        for face in KITION_FONTS {
            assert!(crate::built_in::built_in(face.scripts), "{} ({:?}) is built in", face.name(), face.scripts);
        }
    }

    /// Family-name prefixes `build.rs` never embeds (its own `BARRED_FAMILIES`): Adobe's type
    /// designs, which includes Noto CJK (Source Han under another name).
    const BARRED_FAMILIES: [&str; 5] = ["Source Han", "Source Serif", "Source Sans", "Noto Sans CJK", "Noto Serif CJK"];

    /// Whatever the kition-fonts checkout holds, no Adobe type design is embedded (AGENTS.md §1.1).
    #[test]
    fn barred_families_are_never_embedded() {
        for face in KITION_FONTS {
            assert!(!BARRED_FAMILIES.iter().any(|barred| face.family.starts_with(barred)), "{} is embedded", face.name());
        }
        assert!(BARRED_FAMILIES.iter().any(|barred| "Noto Sans CJK SC".starts_with(barred)));
        assert!(!BARRED_FAMILIES.iter().any(|barred| "Noto Sans Arabic".starts_with(barred)));
    }

    #[test]
    fn kition_fonts_are_optional_and_consistent() {
        check_test_faces(&[TestFace::Face("Shippori Mincho", "Regular")]).unwrap_or_else(|e| panic!("{e}"));
        // Holds with or without KITION_FONTS_DIR.
        assert_eq!(SHIPPORI_MINCHO.is_some(), KITION_FONTS.iter().any(|f| f.family == "Shippori Mincho" && f.style == "Regular"));
        assert_eq!(document_japanese_font().is_some(), KITION_FONTS.iter().any(|f| f.covers("Jpan")));
        let ui = ui_japanese_fonts();
        assert_eq!(ui.len(), KITION_FONTS.iter().filter(|f| f.covers("Jpan")).count());
        let ui_zh = ui_chinese_fonts();
        assert_eq!(ui_zh.len(), KITION_FONTS.iter().filter(|f| f.covers("Hans")).count());
        if KITION_FONTS.is_empty() {
            eprintln!("built without kition-fonts (KITION_FONTS_DIR unset): no Japanese faces, as expected");
            assert!(SHIPPORI_MINCHO.is_none() && ui.is_empty() && ui_zh.is_empty());
            return;
        }
        assert!(KITION_FONTS.iter().all(|f| !f.bytes.is_empty()));
        if KITION_FONTS.iter().any(|f| f.family == "BIZ UDPGothic") {
            assert_eq!(ui.first().map(|f| f.family), Some("BIZ UDPGothic"));
        }
        assert_eq!(document_japanese_font().map(|f| f.family), Some("Shippori Mincho"));
    }
}
