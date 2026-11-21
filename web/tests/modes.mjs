// The editor in each display mode, through the development host: node tests/modes.mjs [chromium|webkit]
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { runModeSmoke } from '@kition/kit-dev-host/mode-smoke'
// Playwright's WebKit reports a Mac user agent and, from it, navigator.platform "MacIntel" on every
// host, so on Linux the editor expects Cmd while ControlOrMeta presses Ctrl. Keep the host's own
// platform, which is what ControlOrMeta follows.
process.env.PLAYWRIGHT_NO_UA_PLATFORM = '1'

await runModeSmoke({
  kit: 'kit.pdf',
  config: new URL('../vite.config.ts', import.meta.url).pathname,
  browser: process.argv[2] === 'webkit' ? webkit : chromium,
  createServer,
  line: { tab: 'home', primary: ['pdf-previous', 'pdf-page-field', 'pdf-next', 'pdf-fit-width', 'pdf-rotate-right', 'pdf-insert-page', 'pdf-sticky-note', 'pdf-highlight'] },
})
