//! [`PdfSession`]: one PDF's replica, its [`PdfModel`] view and its blobs, for the engine's
//! command bridge.
//!
//! It is `kition_kit::session::KitSession<PdfModel>`'s flow (transactions, per-actor undo, the
//! change exchange, the post-merge repair through [`KitModel::validate`], the view reloaded
//! through [`KitModel::apply`]) on concrete parts, as Kition Photo's `PhotoSession` and Kition
//! Slide's `SlideSession` do: the engine must enumerate, ship and garbage-collect the blobs (the
//! base PDF and stream data), which `KitSession`'s boxed blob store does not allow. Edits are
//! diff-bridged: the engine changes its own COS document, and [`PdfSession::record`] writes
//! what changed as one transaction ([`write_changes`]), then adopts it as the view. Everything
//! here is `Send`.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, OriginKind, Replica, Tx, UndoScope, Version};
use kition_kit::session::{BlobSource, Damage, KitModel, MemoryBlobs};
use kition_pdf_cos::Document;

use crate::blob_gc::{BlobIndex, Moved};
use crate::read::{Bases, read_document, stale_entries};
use crate::write::{Written, base_max, rebase, write_changes, write_document};
use crate::{Ids, PdfModel, Result, kit_err, same_bytes};

/// One open PDF as a KDM replica and the engine's view of it. See the module docs.
pub struct PdfSession {
    replica: Replica,
    model: PdfModel,
    blobs: MemoryBlobs,
    /// Which blobs are still needed ([`crate::blob_gc`]); the rest are dropped after each change.
    index: BlobIndex,
    /// The hashes each record, import, undo and redo started referencing, with the version right
    /// after it, for [`PdfSession::blobs_since`] (Kition Photo's blob log: chronological, so
    /// `since` cleanly splits it into "already has" and "needs"; trimmed with the store).
    blob_log: Vec<(Version, Vec<String>)>,
    /// Overlay entries of an old epoch the repair dropped ([`PdfSession::dropped`]).
    dropped: u64,
    /// What the last record wrote (for tests and diagnostics).
    last: Written,
}

impl std::fmt::Debug for PdfSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfSession").field("doc", &self.replica.doc_id()).field("actor", &self.replica.actor()).finish_non_exhaustive()
    }
}

/// `doc`'s base opened afresh: `doc` itself when it has no edits (freshly opened or rewritten).
fn pristine_of(doc: &Document, password: Option<&str>) -> Result<Document> {
    if !doc.is_modified() {
        return Ok(doc.clone());
    }
    Document::open_with_password(doc.bytes().clone(), password).map_err(kit_err)
}

impl PdfSession {
    /// A new replica holding `doc` (opened from bytes; protected bases open with `password`),
    /// written in its creating transaction (not an undo step). The view is `doc` itself, with
    /// its own object numbers.
    pub fn create(doc: &Document, password: Option<&str>) -> Result<Self> {
        if doc.bytes().is_empty() {
            return Err(kit_err("a document made in memory has no base file to replicate; save it first"));
        }
        let pristine = pristine_of(doc, password)?;
        let actor = ActorId::random();
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::new(actor.get());
        let (mut replica, written) = Replica::create_with(PdfModel::schema(), DocId::new(), actor, &Origin::system("create"), |tx| {
            write_document(tx, doc, &pristine, &mut ids, &mut blobs)
        })
        .map_err(kit_err)?;
        replica.take_changed();
        let index = BlobIndex::build(&replica);
        let mut bases = Bases::default();
        bases.insert(blobs_hash(&replica), pristine.clone());
        let model = PdfModel::from_parts(doc.clone(), pristine, ids, bases, password.map(str::to_owned));
        Ok(Self { replica, model, blobs, index, blob_log: Vec::new(), dropped: 0, last: written })
    }

    /// Opens a snapshot (or a full change export) of another replica as a new replica of a fresh
    /// actor. `blobs` must hold the blobs it references; a protected base opens with `password`.
    pub fn open_snapshot(bytes: &[u8], blobs: MemoryBlobs, password: Option<&str>) -> Result<Self> {
        let mut replica = Replica::load(PdfModel::schema(), bytes, ActorId::random()).map_err(kit_err)?;
        replica.take_changed();
        let model = PdfModel::read(&replica, &blobs, password)?;
        let index = BlobIndex::build(&replica);
        let mut session = Self { replica, model, blobs, index, blob_log: Vec::new(), dropped: 0, last: Written::default() };
        session.index.collect_garbage(&mut session.blobs);
        Ok(session)
    }

