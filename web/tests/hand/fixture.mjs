// The hand tests' synthetic PDF (contributor-original, written here, no third-party sample): eight
// Letter pages of Helvetica text, numbered lines, the word "aurora" a few times on pages 1, 3, 5
// and 7, a heading per page, one text field ("observer") on page 2, a form on page 4 (a required
// text field "city", a drop-down "country" whose options have export values, a check box
// "subscribe" and three radio buttons "size") and a sticky note on page 6 with an author and a date.
// handPdf() answers the file's bytes; node tests/hand/fixture.mjs out.pdf writes them.
// handPdf({ marks: true }) adds what the page layer's tests point at (MARKS): on page 1 a link to
// page 7 over line 3 and a link to a web address over line 30; on page 2 a red square and a second
// sticky note; page 8 turned a quarter (/Rotate 90) with a square of its own.
// handPdf({ readOnly: true }) adds a read-only text field "code" holding READ_ONLY.value on page 4.
// handPdf({ outline: true }) adds the bookmarks OUTLINE (nested; "Part two" starts folded) and
// /PageMode /UseOutlines, so the file asks to open with its bookmarks.
import { writeFileSync } from 'node:fs'

export const PAGES = 8
export const AURORA_PAGES = [1, 3, 5, 7]
/** How many times "aurora" is on each page that has it. */
export const AURORA_PER_PAGE = 3
/** The drop-down's options: export value and what it shows. */
export const COUNTRIES = [['NO', 'Norway'], ['IS', 'Iceland'], ['FI', 'Finland']]
export const SIZES = ['S', 'M', 'L']
/** The read-only field handPdf({ readOnly: true }) adds on page 4 (user space). */
export const READ_ONLY = { name: 'code', value: 'A-1', rect: [380, 600, 460, 618] }
/** What handPdf({ outline: true }) adds: titles, pages (1-based), whether a parent starts unfolded. */
export const OUTLINE = [
  { title: 'Part one', page: 1, open: true, children: [{ title: 'Chapter 1', page: 2 }, { title: 'Chapter 2', page: 3 }] },
  { title: 'Part two', page: 5, open: false, children: [{ title: 'Chapter 3', page: 6 }] },
  { title: 'Appendix', page: 8 },
]
/** The sticky note the file starts with. */
export const NOTE = { page: 6, contents: 'Fixture note', author: 'Fixture author' }
/** What handPdf({ marks: true }) adds, rectangles in PDF user space ([x0, y0, x1, y1], y up). */
export const MARKS = {
  pageLink: { page: 1, rect: [72, 656, 300, 670], to: 7 },
  urlLink: { page: 1, rect: [72, 251, 300, 265], url: 'https://example.org/aurora' },
  square: { page: 2, rect: [300, 480, 420, 560], id: 'fixture-square', contents: 'Square comment' },
  note: { page: 2, rect: [520, 600, 540, 620], id: 'fixture-note-2', contents: 'Second note', author: 'Fixture author' },
  rotated: { page: 8, rect: [100, 600, 200, 700], id: 'fixture-rotated', rotate: 90 },
}

const esc = (text) => text.replace(/([\\()])/g, '\\$1')
const stream = (data) => `<< /Length ${Buffer.byteLength(data, 'latin1')} >>\nstream\n${data}\nendstream`

