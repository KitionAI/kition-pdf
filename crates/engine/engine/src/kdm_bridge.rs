//! The command bridge to each document's KDM replica (Kition phase 3g), the same design as
//! Kition Slide's, Photo's and Film's.
//!
//! A document's replica ([`kdm_model::PdfSession`], made lazily on the first edit or when a host
//! asks for it) is its shared source of truth; the editor's COS document is the engine's own view
//! of it. Every edit runs exactly as before ([`Session::apply`]); when it has succeeded, the
//! bridge writes what it changed as one transaction: the overlay entries it changed, or a rebase
//! for an edit that rewrites the file in full (applying redactions, changing security,
//! sanitizing), which [`Session::apply`] now writes and reopens at once ([`rewritten`]) so that
//! the replica, the view and the next save agree on one base.
//!
//! Undo is per person: each history step remembers how many replica undo steps it took
//! ([`Link::undo_steps`], beside the editor's snapshots); undo and redo step the replica that many
//! times and take the replica's view (with every collaborator's edit since) as the document,
//! not the engine's own snapshot. A step the replica holds nothing of (made before a reset) takes
//! the snapshot and writes it as a `system` change, so the replica always follows the document.
//!
//! Direct writes: saving incrementally ([`Session::mark_saved`]) keeps the replica's base (the
//! saved file is the base with the view's edits appended, the same objects under the same
//! numbers), so collaborators get nothing and an undo past the save still works; a save that
//! renumbered (not an extension of the base) is a `system` rebase, after which undo and redo step
//! back by the history's snapshots (the replica steps were written against the replaced base). A
//! signature ([`Session::mark_signed`]) is always a `system` rebase: every peer needs the signed
//! bytes.
//! Reverting writes the difference as a `system` change and clears the history, as the engine
//! does. Layer and hide-comments toggles, the renderer and the canvas backdrop are this person's
//! own: never written.
//!
//! A replica that refuses a write or an undo is left ([`Session::kdm_resets`]): the document keeps
//! working from its own history snapshots, and the next edit makes a fresh replica. Changes reach
//! a replica in one order (an import that does not follow on is refused), and a draft made
//! against an older base than the document's is refused ([`Session::merge_changes`]). Only one
//! scope's undo (or redo) of a full rewrite under the other scope's later edits leaves edits of
//! another base, which the repair drops and counts ([`Session::kdm_dropped`]).

use std::sync::Arc;

use kdm_model::PdfSession;
use kdm_model::kition_kit::kdm::{Origin, OriginKind, UndoScope, Version};
use kdm_model::kition_kit::session::{Damage, MemoryBlobs};
use kition_pdf_cos::{SaveOptions, write_full};
use kition_pdf_render::inspect;

use crate::{DocId, Document, EditError, Session};

/// The origin of the local person's edits (used when nothing else was set with
/// [`Session::apply_as`]/[`Session::set_origin`]).
pub fn local_person() -> Origin {
    Origin::person("person:local")
}

fn replica_error(error: impl std::fmt::Display) -> EditError {
    EditError::Replica(error.to_string())
}

/// `next`, an edit's result that must be written in full (redaction, security, sanitizing), as
/// that full rewrite opened again (with `password`, the strongest the new state knows): a new
/// base the replica can share. Stamped with `mod_date`, as a save would.
pub(crate) fn rewritten(
    next: &kition_pdf_cos::Document,
    mod_date: Option<String>,
    password: Option<&str>,
) -> Result<kition_pdf_cos::Document, EditError> {
    let opts = SaveOptions { mod_date, ..SaveOptions::default() };
    let bytes = crate::guard(|| write_full(next, &opts)).map_err(EditError::Write)?.map_err(|e| EditError::Write(e.to_string()))?;
    kition_pdf_cos::Document::open_with_password(Arc::new(bytes), password).map_err(|e| EditError::Reopen(e.to_string()))
}