    /// The view: the document as of the last record, undo, redo or import.
    pub fn document(&self) -> &Document {
        &self.model.doc
    }

    pub fn model(&self) -> &PdfModel {
        &self.model
    }

    /// The password protected bases open with (a peer's rebase onto a protected file needs it).
    pub fn set_password(&mut self, password: Option<String>) {
        self.model.set_password(password);
    }

    /// The typed objects of the view, objects without a stored id named by their overlay keys
    /// (as the replica holds them).
    pub fn projection(&self) -> crate::projection::Projection {
        crate::projection::project_keyed(&self.model.doc, &crate::projection::keyer(self.model.ids()))
    }

    /// The overlay key of object `num` of the view (its number for an object of the base).
    pub fn key_of(&self, num: u32) -> String {
        self.model.ids().known_key(num).unwrap_or_else(|| num.to_string())
    }

    /// What the last record wrote: the overlay keys, and whether it was a rebase.
    pub fn last_written(&self) -> &Written {
        &self.last
    }

    /// How many overlay entries the repair dropped because they were written against another base
    /// than the replica's: an undo (or redo) of one scope's full rewrite under the other scope's
    /// later edits (the person undoes a sanitize after an agent's comment) brings the old base
    /// back and keeps the later entries, written against the rewritten one. Nothing else counts
    /// here: changes apply in one order (a peer's arrive with its own repair), and a draft whose
    /// entries would be of another base is refused ([`PdfSession::merged_state`]). The editor
    /// shows it.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Writes what differs between the view and `after` as one transaction of `origin`, then
    /// makes `after` the view: overlay entries, or a rebase when `after` sits on other bytes (a
    /// full rewrite). Answers how many undo steps that made: 1, or 0 when nothing replicated
    /// changed or `origin` is a `system`/`import` one (never an undo step). On failure nothing is
    /// written and the view is unchanged.
    pub fn record(&mut self, origin: &Origin, after: &Document) -> Result<u32> {
        let pristine_after = if same_bytes(self.model.doc.bytes(), after.bytes()) { None } else { Some(pristine_of(after, self.model.password())?) };
        self.write(origin, after, pristine_after, false)
    }

    /// Rebases the replica onto `doc`'s bytes (a new epoch) even when the view already sits on
    /// them: a signed file, whose bytes every peer must have exactly.
    pub fn rebase(&mut self, origin: &Origin, doc: &Document) -> Result<u32> {
        let pristine = pristine_of(doc, self.model.password())?;
        self.write(origin, doc, Some(pristine), true)
    }

    fn write(&mut self, origin: &Origin, after: &Document, pristine_after: Option<Document>, force: bool) -> Result<u32> {
        let mut ids = self.model.ids().clone();
        let version = self.replica.version();
        let (model, blobs) = (&self.model, &mut self.blobs);
        let result = self.replica.transact(origin, |tx| match (&pristine_after, force) {
            (Some(pristine), true) => rebase(tx, after, pristine, &mut ids, blobs),
            _ => write_changes(tx, &model.doc, after, &model.pristine, pristine_after.as_ref(), &mut ids, blobs),
        });
        let changed = self.replica.take_changed();
        let written = match result {
            Ok(written) => written,
            Err(error) => {
                self.index.collect_garbage(&mut self.blobs);
                return Err(kit_err(error));
            }
        };
        let moved = self.index.update(&self.replica, &changed);
        if let Some(pristine) = &pristine_after {
            let hash = blobs_hash(&self.replica);
            self.model.bases_mut().insert(hash, pristine.clone());
        }
        self.model.adopt(after.clone(), ids, pristine_after);
        self.last = written;
        let wrote = self.replica.version() != version;
        let undoable = wrote && !matches!(origin.kind, OriginKind::System | OriginKind::Import);
        self.log_added(moved.added.iter().cloned());
        if undoable {
            self.index.push_step(moved);
        }
        self.index.collect_garbage(&mut self.blobs);
        Ok(u32::from(undoable))
    }

    /// Takes `doc` as the view without writing anything: the file the view was just saved to
    /// (its bytes the base's with the view's edits appended, the same objects under the same
    /// numbers), so the next edit diffs against it and the next save appends to it. The replica
    /// keeps its base: an undo past the save still finds what it undoes.
    pub fn adopt_local(&mut self, doc: Document) {
        let ids = self.model.ids().clone();
        self.model.adopt(doc, ids, None);
    }

