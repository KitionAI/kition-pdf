# kition-pdf-fonts

Layer L2. Font helpers for the appearance streams Kition PDF generates (comments, form fields).

Today it holds what generated appearances need without a font program:

- `helvetica_width`: an approximation of Helvetica's proportions by character class. No metrics
  file or font program from any vendor is bundled; widths are Kition PDF's own estimates, good
  enough for line breaking and alignment, not for typesetting.
- `wrap`: greedy line breaking with that measure (paragraphs on newlines, long words split).
- `win_ansi`: Unicode → WinAnsiEncoding bytes (`?` for characters it can't represent), and
  `literal` to write bytes as a PDF literal string.
- `type3_font`: a Type 3 font drawn from a document face's glyph outlines, with a ToUnicode map,
  holding only the characters given (at most `TYPE3_GLYPHS`): how added page text and form-field
  appearances draw what the standard fonts can't (Chinese in a Helvetica field, say).

It also holds the built-in faces from the optional [kition-fonts](https://github.com/KitionAI/kition-fonts)
build input. When the build sets `KITION_FONTS_DIR=<checkout>`, `build.rs` embeds the checkout's
Japanese, Arabic and Telugu faces (manifest scripts `Jpan`, `Arab` or `Telu`, and not `Hans` or
`Hant`) as `KITION_FONTS`, and fails if it can't read the checkout while `KITION_FONTS_REQUIRED=1`.
It leaves the checkout's Chinese, Latin and emoji faces to the host, which adds them at run time
(`add_document_font`, kition-sdk engine contract E9), as Kition's web editor does: built in, Arimo
would draw every Helvetica text item as Type 3 outlines, and a Chinese face would draw Chinese
before any font is added, so the build would write different PDFs from one without the checkout.
A face that covers Chinese (Droid Sans Fallback) draws Han characters in their Chinese forms, so it
is not used as a Japanese face either. The other way round, a built-in Japanese face draws only
Japanese text: `face_for(c, japanese)`, the face page text falls back to, offers it only for text
that `is_japanese` (has kana), so "Café 中" takes 中 from a font the host added, or is refused.

The public kition-fonts holds BIZ UDPGothic, BIZ UDMincho and Shippori Mincho (Japanese) and Noto
Sans Arabic, all OFL-1.1, so a build with it embeds those four. Shippori Mincho's kanji come from
Genryu Mincho, a derivative of Source Han Serif: it is the one exception to AGENTS.md §1.1.
`KITION_FONTS_DIR_SET` records whether the variable was set; `check_test_faces` uses it so that the
tests of the built-in faces skip in a build without the variable and, in a build with it, fail and
name each face they need that the checkout lacks.

Without the variable `KITION_FONTS` is empty and everything below copes:

- `ui_japanese_fonts`: the `Jpan` faces for the interface, BIZ UDPGothic first.
- `ui_chinese_fonts`: the `Hans` faces for the interface, in manifest order. Empty, since
  `build.rs` builds in no Chinese face.
- `ui_cjk_fonts(prefer_hans)`: both in fallback order for the UI language (Chinese group first
  in Chinese mode, so one line never mixes faces with different vertical metrics).
- `ui_arabic_fonts`: the `Arab` faces for Arabic-script interface text (file names, document
  titles), in manifest order.
- `ui_telugu_fonts`: the `Telu` faces for Telugu interface text (the Telugu catalog, file names,
  document titles), in manifest order.
- `document_japanese_font` / `japanese_glyph`: the face (Shippori Mincho, then BIZ UDMincho) whose
  outlines become the Type 3 fallback font for Japanese text written into PDFs. Without it,
  `japanese_glyph` returns `GlyphError::NoFont` and the editor reports a clear error.
- `document_japanese_font_for_style` / `japanese_glyph_from`: select and draw a real Japanese
  fallback face for edited text. Sans sources prefer BIZ UDPGothic Regular/Bold; serif sources
  keep Mincho. Type 3 paths and advances both come from that selected face. Missing weights
  fall back to a real Regular face, never synthetic bold; no italic face is supplied. The
  default `document_japanese_font` / `japanese_glyph` APIs still use regular document Mincho.
- `document_arabic_font` / `shape_arabic` / `arabic_glyph`: the first `Arab` face, shaped with
  harfrust (joining forms, ligatures, mark positions) into clusters for Arabic text written into
  PDFs; the editor draws each cluster as one Type 3 glyph with a ToUnicode entry. Without the face
  they return `GlyphError::NoFont` and the editor reports a clear error.
- `document_japanese_fonts_for_style`: every `Jpan` face in that order of preference, so the
  editor can use the first one that has all of the replacement's glyphs (the faces differ in
  coverage, e.g. of Cyrillic).
- `SHIPPORI_MINCHO`: Shippori Mincho's bytes, or `None`.

Generated Japanese Type 3 fonts record the fallback face's family and style in an indirect
`FontDescriptor`. `pdf::Metrics::base_font` uses its `FontName` when the Type 3 dictionary has
no `BaseFont`, so text inspection and edit-tool results identify the fallback after save/reopen.
The descriptor does not change glyph programs or replace the Type 3 `FontBBox` metrics when
`Ascent`/`Descent` are omitted (PDF Reference 1.7, tables 5.9 and 5.19). The small web input has
only Gothic Regular, so it cannot preserve bold. No new font assets or changes to that input are
needed.

wasm32 builds embed only BIZ UDPGothic Regular and any `Arab` or `Telu` face, to keep the web build
small. The web editor adds Droid Sans Fallback from kition-fonts at run time (`addFont`), which
draws Chinese text added to a page and Chinese CID fonts a PDF does not embed. Font files are never
committed here (`AGENTS.md` §1.4).

The full font subsystem (parsing, shaping, subsetting and embedding) arrives with M2.2/M7.
