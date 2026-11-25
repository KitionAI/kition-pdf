/**
 * Edits the document's replica dropped (the engine's `kdmDropped`, a count that only grows). An
 * undo or redo of an edit that rewrote the whole file (applying redactions, sanitizing, changing
 * its security) under someone else's later edits brings the other file back, and those edits,
 * made on the file the undo replaced, cannot stay in it: the engine drops them. A draft made on
 * an older file is refused instead, saying why. The editor tells the person when the count grows.
 */

/** The edits dropped since the count was `seen` (the count is `now`): none when it did not grow. */
export function newlyDropped(seen: number, now: number): number {
  return Number.isFinite(seen) && Number.isFinite(now) && now > seen ? now - seen : 0
}
