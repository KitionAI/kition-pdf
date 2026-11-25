//! Bridges Kition PDF's COS document ([`kition_pdf_cos::Document`]) to a `kit.pdf` KDM replica.
//!
//! A PDF has no typed model to merge field by field: its object graph *is* the model, and an
//! edit is a set of changed indirect objects over the file as it was opened (the COS overlay).
//! The replica holds exactly that (`model/schema.json`, one `Pdf` object):
//!
//! - `base`: the file the overlay applies to, as a blob (the PDF as opened, or as last rewritten
//!   in full);
//! - `baseMax`: the base's highest object number;
//! - `epoch`: which base the overlay entries were written against;
//! - `overlay`: a map, one entry per edited object: key `"<number>"` for an object of the base,
//!   `"<actor>.<n>"` for one added since ([`Ids`]), value `{ epoch, gen, object }` (the object
//!   as [`codec`] writes it) or `{ epoch, gen, freed: true }`; and the key `"trailer"` for the
//!   trailer when an edit changed it.
//!
//! **Edits.** An edit is the overlay entries it changed ([`write_changes`]): adding a comment
//! writes the comment, its appearance stream and the page whose `/Annots` it joined. A list
//! (`/Annots`, `/Kids`, `/Fields`) is one entry, so the view lists every object added since the
//! base that nobody removed (`lists`).
//!
//! **Rebases.** An edit that rewrites the file in full (applying redactions, changing security,
//! sanitizing, repairing, a signature) renumbers every object, so it becomes a *rebase*
//! ([`rebase`]): the rewritten bytes are a new base of a new epoch, and the overlay is cleared.
//! The replica's changes apply in one order, so no edit is ever written against a base a rebase
//! replaced; the repair ([`KitModel::validate`]) still deletes an overlay entry of another epoch
//! should one appear, and counts it ([`PdfSession::dropped`]).
//!
//! **What is not replicated.** Saving incrementally appends the overlay to the file, but the
//! replica keeps its base: the saved file is the same document, and an undo past the save still
//! finds what it undoes ([`PdfSession::adopt_local`]). The renderer, the canvas backdrop and the
//! layer (optional content) and hide-comments view toggles are each person's own. Passwords are
//! never replicated: a peer opens a protected base with its own.
//!
//! Nothing is lost on a round trip: [`read_document`] gives back the document
//! [`write_document`] wrote, the base byte for byte and every edit object for object.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod blob_gc;
pub mod codec;
mod dangling;
mod held;
mod ids;
mod lists;
pub mod projection;
mod read;
mod session;
mod validate;
mod write;

use std::collections::BTreeSet;
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema, Tx};
use kition_kit::session::{BlobSink, BlobSource, Damage, KitError, KitModel};
use kition_pdf_cos::Document;

pub use ids::{Ids, base_number};
pub use read::{Bases, read_document};
pub use session::PdfSession;
pub use write::{Written, rebase, write_changes, write_document};

/// Re-exported so downstream crates (the engine command bridge) share this crate's exact pin.
pub use kition_kit;

type Result<T> = kition_kit::session::Result<T>;
pub(crate) type KdmResult<T> = std::result::Result<T, KdmError>;

const SCHEMA: &str = include_str!("../model/schema.json");

/// The MIME type of the `base` blob.
pub const PDF_MIME: &str = "application/pdf";

/// The overlay key of the trailer.
pub const TRAILER: &str = "trailer";

/// The `Pdf` object's id: one per replica, minted by [`write_document`] with
/// [`Tx::create_with_id`], so every reader finds it without an index.
pub fn root_id() -> ObjectId {
    ObjectId::from_string("pdf")
}

pub(crate) fn kit_err(error: impl std::fmt::Display) -> KitError {
    KitError::new(error.to_string())
}

pub(crate) fn kdm_err(error: impl std::fmt::Display) -> KdmError {
    KdmError::Engine(error.to_string())
}

/// Whether two documents' bytes are the same file (the same buffer, or equal bytes).
pub fn same_bytes(a: &Arc<Vec<u8>>, b: &Arc<Vec<u8>>) -> bool {
    Arc::ptr_eq(a, b) || (a.len() == b.len() && a.as_slice() == b.as_slice())
}

/// The view `doc` (over `pristine`, numbered by `ids`) as replicas compare it: the base's hash,
/// and every edited object and the trailer under its overlay key, references written as keys.
/// Equal on replicas that converged, whatever numbers each gave the objects added since the base.
pub fn canonical(doc: &Document, pristine: &Document, ids: &Ids) -> serde_json::Value {
    let mut scratch = kition_kit::session::MemoryBlobs::default();
    let mut objects = serde_json::Map::new();
    let base = kition_kit::session::sha256_hex(pristine.bytes());
    for (num, generation, object) in doc.overlay_entries() {
        let key = ids.known_key(num).unwrap_or_else(|| format!("#{num}"));
        let mut name = |n: u32| ids.known_key(n).unwrap_or_else(|| format!("#{n}"));
        let value = match object {
            Some(object) => codec::Encoder { key: &mut name, blobs: &mut scratch }.object(&object).unwrap_or_else(serde_json::Value::String),
            None => serde_json::Value::String("freed".into()),
        };
        objects.insert(key, serde_json::json!({ "gen": generation, "object": value }));
    }
    let mut name = |n: u32| ids.known_key(n).unwrap_or_else(|| format!("#{n}"));
    let trailer = if write::same_trailer(doc.trailer(), pristine.trailer()) {
        serde_json::Value::Null
    } else {
        codec::Encoder { key: &mut name, blobs: &mut scratch }
            .dict(&write::without_save_keys(doc.trailer()))
            .unwrap_or_else(serde_json::Value::String)
    };
    serde_json::json!({ "base": base, "objects": objects, "trailer": trailer })
}

