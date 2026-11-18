use super::*;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest, RequestKind};
use std::{collections::BTreeMap, io::BufReader, sync::Arc};

pub(super) fn run(args: &[String]) -> Result<()> {
    validate_args(args, &["--manifest", "--out", "--suite", "--dpi", "--timeout", "--group", "--limit-files"], &["--resume"], None)?;
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(arg(args, "--manifest")?)?).context("expected native schema-v2 manifest; run golden inventory first")?;
    ensure!(manifest.schema_version == SCHEMA && !manifest.files.is_empty(), "unsupported or empty manifest; run golden inventory first");
    let out = PathBuf::from(arg(args, "--out")?);
    fs::create_dir_all(&out)?;
    let suite = option(args, "--suite").unwrap_or_else(|| "smoke".into());
    ensure!(matches!(suite.as_str(), "smoke" | "full"), "--suite must be smoke or full");
    let dpi = number(args, "--dpi", 72)?;
    ensure!(dpi <= 288, "--dpi must be <= 288");
    let timeout = number(args, "--timeout", 60)?;
    let resume = args.iter().any(|s| s == "--resume");
    let limit = number(args, "--limit-files", u64::MAX)?;
    let group = option(args, "--group");
    let env = environment()?;
    ensure!(env["poppler"].is_string(), "pdftoppm (Poppler) is required on PATH");
    let fingerprint = hex(&Sha256::digest(serde_json::to_vec(
        &json!({"inventory_environment":manifest.environment,"environment":env,"dpi":dpi,"timeout":timeout,"schema":SCHEMA}),
    )?));
    let mut results = Vec::new();
    let mut file_results = Vec::new();
    let mut selected = 0u64;
    for entry in &manifest.files {
        if group.as_ref().is_some_and(|g| g != &entry.group) || selected >= limit {
            continue;
        }
        selected += 1;
        ensure!(entry.sha256.len() == 64 && entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()), "invalid manifest SHA-256");
        let actual_sha = sha(&entry.path);
        if !matches!(actual_sha, Ok(ref hash) if hash == &entry.sha256) {
            file_results.push(json!({"source":entry.source,"group":entry.group,"status":"hash_error","error":actual_sha.err().map(|e| e.to_string()),"expected_pages":entry.metadata["page_count"]}));
            continue;
        }
        let count = entry.metadata["page_count"].as_u64().unwrap_or(0);
        if count == 0 || entry.metadata["status"] != "ok" {
            file_results.push(json!({"source":entry.source,"group":entry.group,"status":"metadata_error","metadata":entry.metadata}));
            continue;
        }
        ensure!(count <= 100_000, "manifest page count exceeds safety limit");
        let pages = selected_pages(count, &suite);
        file_results.push(json!({"source":entry.source,"group":entry.group,"status":"scheduled","total_pages":count,"selected_pages":pages}));
        for page in pages {
            let dir = out.join(&entry.sha256).join(format!("page-{page}-{dpi}dpi"));
            fs::create_dir_all(&dir)?;
            let result_path = dir.join("result.json");
            let cached: Option<Value> = if resume { fs::read(&result_path).ok().and_then(|b| serde_json::from_slice(&b).ok()) } else { None };
            if let Some(cached) = cached.filter(|v| {
                v["fingerprint"] == fingerprint
                    && v["sha256"] == entry.sha256
                    && v["source"] == entry.source
                    && v["group"] == entry.group
                    && v["status"] == "ok"
                    && ["native", "reference", "diff"]
                        .iter()
                        .all(|name| sha(&dir.join(format!("{name}.png"))).is_ok_and(|hash| v["artifact_sha256"][name] == hash))
            }) {
                results.push(cached);
                continue;
            }
            let result = measure(entry, page, dpi, timeout, &dir, &fingerprint).unwrap_or_else(|e| json!({"source":entry.source,"group":entry.group,"page":page,"sha256":entry.sha256,"fingerprint":fingerprint,"status":"harness_error","error":format!("{e:#}")}));
            json_write(&result_path, &result)?;
            eprintln!("{} page {page}/{count}: {}", entry.source, result["status"]);
            results.push(result);
            // Keep this checkpoint small: full-corpus runs must not rewrite all prior results per page.
            json_write(
                &out.join("progress.json"),
                &json!({"completed_pages":results.len(),"last_source":entry.source,"last_page":page,"fingerprint":fingerprint}),
            )?;
        }
    }
    ensure!(selected > 0, "selection matched no PDF files");
    let mut groups: BTreeMap<String, Value> = BTreeMap::new();
    for file in &file_results {
        let group = file["group"].as_str().unwrap_or("unknown").to_string();
        let value = groups
            .entry(group)
            .or_insert_with(|| json!({"files":0,"file_preflight_failures":0,"document_pages":0,"pages":0,"ok":0,"failed":0,"visual_candidates":0}));
        value["files"] = json!(value["files"].as_u64().unwrap_or(0) + 1);
        value["document_pages"] = json!(
            value["document_pages"].as_u64().unwrap_or(0) + file["total_pages"].as_u64().or_else(|| file["expected_pages"].as_u64()).unwrap_or(0)
        );
        if file["status"] != "scheduled" {
            value["file_preflight_failures"] = json!(value["file_preflight_failures"].as_u64().unwrap_or(0) + 1);
        }
    }
    for result in &results {
        let group = result["group"].as_str().unwrap_or("unknown").to_string();
        let value = groups.entry(group).or_insert_with(|| json!({"pages":0,"ok":0,"failed":0,"visual_candidates":0}));
        for key in ["pages", if result["status"] == "ok" { "ok" } else { "failed" }] {
            value[key] = json!(value[key].as_u64().unwrap_or(0) + 1);
        }
        if result["metrics"]["visual_candidate"] == true {
            value["visual_candidates"] = json!(value["visual_candidates"].as_u64().unwrap_or(0) + 1);
        }
    }
    let failed = results.iter().filter(|r| r["status"] != "ok").count() + file_results.iter().filter(|r| r["status"] != "scheduled").count();
    json_write(
        &out.join("report.json"),
        &json!({"schema_version":SCHEMA,"environment":env,"fingerprint":fingerprint,"suite":suite,"dpi":dpi,"selected_files":selected,"failed_tasks":failed,"groups":groups,"files":file_results,"pages":results,"limitations":["native PageRenderer; no SDK/WASM coverage","cold open per page, no warm-cache timing","allocated raster bytes are a memory proxy, not peak RSS","global luminance SSIM, not windowed SSIM","reference differences require review; Poppler is not absolute truth","no Ghostscript cross-check","text find uses a generated substring, not a labeled accuracy oracle"]}),
    )?;
    html(&out, &results)?;
    eprintln!("report: {} ({} pages, {failed} failures)", out.join("report.json").display(), results.len());
    ensure!(failed == 0, "{failed} golden tasks failed; all failures are recorded in report.json");
    Ok(())
}
fn selected_pages(count: u64, suite: &str) -> Vec<u64> {
    if suite == "full" {
        (1..=count).collect()
    } else {
        let mut pages = vec![1, count.div_ceil(2), count];
        pages.sort_unstable();
        pages.dedup();
        pages
    }
}
fn measure(entry: &Entry, page: u64, dpi: u64, timeout: u64, dir: &Path, fingerprint: &str) -> Result<Value> {
    let mut native = Command::new(std::env::current_exe()?);
    native.args(["golden", "worker-render"]).arg(&entry.path).arg(page.to_string()).arg(dpi.to_string()).arg(dir);
    let native_process = timed(&mut native, timeout, &dir.join("native.log"))?;
    let mut reference = Command::new("pdftoppm");
    reference
        .args(["-f", &page.to_string(), "-l", &page.to_string(), "-singlefile", "-cropbox", "-r", &dpi.to_string(), "-png"])
        .arg(&entry.path)
        .arg(dir.join("reference"));
    let reference_process = timed(&mut reference, timeout, &dir.join("reference.log"))?;
    let native_details: Value =
        if native_process["status"] == "ok" { serde_json::from_slice(&fs::read(dir.join("native.json"))?)? } else { Value::Null };
    let mut result = json!({"source":entry.source,"group":entry.group,"sha256":entry.sha256,"page":page,"dpi":dpi,"fingerprint":fingerprint,"native_process":native_process,"reference_process":reference_process,"native":native_details,"status":"ok"});
    if result["native_process"]["status"] != "ok" || result["reference_process"]["status"] != "ok" {
        result["status"] = json!("process_error");
        return Ok(result);
    }
    if result["native"]["render_error"].is_string() || result["native"]["text_error"].is_string() {
        result["status"] = json!("engine_error");
        return Ok(result);
    }
    let native = read_png(&dir.join("native.png"))?;
    let reference = read_png(&dir.join("reference.png"))?;
    if native.0 != reference.0 || native.1 != reference.1 {
        result["status"] = json!("geometry_mismatch");
        result["dimensions"] = json!({"native":[native.0,native.1],"reference":[reference.0,reference.1]});
    } else {
        let (metrics, difference) = compare(&native.2, &reference.2)?;
        write_png(&dir.join("diff.png"), native.0, native.1, &difference)?;
        result["metrics"] = metrics;
        result["artifact_sha256"] =
            json!({"native":sha(&dir.join("native.png"))?,"reference":sha(&dir.join("reference.png"))?,"diff":sha(&dir.join("diff.png"))?});
        if result["native"]["render_warnings"].as_array().is_some_and(|w| !w.is_empty())
            || result["native"]["text_warnings"].as_array().is_some_and(|w| !w.is_empty())
        {
            result["status"] = json!("engine_warning");
        }
    }
    Ok(result)
}
pub(super) fn worker(args: &[String]) -> Result<()> {
    let path = args.first().context("missing PDF")?;
    let page: usize = args.get(1).context("missing page")?.parse()?;
    let dpi: f32 = args.get(2).context("missing dpi")?.parse()?;
    ensure!(page > 0 && dpi.is_finite() && dpi > 0.0 && dpi <= 288.0, "invalid page/dpi");
    let dir = PathBuf::from(args.get(3).context("missing output")?);
    ensure!(fs::metadata(path)?.len() <= MAX_PDF, "PDF too large");
    let open = Instant::now();
    let mut renderer = PageRenderer::new(Arc::new(fs::read(path)?), RenderConfig::default());
    let count = renderer.page_count();
    ensure!(page <= count, "page {page} is outside document ({count} pages)");
    let open_ms = open.elapsed().as_millis();
    let render = Instant::now();
    let pixels = renderer.render(RenderRequest { page: page - 1, scale: dpi / 72.0, ..Default::default() });
    let render_ms = render.elapsed().as_millis();
    if pixels.error.is_none() {
        let mut rgb = Vec::with_capacity(pixels.rgba.len() / 4 * 3);
        for p in pixels.rgba.as_chunks::<4>().0.iter() {
            let alpha = *p.get(3).context("invalid RGBA")?;
            for channel in p.iter().take(3) {
                rgb.push(channel.saturating_add(255 - alpha));
            }
        }
        write_png(&dir.join("native.png"), pixels.width, pixels.height, &rgb)?;
    }
    let text_start = Instant::now();
    let text = renderer.render(RenderRequest { page: page - 1, kind: RequestKind::Text, scale: 1.0, ..Default::default() });
    let plain = text.text.as_ref().map(|t| t.plain_text()).unwrap_or_default();
    let needle: String = plain.chars().take(8).collect();
    let matches = text.text.as_ref().filter(|_| !needle.is_empty()).map(|t| t.find(&needle).len());
    json_write(
        &dir.join("native.json"),
        &json!({"open_ms":open_ms,"render_ms":render_ms,"text_ms":text_start.elapsed().as_millis(),"width":pixels.width,"height":pixels.height,"raster_bytes_proxy":pixels.rgba.len(),"render_error":pixels.error,"render_warnings":pixels.warnings.iter().map(|w|format!("{w:?}")).collect::<Vec<_>>(),"text_error":text.error,"text_warnings":text.warnings.iter().map(|w|format!("{w:?}")).collect::<Vec<_>>(),"text_characters":plain.chars().count(),"text_sha256":hex(&Sha256::digest(plain.as_bytes())),"text_excerpt":plain.chars().take(160).collect::<String>(),"self_find_query":needle,"self_find_matches":matches}),
    )
}
fn write_png(path: &Path, width: u32, height: u32, rgb: &[u8]) -> Result<()> {
    let mut encoder = png::Encoder::new(fs::File::create(path)?, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgb)?;
    Ok(())
}
fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(BufReader::new(fs::File::open(path)?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let size = reader.output_buffer_size().context("PNG size overflow")?;
    ensure!(size <= 256_000_000, "PNG exceeds raster memory cap");
    let mut buffer = vec![0; size];
    let info = reader.next_frame(&mut buffer)?;
    ensure!(info.color_type == png::ColorType::Rgb, "expected RGB PNG from native/Poppler, got {:?}", info.color_type);
    buffer.truncate(info.buffer_size());
    Ok((info.width, info.height, buffer))
}
fn compare(a: &[u8], b: &[u8]) -> Result<(Value, Vec<u8>)> {
    ensure!(!a.is_empty() && a.len() == b.len() && a.len().is_multiple_of(3), "incompatible RGB buffers");
    let mut difference = Vec::with_capacity(a.len());
    let mut absolute = 0u64;
    let mut changed = 0u64;
    let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (a, b) in a.as_chunks::<3>().0.iter().zip(b.as_chunks::<3>().0.iter()) {
        let mut maximum = 0;
        for (a, b) in a.iter().zip(b) {
            let d = a.abs_diff(*b);
            difference.push(d);
            absolute += u64::from(d);
            maximum = maximum.max(d);
        }
        if maximum > 16 {
            changed += 1;
        }
        let x: f64 = a.iter().zip([0.2126, 0.7152, 0.0722]).map(|(v, w)| f64::from(*v) * w).sum();
        let y: f64 = b.iter().zip([0.2126, 0.7152, 0.0722]).map(|(v, w)| f64::from(*v) * w).sum();
        sx += x;
        sy += y;
        sxx += x * x;
        syy += y * y;
        sxy += x * y;
    }
    let n = (a.len() / 3) as f64;
    let mx = sx / n;
    let my = sy / n;
    let vx = (sxx / n - mx * mx).max(0.0);
    let vy = (syy / n - my * my).max(0.0);
    let cov = sxy / n - mx * my;
    let ssim = ((2.0 * mx * my + 6.5025) * (2.0 * cov + 58.5225)) / ((mx * mx + my * my + 6.5025) * (vx + vy + 58.5225));
    let ratio = changed as f64 / n;
    Ok((
        json!({"mean_absolute_channel_error":absolute as f64/a.len() as f64,"difference_pixel_ratio":ratio,"pixel_threshold":16,"global_luminance_ssim":ssim,"visual_candidate":ssim < 0.99 || ratio > 0.01}),
        difference,
    ))
}
fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}
fn html(out: &Path, results: &[Value]) -> Result<()> {
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>Local golden PDF comparison</title><style>body{font:14px sans-serif}img{width:32%;border:1px solid #ddd}article{margin:2em 0}</style><h1>Local golden PDF comparison</h1><p>Native / Poppler reference / absolute difference. Differences are review candidates, not correctness verdicts. See report.json for every file and failure.</p>",
    );
    for r in results {
        html.push_str(&format!(
            "<article><h2>{} — page {} ({})</h2>",
            escape(r["source"].as_str().unwrap_or("")),
            r["page"],
            escape(r["status"].as_str().unwrap_or(""))
        ));
        if let (Some(hash), Some(page), Some(dpi)) = (r["sha256"].as_str(), r["page"].as_u64(), r["dpi"].as_u64()) {
            for name in ["native", "reference", "diff"] {
                html.push_str(&format!("<img loading=lazy alt={name} src=\"{}/page-{page}-{dpi}dpi/{name}.png\">", escape(hash)));
            }
        }
        html.push_str("</article>");
    }
    fs::write(out.join("index.html"), html)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoke_keeps_middle_and_last() {
        assert_eq!(selected_pages(1044, "smoke"), vec![1, 522, 1044]);
        assert_eq!(selected_pages(1, "smoke"), vec![1]);
    }
    #[test]
    fn metrics_detect_missing_content() {
        let (same, _) = compare(&[255; 12], &[255; 12]).unwrap();
        assert_eq!(same["global_luminance_ssim"], 1.0);
        assert_eq!(same["visual_candidate"], false);
        let (changed, diff) = compare(&[255; 12], &[0; 12]).unwrap();
        assert_eq!(changed["difference_pixel_ratio"], 1.0);
        assert_eq!(changed["visual_candidate"], true);
        assert_eq!(diff, vec![255; 12]);
    }
}
