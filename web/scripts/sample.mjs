// Makes public/sample/Sample.pdf with the engine itself: two pages with text, bookmarks, a
// comment, a highlight, and a form field.
import fs from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = fileURLToPath(new URL('..', import.meta.url))
const engine = await import(pathToFileURL(`${web}public/engine/kition_pdf_kit.js`).href)
await engine.default({ module_or_path: await fs.readFile(`${web}public/engine/kition_pdf_sdk_bg.wasm`) })
const doc = engine.PdfDocument.create('Sample.pdf', 612, 792)
const run = (command, params = {}) => doc.execute(command, JSON.stringify(params))
run('page.addText', { page: 1, text: 'Field Guide to the Northern Lights', at: [72, 90], size: 24, bold: true, width: 468 })
run('page.addText', { page: 1, text: 'Auroras appear when charged particles from the Sun meet the upper atmosphere.', at: [72, 140], size: 12, width: 468 })
run('page.insertBlank', { at: 2 })
run('page.addText', { page: 2, text: 'Colors by height', at: [72, 90], size: 18, bold: true })
run('page.addText', { page: 2, text: 'Green comes from oxygen near 100 km; red from oxygen higher up.', at: [72, 130], size: 12, width: 468 })
run('page.addText', { page: 2, text: 'Observer:', at: [72, 196], size: 12 })
run('form.addField', { page: 2, name: 'observer', type: 'text', rect: [140, 184, 360, 204] })
run('bookmark.add', { page: 1, title: 'Introduction' })
run('bookmark.add', { page: 2, title: 'Colors by height' })
run('comment.add', { page: 1, type: 'note', at: [540, 60], contents: 'Add a photo here' })
run('comment.add', { page: 2, type: 'highlight', find: 'oxygen' })
await fs.writeFile(`${web}public/sample/Sample.pdf`, doc.save())
console.log('wrote public/sample/Sample.pdf')