/// A file's document, as [`KitModel::import`] reads it.
pub fn decode(bytes: &[u8], password: Option<&str>) -> Result<Document> {
    Document::open_with_password(Arc::new(bytes.to_vec()), password).map_err(kit_err)
}

/// The engine's view of a `kit.pdf` replica: the COS document, its base opened afresh, and the
/// numbering of objects added since the base ([`Ids`]).
pub struct PdfModel {
    /// The view: the base with the overlay applied.
    pub doc: Document,
    /// The replica's base, opened afresh (no edits): shares the view's bytes.
    pristine: Document,
    ids: Ids,
    bases: Bases,
    password: Option<String>,
}

impl std::fmt::Debug for PdfModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfModel").field("doc", &self.doc).finish_non_exhaustive()
    }
}

impl PdfModel {
    /// A view built by the caller itself: `doc` as written by [`write_document`], over
    /// `pristine`, with the ids that write left.
    pub fn from_parts(doc: Document, pristine: Document, ids: Ids, bases: Bases, password: Option<String>) -> Self {
        Self { doc, pristine, ids, bases, password }
    }

    /// Reads `replica` as a view, opening a protected base with `password`.
    pub fn read(replica: &Replica, blobs: &dyn BlobSource, password: Option<&str>) -> Result<Self> {
        let mut ids = Ids::new(replica.actor().get());
        let mut bases = Bases::default();
        let (doc, pristine) = read_document(replica, blobs, &mut bases, &mut ids, password).map_err(kit_err)?;
        Ok(Self { doc, pristine, ids, bases, password: password.map(str::to_owned) })
    }

    pub fn ids(&self) -> &Ids {
        &self.ids
    }

    /// The replica's base, opened afresh.
    pub fn pristine(&self) -> &Document {
        &self.pristine
    }

    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub fn set_password(&mut self, password: Option<String>) {
        self.password = password;
    }

    pub(crate) fn bases(&self) -> &Bases {
        &self.bases
    }

    pub(crate) fn bases_mut(&mut self) -> &mut Bases {
        &mut self.bases
    }

    /// Takes `doc` as the view after the caller wrote it (with `ids`), and `pristine` as the
    /// base when the write was a rebase.
    pub fn adopt(&mut self, doc: Document, ids: Ids, pristine: Option<Document>) {
        if let Some(pristine) = pristine {
            self.pristine = pristine;
        }
        self.doc = doc;
        self.ids = ids;
    }

    fn reload(&mut self, replica: &Replica, blobs: &dyn BlobSource) -> Result<()> {
        let mut ids = self.ids.clone();
        let (doc, pristine) = read_document(replica, blobs, &mut self.bases, &mut ids, self.password.as_deref()).map_err(kit_err)?;
        self.doc = doc;
        self.pristine = pristine;
        self.ids = ids;
        Ok(())
    }
}

impl KitModel for PdfModel {
    fn schema() -> Arc<Schema> {
        #[allow(clippy::expect_used)]
        Arc::new(Schema::parse(SCHEMA).expect("model/schema.json is a fixed, tested file"))
    }

    fn load(replica: &Replica, blobs: &dyn BlobSource) -> Result<Self> {
        Self::read(replica, blobs, None)
    }

    /// Reloads the view (the base stays open; only the overlay is read again).
    fn apply(&mut self, replica: &Replica, _changed: &BTreeSet<ObjectId>, blobs: &dyn BlobSource) -> Result<Damage> {
        self.reload(replica, blobs)?;
        Ok(Damage::Full)
    }

    fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> Result<bool> {
        validate::validate(tx, changed).map_err(kit_err)
    }

    fn import(_name: &str, bytes: &[u8], blobs: &mut dyn BlobSink, actor: ActorId) -> Result<Replica> {
        let doc = decode(bytes, None)?;
        let mut ids = Ids::new(actor.get());
        let (replica, _) = Replica::create_with(Self::schema(), DocId::new(), actor, &Origin::system("import"), |tx| {
            write_document(tx, &doc, &doc, &mut ids, blobs)
        })
        .map_err(kit_err)?;
        Ok(replica)
    }

    /// The view saved incrementally on its base (a full rewrite when the view needs one).
    fn export(&self, _replica: &Replica, _name: &str, _blobs: &dyn BlobSource) -> Result<Vec<u8>> {
        if self.doc.is_modified() || !write::same_trailer(self.doc.trailer(), self.pristine.trailer()) {
            kition_pdf_cos::write_incremental(&self.doc, &kition_pdf_cos::SaveOptions::default()).map_err(kit_err)
        } else {
            Ok(self.doc.bytes().to_vec())
        }
    }
}
