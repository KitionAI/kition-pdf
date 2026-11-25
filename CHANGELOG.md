# Changelog

Changes to Kition PDF, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## Unreleased

- The engine runs on kition-sdk's plain document model (no CRDT): Kition Desktop is for one person, so a PDF's replica is no longer merged from concurrent edits; changes reach a replica in one order. Conflict objects (the edits a full rewrite could not keep) are gone with the merge, and so is the editor's report of them; the whole-document lease stays. The engine answers receipts itself (`changesSince`), merges drafts (`mergeChanges`, `compareChanges`) and restores earlier versions (`restoreState`) as one undo step each, names objects by their tool ids in replies (`modelReply`), and serves the object model (`modelDescribe`, `modelBatch`) in the editor. Loro is no longer bundled or linked.

## 0.1.4

- The same kit as 0.1.3, built on kition-sdk 6672663 (the replica's undo passes over an undo group whose changes came to nothing): the 0.1.3 package was built on kition-sdk 6b4d471.

## 0.1.3

- A host asks what document a PDF file is (`document.identity`) and copies one as a new document (`document.fork`), Kit Process Protocol section 5.6 (kition-sdk 6672663). A `.pdf` embeds no replica (the kit makes one under a new id each time it opens a file), so a PDF carries no document id and its fork is the same bytes.
- The headless program speaks Kit Process Protocol version 1 (`headless.protocol` is `kition-kit/1`), as hosts on kition-sdk from bb1b07b require.

## 0.1.2

- `kit.json` declares the engine's editor state as `engine.editorState`, the same list as the engine's `EDITOR_STATE`, so a host that does not run the engine reads `inspect()` without it.
- Lint fix in the visual compare (clippy 1.100), no change in behaviour.
- The README connects an agent with `npm install -g @kition/cli` and `kition agent add`.

## 0.1.1

- The pasteboard around the pages follows the host's theme, live: in a dark host it is dark, and it repaints when the host's theme changes (before, it kept its built-in light grey).

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
