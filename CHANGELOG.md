# Changelog

Changes to Kition PDF, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## 0.1.0

First public release of the source, under the Apache License 2.0. It contains:

- The Rust engine: a tolerant PDF parser with repair, incremental and full saves, decryption and
  re-encryption; CPU rendering and text extraction; pages, comments and replies, forms with their
  scripts, bookmarks, links, watermarks, headers and footers, redaction, measuring, comparison,
  PDF/A, signatures, OCR and accessibility tools.
- The kit engine (`kition-pdf-kit`), native and WebAssembly: the document is a KDM replica (the
  file as a base and an overlay of edited objects), replicas merge concurrent edits, and undo is
  per person and per agent. A check catalogue of 13 diagnostics reports what is wrong with a file,
  such as a broken link, an unembedded font or an empty required field.
- The headless kit (`kition-pdf`) for agents over the Kit Protocol: it reads a PDF's pages,
  comments, fields, links and bookmarks as typed objects, edits them with the editor's own
  commands, previews pages, and works in drafts. A write names the revision it read and is
  refused when someone else changed what it rests on. It reads nothing outside its workspace.
- The kit's editor (`web/`): page thumbnails, a ribbon and a side panel of bookmarks, comments and
  form fields, in kit-editor's frame.
- NOTICE and docs/attribution.md list the upstream work, the fonts and every asset, and Loro
  (MIT), bundled by the editor and linked by the engine.