/// One document's bridge state.
#[derive(Debug, Default)]
pub struct Link {
    /// The replica, made lazily; `None` again after a reset.
    pub(crate) kdm: Option<PdfSession>,
    /// The editor's document as last written to (or read from) the replica: what a lazily made
    /// replica starts from.
    pub(crate) synced: Option<kition_pdf_cos::Document>,
    pub(crate) resets: u64,
    /// Entries dropped by replicas this document has left.
    pub(crate) dropped_before: u64,
    /// The replica undo steps each editor undo step took, oldest first (beside `Editor::undo`).
    pub(crate) undo_steps: Vec<u32>,
    /// The same for the redo steps (beside `Editor::redo`).
    pub(crate) redo_steps: Vec<u32>,
}

impl Link {
    pub(crate) fn new(cos: Option<&kition_pdf_cos::Document>) -> Self {
        Self { synced: cos.cloned(), ..Self::default() }
    }

    /// The replica, made now from what it last saw if there is none yet.
    pub(crate) fn ensure(&mut self, password: Option<&str>) -> Option<&mut PdfSession> {
        if self.kdm.is_none() {
            let base = self.synced.as_ref()?;
            match PdfSession::create(base, password) {
                Ok(kdm) => self.kdm = Some(kdm),
                Err(error) => {
                    log::warn!("the document can't be held in a replica: {error}");
                    return None;
                }
            }
        }
        if let Some(kdm) = self.kdm.as_mut() {
            kdm.set_password(password.map(str::to_owned));
        }
        self.kdm.as_mut()
    }

    /// Writes ids into the pages `next` made and, with `all`, into every page and comment
    /// without one ([`kdm_model::projection::stamp`]), each as the replica names it now.
    pub(crate) fn stamp(&self, next: &mut kition_pdf_cos::Document, before: &kition_pdf_cos::Document, all: bool) {
        match self.kdm.as_ref() {
            Some(kdm) => kdm_model::projection::stamp(next, before, all, &|num| kdm.key_of(num)),
            None => kdm_model::projection::stamp(next, before, all, &|num| num.to_string()),
        }
    }

    /// The typed objects of the document ([`kdm_model::projection`]), as its replica names them.
    pub(crate) fn projection(&self, cos: &kition_pdf_cos::Document) -> kdm_model::projection::Projection {
        match self.kdm.as_ref() {
            Some(kdm) => kdm_model::projection::project_keyed(cos, &|num| kdm.key_of(num)),
            None => kdm_model::projection::project(cos),
        }
    }

    /// Writes `after` on top of the replica (made now if there is none) as `origin`, and answers
    /// the undo steps that took (0 when the replica refused and was left).
    pub(crate) fn record(&mut self, origin: &Origin, after: &kition_pdf_cos::Document, password: Option<&str>) -> u32 {
        let Some(kdm) = self.ensure(password) else {
            self.synced = Some(after.clone());
            return 0;
        };
        match kdm.record(origin, after) {
            Ok(steps) => {
                self.synced = Some(after.clone());
                steps
            }
            Err(error) => {
                log::warn!("the document's replica refused an edit: {error}");
                self.detach(after);
                0
            }
        }
    }

    /// An edit pushed one editor undo step (the history now `undo_len` long, the oldest dropped
    /// past the limit) that took `steps` replica steps.
    pub(crate) fn pushed(&mut self, steps: u32, undo_len: usize) {
        self.undo_steps.push(steps);
        while self.undo_steps.len() > undo_len {
            self.undo_steps.remove(0);
        }
        self.trim();
    }

    /// The editor kept the redo steps `keep` marks (a new step clears only its own scope's).
    pub(crate) fn keep_redo(&mut self, keep: &[bool]) {
        let mut marks = keep.iter();
        self.redo_steps.retain(|_| marks.next().copied().unwrap_or(false));
    }

    /// A step under the other scope's later ones: the replica undoes (or redoes) its `steps`
    /// transactions of `scope` and answers its view, with the other scope's later edits and every
    /// collaborator's. `None`, changing nothing, when the replica does not hold them (then it is
    /// left only if it refused midway). The caller moves the step, then trims.
    pub(crate) fn replay(
        &mut self,
        steps: u32,
        undo: bool,
        scope: UndoScope,
        current: &kition_pdf_cos::Document,
    ) -> Option<(kition_pdf_cos::Document, u32)> {
        let kdm = self.kdm.as_mut()?;
        let (available, can) = if undo { (kdm.steps().0, kdm.can_undo_in(scope)) } else { (kdm.steps().1, kdm.can_redo_in(scope)) };
        if steps == 0 || !can || usize::try_from(steps).unwrap_or(usize::MAX) > available {
            return None;
        }
        let Some(moved) = take_steps(kdm, steps, undo, scope) else {
            self.detach(current);
            return None;
        };
        let view = kdm.document().clone();
        self.synced = Some(view.clone());
        Some((view, moved))
    }