export function handPdf({ marks = false, readOnly = false, outline = false } = {}) {
  const objects = []
  const add = (body) => { objects.push(body); return objects.length }
  const reserve = () => add(null)
  const set = (n, body) => { objects[n - 1] = body }
  const catalog = reserve()
  const pagesRoot = reserve()
  const helv = add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>')
  const bold = add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>')
  const resources = `<< /Font << /F1 ${helv} 0 R /F2 ${bold} 0 R >> >>`
  const pageIds = Array.from({ length: PAGES }, () => reserve())
  const fields = []
  // A widget's look for one state: a small form XObject (an empty box, or a box with a cross).
  const look = (cross) => add(stream(cross ? '0 g 2 2 m 10 10 l 10 2 m 2 10 l S' : '0.5 g 0.5 0.5 11 11 re S').replace('<<', '<< /Type /XObject /Subtype /Form /BBox [0 0 12 12]'))
  const on = look(true)
  const off = look(false)
  for (let n = 1; n <= PAGES; n += 1) {
    const lines = []
    for (let i = 1; i <= 40; i += 1) {
      const word = AURORA_PAGES.includes(n) && i % 12 === 5 ? 'aurora' : 'display'
      lines.push(`${n}.${i} A quiet ${word} grows over the hills before the rays and the curtain.`)
    }
    let content = `BT /F2 20 Tf 72 720 Td (Page ${n} heading) Tj ET\n`
    content += `BT /F1 11 Tf 72 690 Td 15 TL\n${lines.map((line) => `(${esc(line)}) Tj T*`).join('\n')}\nET\n`
    let annots = ''
    if (n === 2) {
      const field = add(`<< /Type /Annot /Subtype /Widget /FT /Tx /T (observer) /P ${pageIds[1]} 0 R /Rect [380 712 540 734] /F 4 /DA (/F1 12 Tf 0 g) /MK << /BC [0.4 0.4 0.4] >> /BS << /W 1 >> >>`)
      fields.push(field)
      annots = ` /Annots [${field} 0 R]`
    }
    if (n === 4) {
      const page = pageIds[3]
      const box = '/F 4 /DA (/F1 12 Tf 0 g) /MK << /BC [0.4 0.4 0.4] >> /BS << /W 1 >>'
      const city = add(`<< /Type /Annot /Subtype /Widget /FT /Tx /T (city) /TU (City) /Ff 2 /P ${page} 0 R /Rect [380 712 540 734] ${box} >>`)
      const country = add(`<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (country) /TU (Country of residence) /Opt [${COUNTRIES.map(([value, label]) => `[(${value}) (${label})]`).join(' ')}] /P ${page} 0 R /Rect [380 682 540 704] ${box} >>`)
      const subscribe = add(`<< /Type /Annot /Subtype /Widget /FT /Btn /T (subscribe) /V /Off /AS /Off /P ${page} 0 R /Rect [380 656 392 668] /F 4 /AP << /N << /Yes ${on} 0 R /Off ${off} 0 R >> >> >>`)
      const size = reserve()
      const kids = SIZES.map((state, index) => add(`<< /Type /Annot /Subtype /Widget /Parent ${size} 0 R /AS /Off /P ${page} 0 R /Rect [${380 + index * 30} 630 ${392 + index * 30} 642] /F 4 /AP << /N << /${state} ${on} 0 R /Off ${off} 0 R >> >> >>`))
      set(size, `<< /FT /Btn /Ff 49152 /T (size) /V /Off /Kids [${kids.map((kid) => `${kid} 0 R`).join(' ')}] >>`)
      fields.push(city, country, subscribe, size)
      const locked = readOnly ? [add(`<< /Type /Annot /Subtype /Widget /FT /Tx /T (${READ_ONLY.name}) /Ff 1 /V (${READ_ONLY.value}) /P ${page} 0 R /Rect [${READ_ONLY.rect.join(' ')}] ${box} >>`)] : []
      fields.push(...locked)
      annots = ` /Annots [${[city, country, subscribe, ...kids, ...locked].map((id) => `${id} 0 R`).join(' ')}]`
    }
    const square = ({ rect, id, contents }) => {
      const [x0, y0, x1, y1] = rect
      const look = add(stream(`1 0 0 RG 2 w ${x0 + 1} ${y0 + 1} ${x1 - x0 - 2} ${y1 - y0 - 2} re S`).replace('<<', `<< /Type /XObject /Subtype /Form /BBox [${rect.join(' ')}]`))
      return add(`<< /Type /Annot /Subtype /Square /Rect [${rect.join(' ')}] /NM (${id}) /T (${NOTE.author}) /M (D:20261120100000Z)${contents ? ` /Contents (${contents})` : ''} /C [1 0 0] /BS << /W 2 >> /F 4 /AP << /N ${look} 0 R >> >>`)
    }
    if (marks && n === MARKS.pageLink.page) {
      const inside = add(`<< /Type /Annot /Subtype /Link /Rect [${MARKS.pageLink.rect.join(' ')}] /Border [0 0 0] /Dest [${pageIds[MARKS.pageLink.to - 1]} 0 R /XYZ 0 792 null] >>`)
      const web = add(`<< /Type /Annot /Subtype /Link /Rect [${MARKS.urlLink.rect.join(' ')}] /Border [0 0 0] /A << /S /URI /URI (${MARKS.urlLink.url}) >> >>`)
      annots = ` /Annots [${inside} 0 R ${web} 0 R]`
    }
    if (marks && n === MARKS.square.page) {
      const box = square(MARKS.square)
      const { rect, id, contents, author } = MARKS.note
      const note = add(`<< /Type /Annot /Subtype /Text /Rect [${rect.join(' ')}] /NM (${id}) /T (${author}) /M (D:20261120100000Z) /Contents (${contents}) /C [1 0.8 0] /F 4 >>`)
      annots = annots.replace(']', ` ${box} 0 R ${note} 0 R]`)
    }
    let rotate = ''
    if (marks && n === MARKS.rotated.page) {
      annots = ` /Annots [${square(MARKS.rotated)} 0 R]`
      rotate = ` /Rotate ${MARKS.rotated.rotate}`
    }
    if (n === NOTE.page) {
      const note = add(`<< /Type /Annot /Subtype /Text /Rect [500 700 520 720] /NM (fixture-note) /T (${NOTE.author}) /M (D:20261120100000Z) /Contents (${NOTE.contents}) /C [1 0.8 0] /F 4 >>`)
      annots = ` /Annots [${note} 0 R]`
    }
    const contents = add(stream(content))
    set(pageIds[n - 1], `<< /Type /Page /Parent ${pagesRoot} 0 R /MediaBox [0 0 612 792] /Resources ${resources} /Contents ${contents} 0 R${annots}${rotate} >>`)
  }
  let outlines = ''
  if (outline) {
    const root = reserve()
    // Each level's items, linked to their parent and siblings; /Count: open (+) or folded (-).
    const level = (list, parent) => {
      const ids = list.map(() => reserve())
      list.forEach((item, index) => {
        const kids = item.children ? level(item.children, ids[index]) : null
        const count = kids ? (item.open ? kids.length : -kids.length) : 0
        set(ids[index], `<< /Title (${esc(item.title)}) /Parent ${parent} 0 R /Dest [${pageIds[item.page - 1]} 0 R /XYZ null null null]${index > 0 ? ` /Prev ${ids[index - 1]} 0 R` : ''}${index < ids.length - 1 ? ` /Next ${ids[index + 1]} 0 R` : ''}${kids ? ` /First ${kids[0]} 0 R /Last ${kids.at(-1)} 0 R /Count ${count}` : ''} >>`)
      })
      return ids
    }
    const top = level(OUTLINE, root)
    set(root, `<< /Type /Outlines /First ${top[0]} 0 R /Last ${top.at(-1)} 0 R /Count ${top.length + OUTLINE.filter((item) => item.open).reduce((sum, item) => sum + item.children.length, 0)} >>`)
    outlines = ` /Outlines ${root} 0 R /PageMode /UseOutlines`
  }
  set(pagesRoot, `<< /Type /Pages /Kids [${pageIds.map((id) => `${id} 0 R`).join(' ')}] /Count ${PAGES} >>`)
  set(catalog, `<< /Type /Catalog /Pages ${pagesRoot} 0 R${outlines} /AcroForm << /Fields [${fields.map((id) => `${id} 0 R`).join(' ')}] /NeedAppearances true /DA (/F1 12 Tf 0 g) /DR ${resources} >> >>`)
  let pdf = '%PDF-1.7\n%\xE2\xE3\xCF\xD3\n'
  const offsets = []
  objects.forEach((body, index) => {
    offsets.push(Buffer.byteLength(pdf, 'latin1'))
    pdf += `${index + 1} 0 obj\n${body}\nendobj\n`
  })
  const xref = Buffer.byteLength(pdf, 'latin1')
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets.map((offset) => `${String(offset).padStart(10, '0')} 00000 n \n`).join('')}`
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root ${catalog} 0 R >>\nstartxref\n${xref}\n%%EOF\n`
  return Buffer.from(pdf, 'latin1')
}

if (process.argv[1] && new URL(import.meta.url).pathname === process.argv[1] && process.argv[2]) writeFileSync(process.argv[2], handPdf())
