/** The development host (dev.html): opens `public/sample/Sample.pdf`, or a file chosen in the page. */
import { startDevHost } from '@kition/sdk/dev-host'

startDevHost({ sample: './sample/Sample.pdf' })

// The editor has the keyboard as soon as it opens (K1): each time the editor's frame loads (the
// sample, or a file chosen in the page, which loads a new frame) the page gives it the focus, and
// the editor gives it to its pages.
document.addEventListener('load', (event) => {
  const frame = event.target
  if (frame instanceof HTMLIFrameElement && frame.id === 'kit') frame.contentWindow?.focus()
}, true)