    /// The editor's history was cleared (a revert, a signature).
    pub(crate) fn cleared(&mut self) {
        self.undo_steps.clear();
        self.redo_steps.clear();
        self.trim();
    }

    /// Leaves the replica: every step's replica steps were the dropped replica's, and a fresh
    /// one never holds them.
    pub(crate) fn detach(&mut self, current: &kition_pdf_cos::Document) {
        if let Some(kdm) = self.kdm.take() {
            self.dropped_before = self.dropped_before.saturating_add(kdm.dropped());
        }
        self.resets = self.resets.saturating_add(1);
        self.synced = Some(current.clone());
        self.undo_steps.iter_mut().chain(self.redo_steps.iter_mut()).for_each(|s| *s = 0);
    }

    /// Lets the replica forget the steps the history can no longer take, and their blobs.
    pub(crate) fn trim(&mut self) {
        let undo = self.undo_steps.iter().map(|s| *s as usize).sum();
        let redo = self.redo_steps.iter().map(|s| *s as usize).sum();
        if let Some(kdm) = self.kdm.as_mut() {
            kdm.retain_steps(undo, redo);
        }
    }

    /// Steps the replica `steps` times back (`undo`) or forward and answers its view; `None`
    /// when there is no replica, the step holds nothing of it, or it refused (then it is left).
    ///
    /// Answers the view and how many replica steps the history step now holds on the other stack:
    /// fewer than `steps` when the replica passed over some (their objects gone since), whose
    /// replays joined the ones it took back.
    pub(crate) fn step(
        &mut self,
        steps: u32,
        undo: bool,
        scope: UndoScope,
        current: &kition_pdf_cos::Document,
    ) -> Option<(kition_pdf_cos::Document, u32)> {
        if steps == 0 {
            return None;
        }
        let kdm = self.kdm.as_mut()?;
        let Some(moved) = take_steps(kdm, steps, undo, scope) else {
            self.detach(current);
            return None;
        };
        let view = kdm.document().clone();
        self.synced = Some(view.clone());
        Some((view, moved))
    }

    /// A save of the document to `saved` (the editor's document now): one appended to the base
    /// (`extends`) becomes the view without writing anything; another is a `system` rebase.
    pub(crate) fn saved(&mut self, saved: &kition_pdf_cos::Document, extends: bool, password: Option<&str>) {
        match self.kdm.as_mut() {
            None => self.synced = Some(saved.clone()),
            Some(kdm) if extends => {
                kdm.adopt_local(saved.clone());
                self.synced = Some(saved.clone());
            }
            Some(_) => {
                self.record(&Origin::system("save"), saved, password);
                // The rebase replaced the base this person's replica steps were written against:
                // stepping them back changes nothing the document shows, so the history steps
                // back by its own snapshots from here (as after a reset).
                self.undo_steps.iter_mut().chain(self.redo_steps.iter_mut()).for_each(|s| *s = 0);
            }
        }
    }

    /// A signed file saved (the editor's document now): a `system` rebase onto its bytes.
    pub(crate) fn signed(&mut self, signed: &kition_pdf_cos::Document, password: Option<&str>) {
        if let Some(kdm) = self.ensure(password) {
            if let Err(error) = kdm.rebase(&Origin::system("sign"), signed) {
                log::warn!("the document's replica refused the signed file: {error}");
                self.detach(signed);
                return;
            }
            self.synced = Some(signed.clone());
        } else {
            self.synced = Some(signed.clone());
        }
    }

    pub(crate) fn dropped(&self) -> u64 {
        self.dropped_before.saturating_add(self.kdm.as_ref().map_or(0, PdfSession::dropped))
    }
}

