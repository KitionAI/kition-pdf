//! `kition-pdf`: the PDF kit for Kition's agent. The same document the in-page editor runs
//! (`kition-pdf-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange. The host owns every
//! file (KHP A29): the kit reaches none by path, starts no program and opens no connection.

const SANDBOXED: &[&str] = &[];
/// `inspect()` keys about the editing session rather than the file.
const VOLATILE: &[&str] = &[];

fn main() -> std::process::ExitCode {
    kition_headless::run_manifest_model::<kition_pdf_kit::PdfDocument>(include_str!("../../../kit.json"), SANDBOXED, VOLATILE)
}
