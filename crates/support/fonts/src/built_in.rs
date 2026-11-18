//! Which faces of a kition-fonts checkout a build embeds. Shared by `build.rs`, which applies it,
//! and this crate's tests, which pin it (`regressions/issue-pdf-fonts-build-input`).

/// Whether a face for these manifest scripts (ISO 15924) is built in: it is for Japanese, Arabic or
/// Telugu, and not for Chinese. A checkout's Chinese, Latin and emoji faces are run-time fonts a
/// host adds (`add_document_font`, kition-sdk engine contract E9). Built in, a Latin face would
/// draw standard-font text as Type 3 outlines (Arimo stands in for Helvetica) and a Chinese face
/// would draw Chinese before any font is added. A face that also covers Chinese (Droid Sans
/// Fallback: `Hans,Hant,Jpan`) draws Han characters in their Chinese forms, so it is no Japanese face.
pub fn built_in(scripts: &[&str]) -> bool {
    scripts.iter().any(|s| matches!(*s, "Jpan" | "Arab" | "Telu")) && !scripts.iter().any(|s| matches!(*s, "Hans" | "Hant"))
}

#[cfg(test)]
mod tests {
    use super::built_in;

    /// The scripts of the public kition-fonts' Chinese, Latin and emoji faces: none is built in, so
    /// they never change the PDFs a build given that checkout writes.
    #[test]
    fn built_in_skips_the_public_kition_fonts_chinese_latin_and_emoji_faces() {
        for scripts in [
            &["Hans", "Hant", "Jpan"][..], // Droid Sans Fallback
            &["Latn", "Cyrl", "Grek"],     // Inter
            &["Latn", "Cyrl"],             // Lora, JetBrains Mono
            &["Latn"],                     // Bricolage Grotesque, Carlito, Caladea, Tinos, Cousine, Arimo
            &["Zsye"],                     // Noto Color Emoji
            &["Hans"],                     // Ma Shan Zheng
        ] {
            assert!(!built_in(scripts), "{scripts:?} is built in");
        }
    }

    #[test]
    fn built_in_keeps_japanese_arabic_and_telugu_faces() {
        for scripts in [&["Jpan", "Latn"][..], &["Jpan"], &["Arab"], &["Arab", "Latn"], &["Telu"]] {
            assert!(built_in(scripts), "{scripts:?} is not built in");
        }
        assert!(!built_in(&[]));
        assert!(!built_in(&["Hant", "Jpan"]));
    }
}