/// Takes `steps` of the replica's steps of `scope` off its undo (`undo`) or redo stack, one undo or
/// redo at a time; answers how many steps they made on the other stack (a step the replica passed
/// over joins the next one it takes back). `None` when the replica refused, ran out, or took more
/// than `steps` (a step of an older history step): the caller leaves it.
fn take_steps(kdm: &mut PdfSession, steps: u32, undo: bool, scope: UndoScope) -> Option<u32> {
    let wanted = usize::try_from(steps).ok()?;
    let (mut consumed, mut moved) = (0usize, 0u32);
    while consumed < wanted {
        let (taken, done, _) = kdm.step_in(scope, undo).ok()?;
        if taken == 0 {
            return None;
        }
        consumed += taken;
        moved += u32::from(done);
    }
    (consumed == wanted).then_some(moved)
}

impl Document {
    /// The document's replica, if it has made one.
    pub fn kdm(&self) -> Option<&PdfSession> {
        self.link.kdm.as_ref()
    }

    /// How many times the document left its replica (a write or undo the replica refused).
    pub fn kdm_resets(&self) -> u64 {
        self.link.resets
    }

    /// How many edits (overlay entries) were dropped because an undo or redo brought back another
    /// base than the one they were made against. See [`kdm_model::PdfSession::dropped`].
    pub fn kdm_dropped(&self) -> u64 {
        self.link.dropped()
    }

    /// The password the replica opens protected bases with: the strongest this state knows.
    pub(crate) fn replica_password(&self) -> Option<String> {
        self.editor.as_ref().and_then(|e| e.keys.reopen.clone().or_else(|| e.keys.render.clone())).or_else(|| self.password.clone())
    }

    /// After an import: the editor's document follows the replica. The import is the change
    /// that made the content, applied `now` (the session clock's): its time is the `/ModDate` a
    /// save stamps.
    fn follow_kdm(&mut self, now: Option<i64>) -> Result<(), EditError> {
        let Some(view) = self.link.kdm.as_ref().map(|kdm| kdm.document().clone()) else { return Ok(()) };
        if let Some(editor) = self.editor.as_mut() {
            editor.cos = view.clone();
            editor.edited = now;
        }
        self.link.synced = Some(view);
        self.dirty = true;
        self.generation += 1;
        Session::refresh(self)
    }
}

