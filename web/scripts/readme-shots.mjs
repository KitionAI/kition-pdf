// Takes the README's pictures of the PDF editor (assets/readme/*.png at the repository root) on
// kition-sdk's shared runner (tools/readme-shots/README.md there): the development host in Classic
// mode, light theme, English, 1440 × 900 at device scale 2, on kition-showcase samples. Not a test;
// re-run it by hand when the editor changes:
//
//   cd web && pnpm install && pnpm run engine      # the engine and fonts in public/
//   node scripts/readme-shots.mjs [name ...]       # every shot, or only the named ones
//
// It needs kition-sdk and kition-showcase checkouts next to this repository (or KITION_SDK and
// KITION_SHOWCASE naming them). Run it with no HTTP proxy set. Every step is a person's: a click on
// the ribbon, the side panel's tabs, a thumbnail or a bookmark. The pictures are PNG at device scale
// 2; shrink them afterwards with `pngquant --quality 70-90` (the README's are at most 400 KB each).
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(web, '..', '..', 'kition-sdk'))
const showcase = path.resolve(process.env.KITION_SHOWCASE ?? path.join(web, '..', '..', 'kition-showcase'))
const { openFile, takeShots } = await import(pathToFileURL(path.join(sdk, 'tools/readme-shots/shots.mjs')).href)

/** The showcase sample each shot opens. */
const SAMPLE = {
  hero: 'reviewed-quarterly-report',
  pages: 'confidential-exhibit-bates',
  bookmarks: 'volunteer-handbook-outline',
  forms: 'swim-club-registration-form',
  redaction: 'redacted-incident-memo',
}

await takeShots({
  web,
  sample: async (page, shot) => {
    const name = SAMPLE[shot.name]
    await openFile(page, { name: `${name}.pdf`, bytes: fs.readFileSync(path.join(showcase, 'pdf', name, `${name}.pdf`)) })
  },
  ready: '[data-testid="pdf-side"]',
  query: { modes: 'classic' },
  shots: [
    // A report page marked up by two reviewers, the whole page in view (Fit Page), and the side
    // panel's Comments: notes with their text, highlights, a strikeout, a cloud, a callout, a stamp.
    {
      name: 'hero',
      steps: async (page, { kit }) => {
        await kit.getByTestId('pdf-fit-page').click()
        await kit.getByTestId('pdf-panel-comments').click()
      },
    },
    // A three-page exhibit (a watermark, Bates numbers): the page thumbnails, the second page
    // chosen, and the Home tab's page commands.
    {
      name: 'pages',
      steps: async (page, { kit }) => {
        await kit.getByTestId('pdf-fit-page').click()
        await page.waitForTimeout(600)
        await kit.getByTestId('pdf-thumb-1').click()
      },
    },
    // A handbook's nested bookmarks; a click on "Contents" goes to its page (labelled ii).
    {
      name: 'bookmarks',
      steps: async (page, { kit }) => {
        await kit.getByTestId('pdf-fit-page').click()
        await kit.getByTestId('pdf-panel-bookmarks').click()
        await kit.getByText('Contents', { exact: true }).click()
      },
    },
    // A filled-in membership form and the side panel's Form fields with each field's value.
    { name: 'forms', steps: async (page, { kit }) => kit.getByTestId('pdf-panel-fields').click() },
    // A memo released with names, numbers and a paragraph redacted and labelled, and the Protect tab.
    { name: 'redaction', steps: async (page, { kit }) => kit.getByRole('tab', { name: 'Protect' }).click() },
  ],
})
