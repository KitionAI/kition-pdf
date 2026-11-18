//! Bounded page tiles and exact reduction of the integer 1:1 canvas.

use super::*;

const TILE_SIDE: u32 = 1024;
const SOURCE_SIDE: u32 = 512;
const REDUCTION_ROWS: u32 = 16;
// These cap work for one entire SDK call, including all pages and reduction chunks.
const MAX_SAMPLES: u64 = 64_000_000;
const MAX_REQUESTS: u32 = 4096;
const MAX_FULL_PIXELS: u64 = 16 * 1024 * 1024;
// Beyond this, f32 renderer offsets cannot represent every integer pixel.
const MAX_EXACT_OFFSET: u64 = 1 << 24;

fn work_error() -> String {
    "PDF view exceeds rendering work budget. Retry with a smaller area or zoom in; pending edits are retained.".into()
}

#[derive(Default)]
struct Work {
    pixels: u64,
    requests: u32,
}

impl Work {
    fn charge(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.pixels = self.pixels.saturating_add(u64::from(width) * u64::from(height));
        self.requests = self.requests.saturating_add(1);
        if self.pixels > MAX_SAMPLES || self.requests > MAX_REQUESTS {
            return Err(work_error());
        }
        Ok(())
    }
}

fn filled_frame(width: u32, height: u32, color: [u8; 4]) -> Result<Vec<u8>, String> {
    let count = (width as usize).checked_mul(height as usize).ok_or_else(work_error)?;
    let bytes = count.checked_mul(4).ok_or_else(work_error)?;
    let mut out = Vec::new();
    out.try_reserve_exact(bytes).map_err(|_| "Could not allocate the PDF view. Retry with a smaller area.".to_owned())?;
    for _ in 0..count {
        out.extend_from_slice(&color);
    }
    Ok(out)
}

fn render_tile(renderer: &mut Renderer, index: usize, scale: f32, tile: Tile, work: &mut Work) -> Result<RenderedPage, String> {
    if u64::from(tile.x) + u64::from(tile.w) > MAX_EXACT_OFFSET || u64::from(tile.y) + u64::from(tile.h) > MAX_EXACT_OFFSET {
        return Err(format!(
            "PDF page {} tile coordinates exceed exact renderer precision. Retry at a lower zoom or nearer the page origin.",
            index + 1
        ));
    }
    work.charge(tile.w, tile.h)?;
    #[cfg(feature = "cache-diagnostics")]
    diagnostics::tile_request();
    let page = renderer.pages.render(RenderRequest { page: index, kind: RequestKind::Pixels, tile: Some(tile), scale, tag: 0 });
    check_rendered_page(&page)?;
    if (page.width, page.height) != (tile.w, tile.h) {
        return Err(format!("PDF page {} returned an incomplete tile. Retry with a smaller area.", index + 1));
    }
    Ok(page)
}

impl PdfDocument {
    /// Premultiplied canvas at vector scale. Subdivide the output intersection so the
    /// renderer never silently clamps a tile's dimensions to its 4096-pixel limit.
    pub(super) fn compose(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        if !scale.is_finite() || !(0.01..=400.0).contains(&scale) {
            return Err("PDF zoom exceeds renderer limits. Retry at a zoom between 0.01 and 400.".into());
        }
        let mut out = filled_frame(width, height, self.pasteboard)?;
        let mut work = Work::default();
        for (index, area) in self.areas().iter().enumerate() {
            let px0 = ((area.x - x) * scale).floor().max(0.0);
            let py0 = ((area.y - y) * scale).floor().max(0.0);
            let px1 = ((area.x + area.w - x) * scale).ceil().min(f64::from(width));
            let py1 = ((area.y + area.h - y) * scale).ceil().min(f64::from(height));
            if px1 <= px0 || py1 <= py0 {
                continue;
            }
            let origin_x = (px0 + (x - area.x) * scale).max(0.0);
            let origin_y = (py0 + (y - area.y) * scale).max(0.0);
            if origin_x > MAX_EXACT_OFFSET as f64 || origin_y > MAX_EXACT_OFFSET as f64 {
                return Err(format!("PDF page {} tile coordinates exceed exact renderer precision. Retry at a lower zoom.", index + 1));
            }
            let (w, h) = ((px1 - px0) as u32, (py1 - py0) as u32);
            self.with_renderer(|renderer| -> Result<(), String> {
                for ty in (0..h).step_by(TILE_SIDE as usize) {
                    for tx in (0..w).step_by(TILE_SIDE as usize) {
                        let tile = Tile { x: origin_x as u32 + tx, y: origin_y as u32 + ty, w: (w - tx).min(TILE_SIDE), h: (h - ty).min(TILE_SIDE) };
                        let page = render_tile(renderer, index, scale as f32, tile, &mut work)?;
                        blit(&mut out, width, page.rgba.as_ref(), page.width, page.height, px0 as u32 + tx, py0 as u32 + ty);
                    }
                }
                Ok(())
            })
            .ok_or("PDF document is no longer open")??;
        }
        Ok(out)
    }

