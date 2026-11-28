/**
 * Where the keyboard goes when a control lets go of it. The editor's keys (scrolling, zoom, find,
 * undo) are heard on kit-editor's frame (`.kui-app`) and only while the focus is inside it. A page
 * just opened, a parameters dialog that ran or closed, a field's Enter and the page field's Enter
 * all used to leave the focus on the page's `<body>`, and then no key worked until the pages were
 * clicked. Here the pages (`.pdf-pages`) take the keyboard back instead; a key that still finds
 * the body holding it goes to them once through the shell's `keyboardHome` (`pagesRegion`).
 */

/** The pages' region, which holds the keyboard when no control does (the shell's `keyboardHome`). */
export function pagesRegion(): HTMLElement | null {
  return document.querySelector<HTMLElement>('.pdf-pages')
}

/** Whether no element holds the keyboard (the page's body has it). */
export function keyboardLost() {
  const active = document.activeElement
  return !active || active === document.body || active === document.documentElement || !active.isConnected
}

/** Gives the keyboard to the pages; answers whether they have it now. */
export function focusPages() {
  const region = pagesRegion()
  if (!region) return false
  region.focus({ preventScroll: true })
  return document.activeElement === region
}

/**
 * Once the focus settled after something closed (a dialog, a run): when nothing has the keyboard,
 * the pages get it. Later, not at once, so a control the closing focused keeps it; never while the
 * page is in the background (the host has the keyboard then).
 */
export function returnKeyboardLater() {
  window.setTimeout(() => {
    // The frame's root holds it after a click on a control that takes no focus (WebKit's buttons).
    const root = document.activeElement?.classList.contains('kui-app') ?? false
    if ((keyboardLost() || root) && document.hasFocus()) focusPages()
  }, 0)
}

/**
 * A field whose text is chosen whole when it takes the keyboard (the page and zoom fields), so
 * what is typed replaces it. WebKit puts the caret where the click let go, after the focus: the
 * release that follows the focus is kept from doing so.
 */
export const selectOnFocus = {
  onFocus: (event: { currentTarget: HTMLInputElement }) => {
    event.currentTarget.select()
    event.currentTarget.dataset.selecting = 'true'
  },
  onMouseUp: (event: { currentTarget: HTMLInputElement; preventDefault: () => void }) => {
    if (event.currentTarget.dataset.selecting !== 'true') return
    delete event.currentTarget.dataset.selecting
    event.preventDefault()
  },
}
