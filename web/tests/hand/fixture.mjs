// The hand tests' synthetic PDF (contributor-original, written here, no third-party sample): eight
// Letter pages of Helvetica text, numbered lines, the word "aurora" a few times on pages 1, 3, 5
// and 7, a heading per page, and one text field ("observer") on page 2.
// handPdf() answers the file's bytes; node tests/hand/fixture.mjs out.pdf writes them.
import { writeFileSync } from 'node:fs'

export const PAGES = 8
export const AURORA_PAGES = [1, 3, 5, 7]
/** How many times "aurora" is on each page that has it. */
export const AURORA_PER_PAGE = 3

const esc = (text) => text.replace(/([\\()])/g, '\\$1')
const stream = (data) => `<< /Length ${Buffer.byteLength(data, 'latin1')} >>\nstream\n${data}\nendstream`

export function handPdf() {
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
    const contents = add(stream(content))
    set(pageIds[n - 1], `<< /Type /Page /Parent ${pagesRoot} 0 R /MediaBox [0 0 612 792] /Resources ${resources} /Contents ${contents} 0 R${annots} >>`)
  }
  set(pagesRoot, `<< /Type /Pages /Kids [${pageIds.map((id) => `${id} 0 R`).join(' ')}] /Count ${PAGES} >>`)
  set(catalog, `<< /Type /Catalog /Pages ${pagesRoot} 0 R /AcroForm << /Fields [${fields.map((id) => `${id} 0 R`).join(' ')}] /NeedAppearances true /DA (/F1 12 Tf 0 g) /DR ${resources} >> >>`)
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
