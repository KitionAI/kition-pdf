# Target-app parity

> **Last updated:** 2026-11-27 · **Source:** `features.yaml` (updated 2026-11-27) and the engine's command list

Each feature in `features.yaml` records its status against what the feature promises and, where it
has one, the target application it is compared with (Adobe Acrobat Pro 26.002.21931).
The difference column is the feature's recorded gap.

| Feature | Priority | Status | Target | Difference |
| --- | --- | --- | --- | --- |
| `pdf.agent.act` An agent inserts and orders pages | P0 | has | — | — |
| `pdf.agent.observe` An agent reads the pages and marks the right one | P0 | has | — | — |
| `pdf.agent.premises` An agent's write lands only on what it observed: what each write rests on is held against other writers | P0 | partial | — | Six named-field reason cells wait for the pinned SDK's observed-name targeting; required deleted/relocated currently report changed. Both matrices have zero wrong writes and false refusals under the same-field rule; 48 default OCR executions remain blocked by missing models. |
| `pdf.agent.recover` An agent fixes what the check reports | P0 | has | — | — |
| `pdf.bookmarks.edit` Nested bookmarks with CJK titles survive saving and reopening | P0 | has | Adobe Acrobat Pro 26.002.21931 | — |
| `pdf.comments` Comments: add, reply, edit, set status, mark, lock, hide, delete | P0 | has | Adobe Acrobat Pro 26.002.21931 | — |
| `pdf.forms.fill` Text, check box and combo box fields and their filled values survive saving and reopening | P0 | has | Adobe Acrobat Pro 26.002.21931 | — |
| `pdf.links.edit` Web and page links survive saving and reopening | P0 | has | Adobe Acrobat Pro 26.002.21931 | — |
| `pdf.pages.organize` Inserted, duplicated, rotated and moved pages, crop boxes and page labels survive saving and reopening | P0 | has | Adobe Acrobat Pro 26.002.21931 | — |
| `pdf.redact.apply` Applied redactions remove the text from the page and the saved file, and survive the full rewrite and reopening | P0 | has | Adobe Acrobat Pro 26.002.21931 | — |
| `pdf.text.edit` Added and edited page text survives saving and reopening | P0 | has | Adobe Acrobat Pro 26.002.21931 | — |
| `pdf.accessibility` Accessibility: check, fix, figures and alternative text | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.agent.draft` An agent works in a draft over a conversation | P1 | has | — | — |
| `pdf.bookmarks` Bookmarks: list, add, build from the structure | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.compare` Compare two documents and mark the differences | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.content` Page content: add text, list, update and delete content | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.document` Document properties, initial view, revisions, hidden information, flattening and actions | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.document-text` Hosts read a PDF's words page by page through document.text, for search | P1 | has | — | — |
| `pdf.editor.display-modes` The editor in Menus, Single line, Classic and Focus; Focus names the tool, hides the find bar, and Escape clears a selection first | P1 | has | — | — |
| `pdf.editor.page-scroll` Page scrollbars show where the view is and drag through the pages; scrolling stops at the first and last page | P1 | has | — | — |
| `pdf.editor.keyboard-and-view` The editor's keyboard and view: the keys work as soon as a PDF opens and after a dialog or a field; reader's scroll keys; Cmd+0/1/2 fit page, 100 %, fit width; a zoom ladder, a typed percentage and a 1.2-times wheel notch; Select All and Copy from anywhere; find keeps its match through an edit | P0 | has | — | Measured in dev.html only. Select All takes every page up to 64, the current page beyond. Real wheels and trackpads are not measured. |
| `pdf.editor.markup-and-comments` Marking text and working with comments without a dialog: Highlight, Underline and Strikethrough mark the selection or the found match at once; a bar beside a selection; the Comments panel goes to a comment, edits, replies, sets its status and deletes it; the Comments tab acts on the chosen one | P0 | has | — | Drawing on the page is pdf.editor.annotation-tools. A selection across pages makes one comment per page, each its own undo step. Measured in dev.html only. |
| `pdf.editor.params-and-fields` Editing without JSON: the parameters dialog names its fields, takes pages as 3 or 2-4 and points as 300, 400, starts from the selection, the last press and the current page, and keeps what was typed when a command fails; the form fields panel draws each field for its type and shows the value an undo brings back | P0 | has | — | Points and rectangles are still typed, not picked on the page. Ink strokes and form values are still JSON. Fields are filled in the panel, not on the page. Measured in dev.html only. |
| `pdf.editor.annotation-select` Comments on the page: a click selects one (frame, handles), hovering shows its author and text, a drag moves it, a handle resizes it, the arrows nudge it, Delete deletes it, Escape lets go, a double-click edits its text, each gesture one undo step; the cursor says what is under it; right-click menus for a comment, selected text and the page | P0 | has | — | One comment at a time. A drag moves a frame, not the comment's pixels. Lines and polygons have no point handles. Measured in dev.html only. |
| `pdf.editor.annotation-tools` Comment tools on the page: a tool rail with V and H; a sticky note placed with a click and written at once; a text box dragged or clicked; the highlighter, underline and strikethrough mark the text dragged over and stay on; rectangle, oval, line, arrow (Shift) and drawing, each one undo step and selected once made; an options bar for colour, opacity and line width | P0 | has | — | No callout, squiggly, polygon, cloud, eraser or stamp tool yet. No fill or font size in the bar; a comment's own opacity and width show only once set this session. Measured in dev.html only. |
| `pdf.editor.links` Links on the page: a hand cursor; a click goes to the linked page and Alt+Left comes back; a web address is asked about and opened by the host | P1 | has | — | A link goes to the top of its page (no destination place or zoom). Only links go into the view history. Measured in dev.html only. |
| `pdf.fill-sign` Fill and sign: add text and dates | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.forms` Forms: fields, fill, reset, add, properties, tab order, detection, actions and scripts | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.history` Undo and redo | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.images` Images on a page: list and edit | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.links` Links: list, add, edit, delete, make from URLs, remove | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.measure` Measuring: distance, perimeter, area, scale and snapping | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.ocr` Recognise text in scanned pages | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.page-marks` Headers and footers, watermarks, backgrounds and removing them | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.pages` Pages: rotate, delete, move, insert blank, duplicate, page boxes and numbering | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.pdfa` PDF/A: verify and convert | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.redact` Redaction: mark, apply, clear | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.security` Password protection and signatures | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.text` Text: extract, find, select, lines, paragraphs, edit | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.view` Render pages and previews; list and move page objects | P1 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |
| `pdf.javascript` Document JavaScript: run, list, set, enable | P2 | partial | Adobe Acrobat Pro 26.002.21931 | The engine has these commands; no case yet shows the area working end to end in the editor and surviving save and reopen. |

| Priority | has | partial | missing |
| --- | ---: | ---: | ---: |
| P0 | 10 | 1 | 0 |
| P1 | 4 | 19 | 0 |
| P2 | 0 | 1 | 0 |
