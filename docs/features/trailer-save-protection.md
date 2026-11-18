# Trailer preservation during PDF saves

How Kition PDF keeps a file's trailer, its original bytes and the editing state intact when it
saves, and where it refuses instead.

## Trailer fields in a full rewrite

A full rewrite knows the trailer keys `Size`, `Root`, `Info`, `Encrypt` and `ID`. Any other
logical trailer field makes the rewrite refuse before it traverses objects, stamps metadata or
allocates output: an unknown value can describe byte offsets or a producer checksum, and copying
it does not prove that it is still valid after the file is rewritten.

- `DocChecksum` has its own unsupported-rewrite error. Checksum-aware full rewriting is not
  supported.
- Decode-limit errors come before checksum errors, which come before other unsupported
  extensions. Error messages do not disclose private extension names.
- An actual xref stream's physical fields (`Type`, `W`, `Index`, `Filter`, `DecodeParms`,
  `Length`, `DL`) are removed by the parser before it builds the logical trailer. A classic
  trailer that uses those names keeps them as opaque fields, so a full rewrite of it refuses.

Ordinary incremental saving keeps the original PDF prefix. Operations that need a full rewrite
still refuse unsupported trailers. This is a bounded preservation policy, not a guarantee of every
producer extension's incremental semantics.

## Original objects written again

An original uncompressed object written again at an appended offset reopens with the same replica
identity and version, without an extra system import transaction. Both xref entries must be live
with equal generations, the new offset must be in the appended bytes, and the object counts as
written only after its exact source value compares equal.

`Document::try_get_original_at_xref` bypasses editing overlays, cached objects, offset shifting and
scan recovery. It checks the header identity and decrypts with the authenticated document context.
Indirect lengths follow the same exact path, bounded to 16 dependency levels; missing, free,
compressed, non-integer and invalid-generation dependencies fail. This checks provenance, not strict
PDF syntax. Compressed relocation is refused.

## Metadata created by saving

Saving may create an Info dictionary in the adopted bytes without changing replica history. A later
metadata edit records the missing trailer reference when it writes that save-created object, so the
edited metadata stays reachable through undo and redo. Identical overlay references are not written
again.

## Compound saves

`doc_save` prepares flattening, serialization, reopening and the derived view data before it calls
the atomic writer, and checks the known save restrictions before it flattens editable Fill & Sign
marks. A preparation or writer error keeps the live document, undo and redo, the generation, the
dirty state, the replica and the destination as they were. After a successful write, the replica
keeps its link and history. Crash durability and filesystem races are outside this synchronous
guarantee.

## Matching the held file after a save

After the kit adopts an incremental save, a clean view matches the exact acknowledged file and
rejects the pristine bytes it replaced. The matcher rebuilds the logical replica overlay with cloned
numbering maps first; it publishes no replica transaction and does not renumber the live view. An
unreadable logical view refuses the match.

## Password-protected files

Structural inspection reuses the editor's password authentication when its input is the same
clean, unrepaired R6 source; otherwise it opens the file with the password again, with the
inspector's 256 MiB limit and repair path. Encryption creation authenticates as owner while
reopening may authenticate as user, so the creation handler never stands in for the reopened
identity. Reuse requires identical reopen and render credential options and the identical source
allocation; the COS layer reparses the original bytes, compares every effective encryption
parameter, and creates fresh parser caches.

## Known limits

- A full rewrite renumbers the file, and the save acknowledgement rebases the replica: an undo
  recorded before a full Save As does not change the new base.
- Undo, redo or importing peer changes can rebuild the view from the replica's older base; a later
  incremental save then keeps the original prefix but not the complete last acknowledged file.
- Checksum-aware full rewriting is not supported.
- The browser tests use in-memory host storage; real host storage and disk durability are not
  covered by them.