    /// When the saved `file` holds the view (see [`crate::held`]), the numbering of the view's
    /// added objects in it (view number → file number).
    pub fn holds(&self, file: &Arc<Vec<u8>>) -> Option<HashMap<u32, u32>> {
        let max = base_max(self.model.pristine());
        // A locally acknowledged view stores edits in its physical base, so its
        // overlay no longer describes all differences from the replica base. Rebuild
        // that logical overlay with cloned numbering; a failed read cannot hold a file.
        let mut bases = self.model.bases().clone();
        let mut ids = self.model.ids().clone();
        let (logical, _) = read_document(&self.replica, &self.blobs, &mut bases, &mut ids, self.model.password()).ok()?;
        crate::held::match_saved(&logical, self.model.pristine(), max, file, self.model.password())
    }

    /// Takes the saved `file` (opened as `doc`) as the view, its added objects numbered as
    /// [`PdfSession::holds`] found them: the next save appends to it.
    pub fn adopt_file(&mut self, doc: Document, numbering: &HashMap<u32, u32>) {
        let mut ids = self.model.ids().clone();
        let added: Vec<(String, u32)> = ids.added().map(|(key, num)| (key.to_owned(), num)).collect();
        ids.clear_added();
        let mut next = doc.next_num();
        for (key, num) in added {
            let num = match numbering.get(&num) {
                Some(found) => *found,
                None => {
                    // Not in the view (deleted since): a number nothing in the file uses.
                    let free = next;
                    next = next.saturating_add(1);
                    free
                }
            };
            ids.bind(&key, num);
        }
        let mut doc = doc;
        doc.raise_next_num(ids.next_free(next));
        self.model.adopt(doc, ids, None);
    }

    /// Undoes this replica's last step of the person (never an import or a repair), repairs the
    /// result and reloads the view. Answers whether there was one, and the damage.
    pub fn undo(&mut self) -> Result<(bool, Damage)> {
        self.undo_in(UndoScope::Person)
    }

    pub fn redo(&mut self) -> Result<(bool, Damage)> {
        self.redo_in(UndoScope::Person)
    }

