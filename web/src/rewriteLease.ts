import type { KitLeases } from '@kition/kit-editor'

/**
 * An edit that rewrites the whole file (applying redactions, sanitizing, adding or removing
 * security) renumbers every object, so the replica records it as a rebase, and edits someone else
 * makes against the old file meanwhile are dropped when the two merge (Kition platform,
 * M21 and M22). So the editor takes the lease on the whole document (`*`) through its host before
 * such an edit and lets it go when the edit ends; the engine says which commands need it
 * (`leaseScope`). While someone else holds it, this editor runs no edit at all: each would be
 * dropped. Without co-editing every edit goes ahead as before, and an offline edit is allowed.
 */

/** The lease scope of the whole document. */
export const WHOLE_DOCUMENT = '*'
/** One renewal's length; the host renews the lease while it is held. */
export const REWRITE_LEASE_TTL = 15_000

/** Who else holds the whole document's lease, if anyone. */
export function rewritingHolder(leases: KitLeases): string | null {
  const other = leases.list().find((lease) => !lease.holder.self && lease.scope === WHOLE_DOCUMENT)
  return other ? other.holder.name : null
}

/**
 * Runs `edit`, the command the editor is about to run, under the lease it needs: `scope` is the
 * engine's `leaseScope` for it (`*` or empty), `mutates` whether it changes the document. A
 * command that rewrites the file takes the lease first and releases it when `edit` settles; a
 * refusal (someone else holds it) calls `refuse` with who, and nothing runs (answers `null`). Any
 * other edit is refused the same way while someone else rewrites the file; what changes nothing
 * always runs.
 */
export async function withRewriteLease<T>(leases: KitLeases, scope: string, mutates: boolean, edit: () => Promise<T>, refuse: (holder: string) => void): Promise<T | null> {
  if (!leases.available || (!mutates && scope !== WHOLE_DOCUMENT)) return edit()
  if (scope !== WHOLE_DOCUMENT) {
    const holder = rewritingHolder(leases)
    if (holder !== null) {
      refuse(holder)
      return null
    }
    return edit()
  }
  const outcome = await leases.acquire(WHOLE_DOCUMENT, REWRITE_LEASE_TTL)
  if (!outcome.granted) {
    // A read-only session's edit runs into the editor's own refusal, which says why.
    if (outcome.reason === 'read-only') return edit()
    refuse(outcome.holder?.name ?? '')
    return null
  }
  try {
    return await edit()
  } finally {
    // `lease` is null when nobody was there to coordinate with: nothing to let go.
    if (outcome.lease) await leases.release(WHOLE_DOCUMENT)
  }
}
