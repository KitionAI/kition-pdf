//! `kition-pdf`: the PDF kit for Kition's agent. The same document the in-page editor runs
//! (`kition-pdf-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange, and the identity a file
//! carries (`file.identity`, `file.fork`): a `.pdf` embeds no replica (the kit makes one
//! under a new id each time it opens a file), so it carries none and a fork is the same bytes. The
//! host owns every file (KHP A29): the kit reaches none by path, starts no program and opens no
//! connection.

use kition_headless::{FileIdentity, Headless, KitInfo};

const SANDBOXED: &[&str] = &[];
/// `inspect()` keys about the editing session rather than the file.
const VOLATILE: &[&str] = kition_pdf_kit::EDITOR_STATE;

fn main() -> std::process::ExitCode {
    let kit = match KitInfo::from_manifest(include_str!("../../../kit.json"), SANDBOXED, VOLATILE) {
        Ok(kit) => kit,
        Err(error) => {
            eprintln!("kit: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let backend = Headless::<kition_pdf_kit::PdfDocument>::with_replica_port(kit.clone()).with_file_identity(FileIdentity::NotEmbedded);
    kition_headless::serve_backend(&kit, backend)
}