impl Session {
    /// Who the running (or next) edit acts for: the origin of [`Session::apply_as`], or the local
    /// person.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(local_person)
    }

    /// Sets who the following edits act for (`None`: the local person); answers the previous
    /// setting, to put back.
    pub fn set_origin(&mut self, origin: Option<Origin>) -> Option<Origin> {
        std::mem::replace(&mut self.origin, origin)
    }

    /// [`Session::apply`] as `origin` (a person, an agent): the replica records the edit as
    /// theirs, and it undoes as this replica's own step.
    pub fn apply_as(&mut self, id: DocId, edit: crate::Edit, origin: &Origin) -> Result<(), EditError> {
        let previous = self.set_origin(Some(origin.clone()));
        let result = self.apply(id, edit);
        self.set_origin(previous);
        result
    }

    /// [`Session::undo`] as `origin`: in its undo scope (an agent's undo takes back only the
    /// agents' steps; engine rule E14a).
    pub fn undo_as(&mut self, id: DocId, origin: &Origin) -> Result<String, EditError> {
        let previous = self.set_origin(Some(origin.clone()));
        let result = self.undo(id);
        self.set_origin(previous);
        result
    }

    pub fn redo_as(&mut self, id: DocId, origin: &Origin) -> Result<String, EditError> {
        let previous = self.set_origin(Some(origin.clone()));
        let result = self.redo(id);
        self.set_origin(previous);
        result
    }

    /// The document's replica, if it has made one (never makes one).
    pub fn kdm(&self, id: DocId) -> Option<&PdfSession> {
        self.get(id)?.kdm()
    }

    /// How many times the document left its replica. Hosts watch this: the next edit's fresh
    /// replica is a new KDM document collaborators must join from its snapshot.
    pub fn kdm_resets(&self, id: DocId) -> u64 {
        self.get(id).map_or(0, Document::kdm_resets)
    }

    /// How many edits (overlay entries) were dropped because one scope's undo or redo of a full
    /// rewrite brought back another base than the one they were made against (the person undoes
    /// a sanitize after an agent's comment). The editor shows it.
    pub fn kdm_dropped(&self, id: DocId) -> u64 {
        self.get(id).map_or(0, Document::kdm_dropped)
    }

    /// The document's KDM identity, `None` before it has made a replica.
    pub fn doc_id(&self, id: DocId) -> Option<String> {
        Some(self.kdm(id)?.doc_id().to_string())
    }

    fn active_kdm(&mut self, id: DocId) -> Result<&mut PdfSession, EditError> {
        let doc = self.doc_mut(id)?;
        if doc.editor.is_none() {
            return Err(EditError::ReadOnly(doc.read_only_reason.clone().unwrap_or_default()));
        }
        let password = doc.replica_password();
        doc.link.ensure(password.as_deref()).ok_or_else(|| replica_error("the document has no replica and none could be made"))
    }

    /// The document's replica version.
    pub fn version(&mut self, id: DocId) -> Result<Version, EditError> {
        Ok(self.active_kdm(id)?.version())
    }

    /// The document's replica as a snapshot, for a peer ([`Session::open_snapshot`]).
    pub fn snapshot(&mut self, id: DocId) -> Result<Vec<u8>, EditError> {
        self.active_kdm(id)?.snapshot().map_err(replica_error)
    }

    /// SHA-256 of the document's replicated state: equal on replicas that converged.
    pub fn state_hash(&mut self, id: DocId) -> Result<String, EditError> {
        self.active_kdm(id)?.state_hash().map_err(replica_error)
    }

    /// The document's changes since `since` (`Version::empty()` for all of them).
    pub fn export_changes(&mut self, id: DocId, since: &Version) -> Result<Vec<u8>, EditError> {
        self.active_kdm(id)?.export_changes(since).map_err(replica_error)
    }

    /// Every blob the document's replica references or can still bring back (the base PDF and
    /// stream data).
    pub fn blobs(&mut self, id: DocId) -> Result<MemoryBlobs, EditError> {
        Ok(self.active_kdm(id)?.blobs().clone())
    }

    /// Adds a peer's blobs (before importing its changes): kept until the next import.
    pub fn add_blobs(&mut self, id: DocId, blobs: &MemoryBlobs) -> Result<(), EditError> {
        self.active_kdm(id)?.add_blobs(blobs);
        Ok(())
    }

    /// The blobs a peer at `since` needs (`Version::empty()`: every blob the current state
    /// references), so a live change ships only its own stream data, never the base again.
    pub fn blobs_since(&mut self, id: DocId, since: &Version) -> Result<MemoryBlobs, EditError> {
        Ok(MemoryBlobs(self.active_kdm(id)?.blobs_since(since).into_iter().collect()))
    }

    /// Imports a collaborator's (or stored) changes into the document's replica (their blobs
    /// added first, [`Session::add_blobs`]). The document follows the replica; one whose view
    /// could not be read from it leaves it (a reset) rather than diff against it later.
    pub fn import_changes(&mut self, id: DocId, bytes: &[u8]) -> Result<Damage, EditError> {
        let now = self.now();
        let kdm = self.active_kdm(id)?;
        let before = kdm.version();
        let result = kdm.import_changes(bytes);
        let changed = kdm.version() != before;
        let follow = result.is_ok() || (changed && kdm.view_loaded());
        let doc = self.doc_mut(id)?;
        if follow {
            if changed {
                doc.follow_kdm(now)?;
            }
        } else if changed && let Some(cos) = doc.editor.as_ref().map(|e| e.cos.clone()) {
            doc.link.detach(&cos);
        }
        doc.link.trim();
        result.map_err(replica_error)
    }

    /// Takes a draft's changes (made on a replica forked from this one, `mergeChanges`) into the
    /// document as one undo step of the person who accepts it, even when an agent made the draft
    /// (KitModel M12a: the KDM records it in the person's history, and so does the document's):
    /// the document follows the replica, and the person's undo takes the whole draft back. Add the
    /// draft's blobs first ([`Session::add_blobs`]). A draft made against an older base than the
    /// document's (a full rewrite since) is refused, saying so ([`kdm_model::PdfSession::merged_state`]).
    /// A refused merge changes nothing.
    pub fn merge_changes(&mut self, id: DocId, bytes: &[u8], origin: &Origin) -> Result<Damage, EditError> {
        self.take_in(id, UndoScope::Person, "Merge Draft", |kdm| kdm.merge_changes(bytes, origin))
    }

    /// What merging a draft's changes would change in the document now (a `StateDiff`);
    /// nothing changes. Refuses what [`Session::merge_changes`] refuses.
    pub fn compare_changes(&mut self, id: DocId, bytes: &[u8]) -> Result<kdm_model::kition_kit::kdm::diff::StateDiff, EditError> {
        let kdm = self.active_kdm(id)?;
        let before = kdm.replica().state_json().map_err(replica_error)?;
        let after = kdm.merged_state(bytes).map_err(replica_error)?;
        Ok(kdm_model::kition_kit::kdm::diff::diff_states(&before, &after))
    }

    /// Writes the state a snapshot holds (a checkpoint of this document, `replica/restore`) over
    /// the document by object id, as one undo step of `origin`'s scope (the replica records the
    /// restore as `origin`'s transaction), taken in as a merged draft is ([`Session::merge_changes`]). Add the blobs it references first. Nothing changes when the
    /// states are equal; another document's snapshot is refused.
    pub fn restore_state(&mut self, id: DocId, snapshot: &[u8], origin: &Origin) -> Result<Damage, EditError> {
        let history = UndoScope::of(origin.kind).unwrap_or(UndoScope::Person);
        self.take_in(id, history, "Restore Version", |kdm| kdm.restore_state(snapshot, origin))
    }

    /// One transaction `write` makes on the document's replica, taken into the document as a
    /// command's step: the document follows the replica, and its history gets the step (whose
    /// undo steps the replica back) when the transaction is one, in `history`: the scope the
    /// replica recorded it in. A merged draft is the person's whatever its origin (the KDM records
    /// it in the person's history, `Replica::merge_changes`, M12a), so the document's history must
    /// put it there too, or the person's Undo would not find it and an agent's would step the
    /// replica's person history.
    fn take_in(
        &mut self,
        id: DocId,
        history: UndoScope,
        label: &str,
        write: impl FnOnce(&mut PdfSession) -> kdm_model::kition_kit::session::Result<(u32, Damage)>,
    ) -> Result<Damage, EditError> {
        let now = self.now();
        let kdm = self.active_kdm(id)?;
        let before = kdm.version();
        let result = write(&mut *kdm);
        let changed = kdm.version() != before;
        let follow = changed && (result.is_ok() || kdm.view_loaded());
        let doc = self.doc_mut(id)?;
        if follow {
            let steps = result.as_ref().map_or(0, |(steps, _)| *steps);
            if steps > 0
                && let Some(editor) = doc.editor.as_mut()
            {
                let entry = (label.to_owned(), editor.cos.clone(), editor.keys.clone(), crate::Scope::Full, history, editor.edited);
                editor.undo.push(entry);
                if editor.undo.len() > crate::MAX_UNDO {
                    editor.undo.remove(0);
                }
                // A new step clears its own history's redo steps, never the other scope's.
                let keep: Vec<bool> = editor.redo.iter().map(|u| u.4 != history).collect();
                editor.redo.retain(|u| u.4 != history);
                let len = editor.undo.len();
                doc.link.keep_redo(&keep);
                doc.link.pushed(steps, len);
            }
            doc.follow_kdm(now)?;
        } else if changed && let Some(cos) = doc.editor.as_ref().map(|e| e.cos.clone()) {
            doc.link.detach(&cos);
        }
        doc.link.trim();
        result.map(|(_, damage)| damage).map_err(replica_error)
    }

    /// Opens another replica's snapshot (with the blobs it references) as a new document, a
    /// replica of the same KDM document under a fresh actor: clean, with an empty history.
    pub fn open_snapshot(&mut self, name: impl Into<String>, bytes: &[u8], blobs: MemoryBlobs, password: Option<&str>) -> Result<DocId, EditError> {
        let kdm = PdfSession::open_snapshot(bytes, blobs, password).map_err(replica_error)?;
        let cos = kdm.document().clone();
        let id = self.open_cos(name.into(), cos, password)?;
        let doc = self.doc_mut(id)?;
        doc.link.synced = doc.editor.as_ref().map(|e| e.cos.clone());
        doc.link.kdm = Some(kdm);
        Ok(id)
    }

    /// [`Session::open_snapshot`] for a host that keeps a document's replica beside its saved
    /// file. The file is what the person saved: when it is the replica's view saved (the base
    /// with the view's edits appended) it becomes the view as it is, so the next save appends to
    /// it; otherwise the file wins, recorded onto the replica as one `import` rebase (not an undo
    /// step) under the replica's own identity, so collaborators get it as ordinary changes.
    /// Empty `file`: there is no file yet, only the replica. The document opens clean either way.
    pub fn open_with_replica(
        &mut self,
        name: impl Into<String>,
        file: &[u8],
        snapshot: &[u8],
        blobs: MemoryBlobs,
        password: Option<&str>,
    ) -> Result<DocId, EditError> {
        let mut kdm = PdfSession::open_snapshot(snapshot, blobs, password).map_err(replica_error)?;
        let cos = if file.is_empty() {
            kdm.document().clone()
        } else {
            let file = Arc::new(file.to_vec());
            let opened = kition_pdf_cos::Document::open_with_password(file.clone(), password).map_err(|e| EditError::Reopen(e.to_string()))?;
            match kdm.holds(&file) {
                Some(numbering) if numbering.is_empty() && kdm_model::same_bytes(&file, kdm.document().bytes()) => kdm.document().clone(),
                Some(numbering) => {
                    kdm.adopt_file(opened, &numbering);
                    kdm.document().clone()
                }
                None => {
                    kdm.record(&Origin::system("import"), &opened).map_err(replica_error)?;
                    opened
                }
            }
        };
        let id = self.open_cos(name.into(), cos, password)?;
        let doc = self.doc_mut(id)?;
        doc.link.synced = doc.editor.as_ref().map(|e| e.cos.clone());
        doc.link.kdm = Some(kdm);
        Ok(id)
    }

    /// Reads the document's file again after another program changed it, keeping this session:
    /// its undo histories, its replica's identities and blobs (kition-sdk
    /// `KitDocument::reconcile_reread`). `file` must extend the file the clean document holds
    /// (the other program appended an incremental update, which keeps every object number): the
    /// objects and trailer entries that update changed are written onto the replica as one
    /// `import` transaction, outside both undo histories, and the file becomes the document, so
    /// the next save appends to it. A later undo steps the replica back and keeps the update.
    ///
    /// Answers `false`, changing nothing, when there is nothing to keep (a document never
    /// edited, which the host then replaces with the file read afresh) or the session cannot be
    /// kept that way: unsaved edits, a file written anew or protected, or a history step the
    /// replica does not hold (undoing it would bring back a snapshot without the update). An
    /// error also changes nothing.
    pub fn reconcile_file(&mut self, id: DocId, file: Arc<Vec<u8>>) -> Result<bool, EditError> {
        let doc = self.doc_mut(id)?;
        let Some(editor) = doc.editor.as_ref() else { return Ok(false) };
        let current = &editor.cos;
        if doc.dirty || current.is_modified() || current.security().is_some() {
            return Ok(false);
        }
        // Never edited: nothing to keep, and the file read afresh names what the update added
        // by its object numbers, as any other reader of the file does.
        if doc.generation == 0 && editor.undo.is_empty() && editor.redo.is_empty() {
            return Ok(false);
        }
        if kdm_model::same_bytes(current.bytes(), &file) {
            return Ok(true);
        }
        if file.len() <= current.bytes().len() || !file.starts_with(current.bytes()) {
            return Ok(false);
        }
        let held = match doc.link.kdm.as_ref() {
            Some(_) => {
                editor.undo.len() == doc.link.undo_steps.len()
                    && editor.redo.len() == doc.link.redo_steps.len()
                    && doc.link.undo_steps.iter().chain(&doc.link.redo_steps).all(|steps| *steps > 0)
            }
            None => editor.undo.is_empty() && editor.redo.is_empty(),
        };
        if !held {
            return Ok(false);
        }
        let opened =
            kition_pdf_cos::Document::open_with_password(file, editor.keys.reopen.as_deref()).map_err(|e| EditError::Reopen(e.to_string()))?;
        if opened.security().is_some() || !opened.revisions().starts_with(current.revisions()) {
            return Ok(false);
        }
        let Some(after) = file_difference(current, &opened) else { return Ok(false) };
        let staged = crate::Editor { cos: opened.clone(), undo: Vec::new(), redo: Vec::new(), keys: editor.keys.clone(), edited: None };
        let view = crate::PreparedView::new(doc, &staged, &doc.xfa_warnings)?;
        if let Some(kdm) = doc.link.kdm.as_mut() {
            kdm.record(&Origin::new(OriginKind::Import, "file:reread"), &after).map_err(replica_error)?;
            kdm.adopt_local(opened.clone());
        }
        doc.link.synced = Some(opened.clone());
        if let Some(editor) = doc.editor.as_mut() {
            editor.cos = opened;
            editor.edited = None;
        }
        view.install(doc);
        doc.dirty = false;
        doc.generation += 1;
        Ok(true)
    }

    /// Registers `cos` (a replica's view) as a new open document, as opening a file does.
    fn open_cos(&mut self, name: String, cos: kition_pdf_cos::Document, password: Option<&str>) -> Result<DocId, EditError> {
        let bytes = if cos.is_modified() {
            Arc::new(kition_pdf_cos::write_incremental(&cos, &SaveOptions::default()).map_err(|e| EditError::Write(e.to_string()))?)
        } else {
            cos.bytes().clone()
        };
        let info = inspect(bytes.clone(), password).map_err(|e| EditError::Reopen(e.to_string()))?;
        let xfa = kition_pdf_xfa::existing_layout(&cos);
        let modified = cos.is_modified();
        let id = self
            .push_document(name, None, bytes, info, Ok(Ok(cos)), password.map(str::to_owned), password, xfa)
            .map_err(|e| EditError::Reopen(e.to_string()))?;
        // A view with edits no file holds yet: they reach this session now, as an import's do
        // ([`Document::follow_kdm`]), and that is the `/ModDate` a save stamps.
        if modified {
            let now = self.now();
            if let Some(editor) = self.doc_mut(id)?.editor.as_mut() {
                editor.edited = now;
            }
        }
        Ok(id)
    }
}

