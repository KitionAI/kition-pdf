# Target-app parity

> **Last updated:** 2026-11-17 · **Source:** `features.yaml` (updated 2026-11-17) and the engine's command list

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
