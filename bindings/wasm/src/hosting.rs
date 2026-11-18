//! How a host runs this kit's documents (KHP A29, A37, A45, section 10.6).
//!
//! In the page the kit already reaches no file: it has no file system. A host that serves the kit
//! to an agent (`kition-pdf`, the headless kit) owns every file too: [`set_files_only`] turns on
//! the engine's host-only mode for the whole process, so no tool reads, writes or lists a file by
//! path (however it is reached: `commands.execute`, `model.batch`, any origin), starts a program
//! (the print spooler) or asks a keychain, and recognition looks for its models in no folder
//! (`kition_pdf_automation::set_host_files_only`).
//!
//! kition-headless sets `kition_kit::host::set_hosting` and tells each document through
//! `KitDocument::hosted`, which calls [`set_files_only`]; tests that run documents in-process call
//! it themselves.

use std::sync::atomic::{AtomicBool, Ordering};

static FILES_ONLY: AtomicBool = AtomicBool::new(false);

/// Says a host serves this process's documents to an agent, owning every file: what
/// kition-headless's `set_hosting` will say. It turns on the engine's host-only mode for the
/// whole process; there is no way back.
pub fn set_files_only() {
    FILES_ONLY.store(true, Ordering::SeqCst);
    kition_pdf_automation::set_host_files_only();
}

/// Whether a host serves this process's documents to an agent ([`set_files_only`]).
pub fn files_only() -> bool {
    FILES_ONLY.load(Ordering::SeqCst) || kition_kit::host::hosting().files_only
}
