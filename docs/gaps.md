# Gaps

> **Last updated:** 2026-11-27 · **Source:** `features.yaml` (updated 2026-11-27) and the engine's command list

Every known shortfall of Kition PDF, from the feature catalogue: features that are not yet `has`,
then the differences recorded on features that work. Update the feature in `features.yaml` when you
land work, then this list. The comparison with the target application is in
[parity/target-app-parity.md](parity/target-app-parity.md); the order of work is in [roadmap.md](roadmap.md).

## Features not done

- `pdf.agent.premises` (P0, partial): An agent's write lands only on what it observed: what each write rests on is held against other writers. Six named-field reason cells wait for the pinned SDK's observed-name targeting; required deleted/relocated currently report changed. Both matrices have zero wrong writes and false refusals under the same-field rule; 48 default OCR executions remain blocked by missing models.
- `pdf.accessibility` (P1, partial): Accessibility: check, fix, figures and alternative text. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.bookmarks` (P1, partial): Bookmarks: list, add, build from the structure. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.collab.drafts` (P1, partial): Drafts merge in and earlier versions restore as one undo step. The editor has no draft bar and no control to save a labelled version: the host compares, merges and records them; drafts that rewrite one object, or one that predates a full rewrite, are refused, not merged.
- `pdf.compare` (P1, partial): Compare two documents and mark the differences. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.content` (P1, partial): Page content: add text, list, update and delete content. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.document` (P1, partial): Document properties, initial view, revisions, hidden information, flattening and actions. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.fill-sign` (P1, partial): Fill and sign: add text and dates. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.forms` (P1, partial): Forms: fields, fill, reset, add, properties, tab order, detection, actions and scripts. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.history` (P1, partial): Undo and redo. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.images` (P1, partial): Images on a page: list and edit. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.links` (P1, partial): Links: list, add, edit, delete, make from URLs, remove. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.measure` (P1, partial): Measuring: distance, perimeter, area, scale and snapping. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.ocr` (P1, partial): Recognise text in scanned pages. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.page-marks` (P1, partial): Headers and footers, watermarks, backgrounds and removing them. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.pages` (P1, partial): Pages: rotate, delete, move, insert blank, duplicate, page boxes and numbering. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.pdfa` (P1, partial): PDF/A: verify and convert. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.redact` (P1, partial): Redaction: mark, apply, clear. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.security` (P1, partial): Password protection and signatures. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.text` (P1, partial): Text: extract, find, select, lines, paragraphs, edit. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.view` (P1, partial): Render pages and previews; list and move page objects. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.
- `pdf.javascript` (P2, partial): Document JavaScript: run, list, set, enable. The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen.

## Differences on features that work

- `pdf.editor.keyboard-and-view` (P0, has): Measured in dev.html only (whether Kition Desktop gives the editor's frame the keyboard on open is not checked here). Select All takes every page up to 64 (the engine's text.select span) and the current page beyond. A mouse wheel's notch is told from a trackpad by its delta (whole 100 px or lines); real wheels and trackpads are not measured. Cmd+1 and Cmd+2 may be taken by a browser's own tab keys outside Kition.
- `pdf.editor.markup-and-comments` (P0, has): Comments are made and chosen only from text selections, the find bar and the Comments panel: nothing on the page is clicked, moved or resized yet (stage 4 and 5 of the plan). A selection across pages makes one comment per page, each its own undo step (the engine has no batch of edits for the editor). Measured in dev.html only.
- `pdf.editor.params-and-fields` (P0, has): Points and rectangles are still typed: nothing is picked on the page (stage 10 of the plan). Lists of points of points (ink strokes) and objects (form values) are still JSON. Fields are filled in the panel, not on the page (stage 6). Measured in dev.html only.

## Catalogue

The catalogue has 42 features: 20 `has`, 22 not yet.