/// `view` (a clean document) with what `file` changed in it as edits: every object the update
/// appended to `view`'s bytes wrote or freed, when its value differs (its cross-reference and
/// object streams aside), and the trailer. `None` when an object of the update cannot be read.
fn file_difference(view: &kition_pdf_cos::Document, file: &kition_pdf_cos::Document) -> Option<kition_pdf_cos::Document> {
    use std::collections::BTreeSet;

    use kition_pdf_cos::{ObjRef, Object, XrefEntry};

    let same_entry = |num: u32| {
        let entry = file.xref_entry(num);
        entry == view.xref_entry(num)
            && match entry {
                Some(XrefEntry::InStream { stream, .. }) => file.xref_entry(stream) == view.xref_entry(stream),
                _ => true,
            }
    };
    let live: BTreeSet<u32> = file.object_numbers().into_iter().collect();
    let was: BTreeSet<u32> = view.object_numbers().into_iter().collect();
    let mut after = view.clone();
    for num in live.union(&was).copied().filter(|num| !same_entry(*num)) {
        let generation = file.generation(num);
        if live.contains(&num) {
            let object = file.try_get(num).ok()?;
            // The update's own cross-reference and object streams are how it is stored, not
            // content: the next save writes its own.
            if matches!(&*object, Object::Stream(stream) if matches!(stream.dict.name(b"Type"), Some(b"XRef" | b"ObjStm"))) {
                continue;
            }
            let unchanged = was.contains(&num) && view.generation(num) == generation && view.try_get(num).ok().is_some_and(|old| old == object);
            if !unchanged {
                after.set(ObjRef { num, generation }, (*object).clone());
            }
        } else {
            after.set_freed(num, generation);
        }
    }
    *after.trailer_mut() = file.trailer().clone();
    after.raise_next_num(file.next_num());
    Some(after)
}
