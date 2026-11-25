# kition-pdf-automation

Agent control for Kition PDF: a headless tool table over the engine. The kit's SDK (`kition-pdf-kit`) offers it to agents as commands, which reach them as the KHP `pdf_*` tools of Kition's kits MCP server.

- **Layer:** L7 (architecture §13), but headless. It depends on `engine`, `render` and `organize`, never on a UI toolkit.
- **Status:** over a hundred tools; `tools::tools()` lists the current set with their schemas (the count isn't repeated here, so it can't go stale). They cover inspection, rendering, text, page edits and organizing, page labels, bookmarks, comments and their review, form filling and authoring (fields, properties, scripts, data exchange, detection), redaction and sanitizing, password protection and signatures, calibrated 2D measurements and CSV export, adding text, images, links and stamps, optimizing, OCR, printing, export, metadata, undo/redo, save, combine, extract and split. Comment tools take geometry in the same top-left-origin points as everything else, and `comment.add` can mark text by searching for it (`find`).

## API

```rust
let mut a = Automation::new().with_root("/work")?;      // optional: confine all paths
let doc = a.call("doc_open", &json!({ "path": "in.pdf" }))?;   // Vec<Content>
a.write_output("page1.png", &png)?;                      // save a result yourself, under the same root rules
tools() -> Vec<ToolDef>                                  // name, title, description, input_schema, read_only, destructive, command
```

`Content` is `Json(Value)` or `Png { data, width, height }`. Errors are `UnknownTool`, `InvalidArgs` (the call didn't match the schema) or `Failed` (a readable message for the agent).

## Conventions

- A tool the kit offers as a command is named by its command id (`page.rotate`, `file.getInfo`: Kition's command naming standard) and its parameters are lower camel case (`toPage`). The tools only the headless executable offers keep `snake_case` names (`doc_open`, `doc_save`). `ToolDef::command` links a tool to the registry id it automates (`page.rotate`), and `command_list` reports the link.
- Pages and positions are **1-based**. Rectangles are PDF points with the origin at the top-left of the displayed page.
- Unknown arguments are rejected, so typos fail loudly.
- Tools that change a document return its summary (`doc`, `pages`, `dirty`, `undo`, `redo`, …).
- `comment.previewImage` returns JSON and a PNG: `layer: "background"` (the default) gives the page without the selected image signature/initials; `layer: "image"` gives its embedded image with alpha. The metadata supplies its displayed rectangle, document rotation, `image_rotation` (how far clockwise the image is shown turned: 0 for one placed by Kition PDF, which keeps it upright as displayed) and annotation opacity. Each result can be saved with the CLI's `--out`. Cache the background and transform the image for live movement/resizing; `comment.edit` commits the final geometry in one undo step. Previewing never changes the document.
- For proportional image signature resizing, use the embedded image layer's width/height ratio rather than a previously stretched annotation rectangle. The GUI preserves this ratio at corners with the opposite corner anchored; edge midpoint handles change only their own axis. `comment.edit` accepts the resulting rectangle.
- `doc_close` refuses to drop unsaved changes unless `discard_changes: true`. `doc_save` writes atomically, incrementally in place, and in full for a new path.
- `doc_save` with `flatten_fill_sign: true` prepares the edit, exact saved bytes,
  reopened graph and derived view before the atomic filesystem write. Returned
  preparation, directory, staging-open, partial-write and rename errors preserve
  live marks, annotations, bytes, history, generation and replica state. Successful
  saves retain existing history semantics: full-rewrite undo/redo limitations are
  separate from this transaction guarantee. Incremental history is tested through
  actual content, including later imported peer notes.
- With a root set, every read and write path must resolve inside it (symlinks and `..` included). Relative paths resolve inside the root, and `..` is resolved by name before the check. Every path outside the root gets the same refusal (`<path> is outside the allowed directory <root>`), whether or not it exists, so a confined agent can't probe the rest of the disk; on Windows another network share or device path (`\\host\share`, `\\?\UNC\…`, `\\.\…`) is refused without being contacted. "Not found" and other filesystem errors are reported only for paths inside the root. Tools that write several files into a folder name them after the document or input file, with separators, colons and control characters replaced by `_`, so a name can't lead them out of the folder. The root itself must be a folder.

## Text selection

`text.select {doc, page, anchor: [x,y], focus: [x,y], endPage?, mode?}` reads
selection text and geometry without editing. Pages are 1-based; both points and
returned rectangles use displayed page-local points with top-left origin, crop
and rotation already applied. `mode` defaults to `range`: endpoints select their
nearest glyph inclusively, backward selections normalize to reading order, and
intermediate pages contribute all their text. `endPage` defaults to `page`.

`column` mode requires one page and includes glyphs whose centers lie inside the
rectangle. It uses the existing visual row ordering (with reading-order fallback
for vertical text); its rectangles remain glyph runs in reading order. Output is
`{text, pages: [{page, text, rects}]}` with ascending pages, including blank pages.
The combined text joins page texts with a newline. More than 64 pages, 1 MiB of
combined UTF-8 text including separators, or 16384 rectangles returns an explicit
error without a partial result. Coordinates must be finite and fit `f32`.

Tests: `cargo test -p kition-pdf-automation --test text_selection`.

## Shared field properties

`form.setProps` accepts either `field` (one name) or `fields` (1–1000 unique names):

```json
{"doc": 1, "fields": ["first_name", "last_name"], "required": true,
 "appearance": {"width": 3, "border": "#1473E6"}}
```

The batch is atomic and creates one undo step. A missing or locked field, invalid option,
unknown argument or malformed appearance refuses the whole edit. Set `locked: false` to
unlock and change fields together. Only supplied properties change: each field/widget keeps
its other colours, fonts, values and unknown PDF dictionary keys. `"none"` removes a border
or fill colour. Names, rectangles and rotations require the single-field form. Results keep
`field` for single edits and return `fields` for batches.

## Adding a tool

1. Add the engine capability first, with its tests (the tool is a thin adapter).
2. Add a `ToolDef` in `src/tools.rs`: a precise description, the schema, `ro()`/`destructive()`, and `cmd()` if a registry command exists.
3. Handle it in `Automation::call`, validating pages and positions with the existing helpers.
4. Add an end-to-end test in `tests/automation.rs`. `tool_table_is_well_formed` checks names, schemas and command links.


## Moving several page objects

`object.list {doc, page}` inventories recognised paragraphs, Image/Form artwork and added items
with generation-scoped kind/index references. `object.move {doc, page, objects, offset, generation?}`
moves 1–1000 references atomically in displayed points (right/down), preserving source glyph bytes
and fonts. A refusal changes nothing; success is one undo step. See
[the selection and movement guide](../../../docs/features/editing-multiple-objects.md) for limits and examples.

## Common command tools

`command_list {doc?, filter?, enabled_only?}` keeps its
`{commands: [...]}` result and adds each mapped tool's parameter schema. `command_run {id, params?}`
runs that command through its existing headless tool: include `doc` and other tool arguments in
`params`. Commands without a headless mapping return a tool error. Unknown parameter keys are
ignored with a `warnings` array; required values and paths still go through the original validation.

`command_batch {steps: [{id, params?}], stop_on_error?}` returns `completed`, `failed`, and per-step
`{ok, result?, error?}` entries. The default is to stop at the first error; earlier edits remain.
`file.inspect {doc?}` returns one document's info, or `{documents: [...]}` for all open documents.
`file.render {doc?, page?, maxSide?}` returns a PNG (page defaults to 1, maximum side defaults
to 1024, allowed 1–4096). Omit `doc` only when exactly one document is open.

Exports complete synchronously.