    pub(super) fn full(&self, x0: i64, y0: i64, w: u32, h: u32) -> Result<Vec<u8>, String> {
        self.full_with_work(x0, y0, w, h, &mut Work::default())
    }

    fn full_with_work(&self, x0: i64, y0: i64, w: u32, h: u32, work: &mut Work) -> Result<Vec<u8>, String> {
        let mut out = filled_frame(w, h, self.pasteboard)?;
        for (index, area) in self.areas().iter().enumerate() {
            let (sx, sy) = (area.x.round() as i64, area.y.round() as i64);
            let (pw, ph) = (area.w.ceil() as u32, area.h.ceil() as u32);
            let (ox0, oy0) = (sx.max(x0), sy.max(y0));
            let (ox1, oy1) = (
                sx.saturating_add(i64::from(pw)).min(x0.saturating_add(i64::from(w))),
                sy.saturating_add(i64::from(ph)).min(y0.saturating_add(i64::from(h))),
            );
            if ox0 >= ox1 || oy0 >= oy1 {
                continue;
            }
            self.with_renderer(|renderer| -> Result<(), String> {
                // A large page never needs a full-frame scratch allocation, even when the
                // renderer would allow it. Ordinary pages retain the existing LRU path.
                let cacheable = pw <= 8192 && ph <= 8192 && u64::from(pw) * u64::from(ph) <= MAX_FULL_PIXELS;
                if cacheable {
                    let page = if let Some(page) = renderer.full.get(index) {
                        page
                    } else {
                        work.charge(pw, ph)?;
                        #[cfg(feature = "cache-diagnostics")]
                        diagnostics::full_render_request();
                        let rendered =
                            renderer.pages.render(RenderRequest { page: index, kind: RequestKind::Pixels, tile: None, scale: 1.0, tag: 0 });
                        check_rendered_page(&rendered)?;
                        let page = Arc::new(PageImage::new(rendered.rgba, rendered.width, rendered.height));
                        renderer.full.insert(index, page.clone());
                        page
                    };
                    for row in oy0..oy1.min(sy.saturating_add(i64::from(page.height))) {
                        let from = (((row - sy) * i64::from(page.width) + (ox0 - sx)) * 4) as usize;
                        let to = (((row - y0) * i64::from(w) + (ox0 - x0)) * 4) as usize;
                        let len = ((ox1.min(sx.saturating_add(i64::from(page.width))) - ox0).max(0) * 4) as usize;
                        if let (Some(source), Some(target)) = (page.pixels.get(from..from + len), out.get_mut(to..to + len)) {
                            over(target, source);
                        }
                    }
                } else {
                    #[cfg(feature = "cache-diagnostics")]
                    diagnostics::large_page_bypass();
                    for row in (oy0..oy1).step_by(TILE_SIDE as usize) {
                        for col in (ox0..ox1).step_by(TILE_SIDE as usize) {
                            let tile = Tile {
                                x: (col - sx) as u32,
                                y: (row - sy) as u32,
                                w: (ox1 - col).min(i64::from(TILE_SIDE)) as u32,
                                h: (oy1 - row).min(i64::from(TILE_SIDE)) as u32,
                            };
                            let page = render_tile(renderer, index, 1.0, tile, work)?;
                            blit(&mut out, w, page.rgba.as_ref(), page.width, page.height, (col - x0) as u32, (row - y0) as u32);
                        }
                    }
                }
                Ok(())
            })
            .ok_or("PDF document is no longer open")??;
        }
        straight(&mut out);
        Ok(out)
    }

