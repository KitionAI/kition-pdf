//! Writes the inputs of pdf's `layer: eval` cases that the conformance fixtures do not cover:
//! `report.pdf`, three pages headed Overview, Budget for 2026 and Appendix, so a case can tell a
//! 1-based page from a 0-based one. Written object by object, the same bytes on every run.
//! `cargo run -p kition-pdf-kit --example ai_inputs -- <folder>`.

use std::path::Path;

/// A PDF of `objects` (object 1 is the catalog), with an xref table.
fn pdf(objects: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

fn stream(body: &str) -> String {
    format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
}

/// Three pages, each a heading and a line in Helvetica (one of the standard 14 fonts).
fn report() -> Vec<u8> {
    let pages = [
        ("Overview", "Sales rose in every region."),
        ("Budget for 2026", "Spending stays within the plan."),
        ("Appendix", "Figures are in thousands."),
    ];
    // 1 catalog, 2 pages, 3 font, then a page object and its content stream per page.
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {} /MediaBox [0 0 612 792] >>", kids.join(" "), pages.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    for (i, (heading, line)) in pages.iter().enumerate() {
        objects.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i));
        objects.push(stream(&format!("BT /F1 24 Tf 72 720 Td ({heading}) Tj /F1 12 Tf 0 -32 Td ({line}) Tj ET")));
    }
    pdf(&objects)
}

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("give the folder to write into")?;
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    std::fs::write(dir.join("report.pdf"), report()).map_err(|error| error.to_string())?;
    println!("wrote report.pdf");
    Ok(())
}