    /// [`PdfSession::undo`] in `scope` (KitModel M12a): the person's and the agents' steps on this
    /// replica are separate histories.
    pub fn undo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        self.step_in(scope, true).map(|(_, done, damage)| (done, damage))
    }

    pub fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        self.step_in(scope, false).map(|(_, done, damage)| (done, damage))
    }

    /// One undo (`undo`) or redo in `scope`, answering how many steps it took off its stack (the
    /// steps the replica passed over, whose objects are gone since, and the one taken back:
    /// `UndoOutcome::steps`), whether one was taken back, and the damage. A step passed over
    /// leaves the stack for good; what it replayed joins the step taken back, one step on the
    /// other stack.
    pub fn step_in(&mut self, scope: UndoScope, undo: bool) -> Result<(usize, bool, Damage)> {
        let can = if undo { self.can_undo_in(scope) } else { self.can_redo_in(scope) };
        if !can {
            return Ok((0, false, Damage::None));
        }
        let outcome = if undo { self.replica.undo_in(scope) } else { self.replica.redo_in(scope) }.map_err(kit_err)?;
        let (steps, done) = (outcome.steps(), outcome.is_done());
        let (damage, moved) = self.settle();
        if steps > 0 {
            self.log_added(moved.added.iter().cloned());
            self.index.stepped(moved, steps, done, undo);
        }
        self.index.collect_garbage(&mut self.blobs);
        Ok((steps, done, damage?))
    }

    /// Whether the person has an undo step to take: one of this replica's, not trimmed by
    /// [`PdfSession::retain_steps`].
    pub fn can_undo(&self) -> bool {
        self.can_undo_in(UndoScope::Person)
    }

    pub fn can_redo(&self) -> bool {
        self.can_redo_in(UndoScope::Person)
    }

    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_undo_in(scope) && self.index.undo_steps() > 0
    }

    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_redo_in(scope) && self.index.redo_steps() > 0
    }

    /// Keeps only the newest `undo` undo steps and the next `redo` redo steps reachable (the
    /// engine history's own) and drops the blobs only older steps needed: the replica has no
    /// limit of its own, so this is what keeps a long session's bases and streams bounded.
    pub fn retain_steps(&mut self, undo: usize, redo: usize) {
        self.index.retain(undo, redo);
        self.index.collect_garbage(&mut self.blobs);
        let blobs = &self.blobs;
        self.blob_log.retain(|(_, hashes)| hashes.iter().any(|hash| blobs.0.contains_key(hash)));
    }

    /// Undo and redo steps reachable, as [`PdfSession::retain_steps`] left them.
    pub fn steps(&self) -> (usize, usize) {
        (self.index.undo_steps(), self.index.redo_steps())
    }

    /// Imports remote or stored changes (their blobs added first, [`PdfSession::add_blobs`]),
    /// repairs the merged state and reloads the view. An import whose repair or view load fails
    /// has still imported its changes: [`PdfSession::view_loaded`] tells whether the view
    /// followed.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        self.replica.import_changes(bytes).map_err(kit_err)?;
        let (damage, moved) = self.settle();
        self.log_added(moved.added.iter().cloned());
        self.index.imported();
        self.index.collect_garbage(&mut self.blobs);
        damage
    }

    /// Takes a draft's changes (made on a fork of this replica) in as one transaction of `origin`
    /// (`Replica::merge_changes_repaired`, the kit's repair inside it): one undo step of the
    /// person's unless `origin` is a `system` or `import` one, even for an agent's draft (M12a).
    /// Reloads the view as an import does; add the draft's blobs first
    /// ([`PdfSession::add_blobs`]). Answers the undo scope the step went to (`None`: no step) and
    /// the damage. A refused merge changes nothing.
    ///
    /// Refused too, before the KDM's rules are asked, is a draft whose edits the document could
    /// not keep ([`PdfSession::merged_state`]): one made against an older base than the
    /// document's (a full rewrite since: applying redactions, sanitizing, changing security,
    /// signing), and one that rewrote the file in full while the document was edited. The overlay
    /// entries either brings are of another base than the merged one, and the repair would delete
    /// them without a word.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<(Option<UndoScope>, Damage)> {
        self.merged_state(bytes)?;
        let before = self.replica.version();
        let merged = self.replica.merge_changes_repaired(bytes, origin, repair).map_err(kit_err)?;
        let (steps, damage) = self.took_in(&before, origin)?;
        Ok((merged.scope.filter(|_| steps > 0), damage))
    }

    /// What merging a draft's changes would make of the replicated state, changing nothing.
    /// Refuses what [`PdfSession::merge_changes`] refuses: a draft made against an older base
    /// than the document's ([`REWRITTEN_SINCE`]), checked first, then what the KDM's rules refuse,
    /// then a merge that would hold overlay entries of another base than its own
    /// ([`REWRITE_LOSES_EDITS`]).
    pub fn merged_state(&self, bytes: &[u8]) -> Result<serde_json::Value> {
        if self.draft_base_epoch(bytes).is_some_and(|then| Some(then) != epoch_of(&self.replica)) {
            return Err(kit_err(REWRITTEN_SINCE));
        }
        let merged = self.replica.merged_state(bytes).map_err(kit_err)?;
        let stale = stale_in(&merged);
        if !stale.is_empty() {
            return Err(kit_err(format!("{REWRITE_LOSES_EDITS} (the overlay entries {})", stale.join(", "))));
        }
        Ok(merged)
    }

    /// The base (its epoch) a draft's `changes` were made against: the epoch at the version of
    /// this replica they follow. `None` when they are not changes of this replica's line, follow
    /// its current version, or follow one its log no longer holds: the KDM's merge then applies
    /// or refuses them by its own rules.
    fn draft_base_epoch(&self, changes: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(changes).ok()?;
        let seq = value.get("after")?.get("seq")?.as_u64()?;
        let lineage = value.get("lineage")?.as_str()?.replace('-', "");
        let now = self.replica.version().encode();
        let (line, at) = now.split_at_checked(16)?;
        let hex: String = line.iter().map(|byte| format!("{byte:02x}")).collect();
        if !hex.eq_ignore_ascii_case(&lineage) || seq >= u64::from_be_bytes(at.try_into().ok()?) {
            return None;
        }
        let version = Version::decode(&[line, &seq.to_be_bytes()].concat()).ok()?;
        epoch_of(&self.replica.fork_at(&version, ActorId::random()).ok()?)
    }

    /// Writes the state `snapshot` holds (a snapshot or the content of this document, as a host's
    /// checkpoint keeps it) over the replica as one transaction of `origin`, by object id
    /// (`kition_kit::session::write_state`), so every object the two share keeps its id: one undo
    /// step of its scope unless `origin` is a `system` or `import` one. Nothing changes when the
    /// states are equal. Refuses another document's snapshot. Add the blobs it references first.
    /// The kit's repair runs inside the same transaction. Answers the undo scope the step went to
    /// (`None`: no step) and the damage.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<(Option<UndoScope>, Damage)> {
        let schema = PdfModel::schema();
        let loaded = Replica::load(Arc::clone(&schema), snapshot, ActorId::random()).map_err(kit_err)?;
        if loaded.doc_id() != self.replica.doc_id() {
            return Err(kit_err(KdmError::WrongDocument { expected: self.replica.doc_id().to_string(), found: loaded.doc_id().to_string() }));
        }
        let (now, then) = (kition_kit::session::state(&self.replica)?, kition_kit::session::state(&loaded)?);
        if now == then {
            return Ok((None, Damage::None));
        }
        let before = self.replica.version();
        let result = self.replica.transact(origin, |tx| {
            let written = kition_kit::session::write_state(tx, &schema, &now, &then).map_err(crate::kdm_err)?;
            // The repair reads only the root (its epoch and overlay): asked about it, it finds
            // whatever the restore wrote there.
            repair(tx, &BTreeSet::from([crate::root_id()])).map(|()| written)
        });
        if let Err(error) = result {
            self.replica.take_changed();
            return Err(kit_err(error));
        }
        let (steps, damage) = self.took_in(&before, origin)?;
        Ok(((steps > 0).then(|| UndoScope::of(origin.kind)).flatten(), damage))
    }

    /// After a merge or a restore wrote one transaction of `origin` since `before`: repairs and
    /// reloads the view as an import does, and counts the transaction as an undo step unless
    /// `origin` is a `system` or `import` one. A failed repair or view load is the error, as an
    /// import's is; the transaction stays ([`PdfSession::view_loaded`] tells whether the view
    /// followed).
    fn took_in(&mut self, before: &Version, origin: &Origin) -> Result<(u32, Damage)> {
        let wrote = self.replica.version() != *before;
        let (damage, moved) = self.settle();
        self.log_added(moved.added.iter().cloned());
        let undoable = wrote && !matches!(origin.kind, OriginKind::System | OriginKind::Import);
        if undoable {
            self.index.push_step(moved);
        }
        self.index.imported();
        self.index.collect_garbage(&mut self.blobs);
        damage.map(|damage| (u32::from(undoable), damage))
    }

    /// Whether the replica as it is now reads as a view (its base present and opening).
    pub fn view_loaded(&self) -> bool {
        let mut bases = self.model.bases().clone();
        let mut ids = self.model.ids().clone();
        read_document(&self.replica, &self.blobs, &mut bases, &mut ids, self.model.password()).is_ok()
    }

    /// The blobs a peer at `since` needs to reach this session's current state (Kition Photo's
    /// `blobs_since`): the hashes logged for a version `since` does not include, still in the
    /// store. An empty `since` answers every blob the current state references (a creating
    /// transaction or a snapshot is never logged).
    pub fn blobs_since(&self, since: &Version) -> Vec<(String, Arc<[u8]>)> {
        if *since == Version::empty() {
            return self.index.referenced().into_iter().filter_map(|hash| self.blobs.0.get(&hash).cloned().map(|bytes| (hash, bytes))).collect();
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (version, hashes) in &self.blob_log {
            if since.includes(version) {
                continue;
            }
            for hash in hashes {
                if seen.insert(hash.clone())
                    && let Some(bytes) = self.blobs.0.get(hash)
                {
                    out.push((hash.clone(), bytes.clone()));
                }
            }
        }
        out
    }

    fn log_added(&mut self, added: impl IntoIterator<Item = String>) {
        let added: Vec<String> = added.into_iter().collect();
        if !added.is_empty() {
            self.blob_log.push((self.replica.version(), added));
        }
    }

    pub fn export_changes(&self, since: &Version) -> Result<Vec<u8>> {
        self.replica.export_changes(since).map_err(kit_err)
    }

    pub fn version(&self) -> Version {
        self.replica.version()
    }

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.replica.snapshot().map_err(kit_err)
    }

    pub fn state_hash(&self) -> Result<String> {
        self.replica.state_hash().map_err(kit_err)
    }

    pub fn doc_id(&self) -> DocId {
        self.replica.doc_id()
    }

    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    /// The blobs the replica references or an undo, redo or pending import can still need.
    pub fn blobs(&self) -> &MemoryBlobs {
        &self.blobs
    }

    /// Adds blobs (from a peer, before importing its changes): kept until the next import.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) {
        for (hash, bytes) in &blobs.0 {
            self.blobs.0.entry(hash.clone()).or_insert_with(|| bytes.clone());
        }
        self.index.protect(blobs.0.keys().cloned());
    }

    /// As `KitSession`'s: repairs what changed in a `system` transaction (not an undo step), then
    /// reloads the view; the view follows the replica even if the repair fails. Counts the stale
    /// entries the repair drops, and answers which blobs moved (the index is updated either way).
    fn settle(&mut self) -> (Result<Damage>, Moved) {
        let mut changed = self.replica.take_changed();
        let repaired = if changed.is_empty() {
            Ok(())
        } else {
            let stale = stale_entries(&self.replica).map(|keys| keys.len()).unwrap_or(0);
            let mut kit_error = None;
            let result = self.replica.transact(&Origin::system("repair"), |tx: &mut Tx<'_>| {
                PdfModel::validate(tx, &changed).map(|_| ()).map_err(|error| {
                    let message = error.to_string();
                    kit_error = Some(error);
                    KdmError::Engine(message)
                })
            });
            if result.is_ok() {
                self.dropped = self.dropped.saturating_add(stale as u64);
            }
            match (result, kit_error) {
                (_, Some(error)) => Err(error),
                (result, None) => result.map_err(kit_err),
            }
        };
        changed.extend(self.replica.take_changed());
        let moved = self.index.update(&self.replica, &changed);
        let damage = self.refresh(&changed).and_then(|damage| repaired.map(|()| damage));
        // The typed objects follow the merged view: two replicas' edits each kept them in step
        // with their own view, and the merge of both may differ from either (one replica's
        // projection of a page another deleted). Every replica writes the same, so they settle.
        if damage.is_ok() {
            let (doc, ids) = (self.model.doc.clone(), self.model.ids().clone());
            if crate::projection::differs(&self.replica, &doc, &ids) {
                let _ = self.replica.transact(&Origin::system("repair"), |tx: &mut Tx<'_>| crate::projection::sync(tx, &doc, &ids));
                self.replica.take_changed();
            }
        }
        (damage, moved)
    }

    fn refresh(&mut self, changed: &BTreeSet<ObjectId>) -> Result<Damage> {
        if changed.is_empty() {
            return Ok(Damage::None);
        }
        let blobs: &dyn BlobSource = &self.blobs;
        self.model.apply(&self.replica, changed, blobs)
    }
}