    pub(super) fn reduce_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let mut work = Work::default();
        reduce(x0, y0, x1, y1, width, height, |x, y, w, h| self.full_with_work(x, y, w, h, &mut work))
    }
}

/// Match the pinned SDK's integer bins and rounded straight-alpha channel mean exactly.
/// Keep only 16 output rows of sums (at most 4 MiB) and one 512² source tile (1 MiB).
fn reduce(
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    width: u32,
    height: u32,
    mut source: impl FnMut(i64, i64, u32, u32) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
    let span_x = i64::from(x1.saturating_sub(x0)).max(1);
    let span_y = i64::from(y1.saturating_sub(y0)).max(1);
    // Repeated samples when upsampling count as work too; neither axis is clamped.
    if (span_x.max(i64::from(width)) as u64).saturating_mul(span_y.max(i64::from(height)) as u64) > MAX_SAMPLES {
        return Err(work_error());
    }
    let edges = |t: u32, span: i64, count: u32| {
        let a = i64::from(t) * span / i64::from(count);
        (a, (i64::from(t + 1) * span / i64::from(count)).max(a + 1))
    };
    // A very skinny source can fit the pixel cap but still require excessive tile
    // setup/interpretation work. Preflight every band, including duplicate upsample bins.
    let columns = (span_x as u64).div_ceil(u64::from(SOURCE_SIDE));
    let mut fetches = 0u64;
    for first_row in (0..height).step_by(REDUCTION_ROWS as usize) {
        let rows = (height - first_row).min(REDUCTION_ROWS);
        let covered = edges(first_row + rows - 1, span_y, height).1 - edges(first_row, span_y, height).0;
        fetches = fetches.saturating_add(columns.saturating_mul((covered as u64).div_ceil(u64::from(SOURCE_SIDE))));
    }
    if fetches > u64::from(MAX_REQUESTS) {
        return Err(work_error());
    }
    let x_bins: Vec<_> = (0..width).map(|col| edges(col, span_x, width)).collect();
    let mut out = filled_frame(width, height, [0; 4])?;
    for first_row in (0..height).step_by(REDUCTION_ROWS as usize) {
        let rows = (height - first_row).min(REDUCTION_ROWS);
        let mut sums = vec![[0u64; 4]; width as usize * rows as usize];
        let sy0 = edges(first_row, span_y, height).0;
        let sy1 = edges(first_row + rows - 1, span_y, height).1;
        for ty in (sy0..sy1).step_by(SOURCE_SIDE as usize) {
            let th = (sy1 - ty).min(i64::from(SOURCE_SIDE)) as u32;
            for tx in (0..span_x).step_by(SOURCE_SIDE as usize) {
                let tw = (span_x - tx).min(i64::from(SOURCE_SIDE)) as u32;
                let pixels = source(i64::from(x0) + tx, i64::from(y0) + ty, tw, th)?;
                for row in 0..rows {
                    let (ay, by) = edges(first_row + row, span_y, height);
                    let (ay, by) = (ay.max(ty), by.min(ty + i64::from(th)));
                    if ay >= by {
                        continue;
                    }
                    let first_col = x_bins.partition_point(|(_, end)| *end <= tx);
                    let last_col = x_bins.partition_point(|(start, _)| *start < tx + i64::from(tw));
                    for col in first_col..last_col {
                        let (ax, bx) = *x_bins.get(col).ok_or_else(work_error)?;
                        let (ax, bx) = (ax.max(tx), bx.min(tx + i64::from(tw)));
                        if ax >= bx {
                            continue;
                        }
                        let Some(sum) = sums.get_mut(row as usize * width as usize + col) else {
                            return Err(work_error());
                        };
                        for sy in ay..by {
                            let from = ((sy - ty) * i64::from(tw) + ax - tx) as usize * 4;
                            let len = (bx - ax) as usize * 4;
                            let samples = pixels.get(from..from + len).ok_or("PDF renderer returned an incomplete source tile. Retry the view.")?;
                            for pixel in samples.as_chunks::<4>().0 {
                                for (total, value) in sum.iter_mut().zip(pixel) {
                                    *total += u64::from(*value);
                                }
                            }
                        }
                    }
                }
            }
        }
        for row in 0..rows {
            let (ay, by) = edges(first_row + row, span_y, height);
            for col in 0..width {
                let (ax, bx) = edges(col, span_x, width);
                let count = ((by - ay) * (bx - ax)) as u64;
                let sum = sums.get(row as usize * width as usize + col as usize).ok_or_else(work_error)?;
                let at = ((first_row + row) as usize * width as usize + col as usize) * 4;
                let target = out.get_mut(at..at + 4).ok_or_else(work_error)?;
                for (target, total) in target.iter_mut().zip(sum) {
                    *target = ((total + count / 2) / count) as u8;
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_kit::raster::{self, Frame};

    #[test]
    fn streamed_bins_match_pinned_helper_for_gradients_alpha_and_mixed_ratios() {
        let (w, h) = (1031u32, 539u32);
        let pixels: Vec<u8> = (0..w * h).flat_map(|i| [(i % 256) as u8, (i / 3 % 256) as u8, (i / 7 % 256) as u8, (i / 11 % 256) as u8]).collect();
        let frame = Frame { rgba: &pixels, width: w, height: h };
        for (x0, y0, x1, y1, ow, oh) in [
            (0, 0, 1031, 539, 71, 29),
            (-3, -2, 1036, 544, 113, 57),
            (128, 33, 139, 47, 32, 5),
            (128, 33, 139, 47, 5, 32),
            (100, 20, 101, 21, 17, 19),
            (120, 120, 100, 100, 7, 3),
            (i32::MAX - 2, i32::MAX - 2, i32::MAX, i32::MAX, 3, 2),
        ] {
            let mut peak_pixels = 0;
            let actual = reduce(x0, y0, x1, y1, ow, oh, |x, y, width, height| {
                peak_pixels = peak_pixels.max(width * height);
                let mut tile = Vec::new();
                for row in 0..height {
                    for col in 0..width {
                        let (sx, sy) = (x + i64::from(col), y + i64::from(row));
                        if sx >= 0 && sx < i64::from(w) && sy >= 0 && sy < i64::from(h) {
                            let at = (sy as usize * w as usize + sx as usize) * 4;
                            tile.extend_from_slice(&pixels[at..at + 4]);
                        } else {
                            tile.extend_from_slice(&[0; 4]);
                        }
                    }
                }
                Ok(tile)
            })
            .unwrap();
            assert_eq!(actual, raster::render_area(frame, x0, y0, x1, y1, ow, oh));
            assert!(peak_pixels <= SOURCE_SIDE * SOURCE_SIDE);
        }
    }

    #[test]
    fn excessive_skinny_or_upsampled_views_reject_before_source_fetch() {
        for (x1, y1, w, h) in [(64_000_000, 1, 8192, 1), (1, 64_000_000, 1, 8192), (1, 1, 8192, 8192)] {
            let mut calls = 0;
            let error = reduce(0, 0, x1, y1, w, h, |_, _, _, _| {
                calls += 1;
                Ok(Vec::new())
            })
            .unwrap_err();
            assert!(error.contains("rendering work budget"));
            assert_eq!(calls, 0);
        }
    }
}
