# Interface language

The kit's editor (`web/`) shows its interface in the language the host asks for: Kition passes its
locale in the Kit UI handshake (`host.init.locale`), and the editor has no language setting of its
own. Command ids, document contents and file names never change with the language.

## Where the strings come from

- **The editor's own strings** are in `web/src/strings.ts`: one catalog per locale (`en-US`,
  `zh-CN`), read through `@kition/kit-editor`'s `createKitStrings`. A key missing from a catalog
  falls back to English.
- **Menu labels and command titles** come from the engine's command registry in English and are
  translated through the catalogs in `web/src/i18n/upstream/<code>.tsv`, which
  `@kition/kit-editor`'s `loadPhrases` reads for the host's locale (`web/src/main.tsx`). Each line
  is `context<TAB>English<TAB>translation`; an empty context is a plain label and `@id` keys an
  entry by command id. The header of `ja.tsv` documents the format and escapes.
- **Labels a catalog lacks** for a locale are in `PHRASES` in `web/src/strings.ts`
  (`withPhrases`); anything still untranslated shows in English.

The catalogs present are Arabic, Bulgarian, Czech, German, Spanish, French, Hungarian, Italian,
Japanese, Polish, Brazilian Portuguese, Russian, Telugu, Ukrainian, and Simplified and Traditional
Chinese. Each is an asset with a `docs/attribution.toml` entry (`kind = "translation"`) checked by
`cargo xtask assets`.

Translations are clean-room: write them from the meaning of the English text, never from another
product's string tables.

## Fonts

The editor draws its interface with the host page's fonts. Text inside a PDF is drawn by the
engine with the document's fonts, the faces the host adds (`addFont`), or the Japanese and Arabic
faces built in when `KITION_FONTS_DIR` names a kition-fonts checkout that has them at build time
(`crates/support/fonts/build.rs`).

## Adding a language

1. Copy the header of `ja.tsv` into `web/src/i18n/upstream/<code>.tsv` and translate entries.
2. Add a `docs/attribution.toml` entry for it (`kind = "translation"`), then run
   `cargo xtask assets --write` and `cargo xtask assets`.
3. If the editor's own strings should be translated too, add a catalog for the locale to
   `CATALOGS` in `web/src/strings.ts`.