/// Why a draft made against an older base than the document's is refused
/// ([`PdfSession::merged_state`]); the editor shows it.
pub const REWRITTEN_SINCE: &str = "the document was rewritten since this draft was made (applying redactions, sanitizing, changing its security or signing renumbers every object), so the draft's edits no longer fit it; make the draft again on the document as it is now";

/// Why a draft that rewrote the file in full while the document was edited is refused.
pub const REWRITE_LOSES_EDITS: &str = "the draft rewrites the whole file and the document was edited since the draft was made: merging would lose those edits; make the draft again on the document as it is now";

/// The kit's repair of a merged draft or a restored state, inside that change and its undo step
/// (data model 6.6, D9): one Undo gives back the exact state before it.
fn repair(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> kition_kit::kdm::Result<()> {
    crate::validate::validate(tx, changed).map(|_| ())
}

/// The epoch (which base the overlay is written against) of the replica's root, if it has one.
fn epoch_of(replica: &Replica) -> Option<String> {
    replica.get(&crate::root_id(), "epoch").ok()?.as_str().map(str::to_owned)
}

/// The overlay keys of `state` (a replicated state as JSON) whose entries were written against
/// another base than the state's own: the ones the repair ([`crate::validate`]) deletes.
fn stale_in(state: &serde_json::Value) -> Vec<String> {
    let Some(pdf) = state.get("objects").and_then(|objects| objects.get(crate::root_id().as_str())) else { return Vec::new() };
    let epoch = pdf.get("epoch").and_then(serde_json::Value::as_str);
    let Some(overlay) = pdf.get("overlay").and_then(serde_json::Value::as_object) else { return Vec::new() };
    overlay.iter().filter(|(_, entry)| entry.get("epoch").and_then(serde_json::Value::as_str) != epoch).map(|(key, _)| key.clone()).collect()
}

/// The hash of the replica's current base blob (empty if unreadable).
fn blobs_hash(replica: &Replica) -> String {
    crate::read::base_of(replica).map(|base| base.blob.hash).unwrap_or_default()
}
