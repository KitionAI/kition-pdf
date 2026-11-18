use crate::{RenderCache, derive_settings};
use hayro_interpret::encode::EncodedShadingPattern;
use hayro_interpret::font::Glyph;
use hayro_interpret::pattern::Pattern;
use hayro_interpret::{
    BlendMode, CacheKey, ClipPath, Device, FillRule, GlyphDrawMode, ImageData, LumaData, MaskType,
    Paint, PathDrawMode, RgbData, SoftMask, StrokeProps,
};
use kurbo::{Affine, BezPath, Point, Rect, Shape, Vec2};
use pic_scale::{
    ImageSize, ImageStore, ImageStoreMut, PicScaleError, Resampling, ResamplingFunction, Scaler,
};
use std::collections::HashMap;
use std::mem::size_of;
use std::rc::Rc;
use std::sync::Arc;
use vello_cpu::color::palette::css::BLACK;
use vello_cpu::color::{AlphaColor, PremulRgba8, Srgb};
use vello_cpu::peniko::{Compose, Fill, ImageQuality, ImageSampler, Mix};
use vello_cpu::{
    Image, ImageSource, Mask, PaintType, Pixmap, RenderContext, RenderSettings, peniko,
};

/// Kition PDF patch: largest image drawn, in pixels (2^28 ≈ 268 MP, well above real page images).
const MAX_IMAGE_PIXELS: u64 = 1 << 28;

/// Kition PDF patch: source axes allowed into CatmullRom planning. Unlike the final pixmap,
/// sources need not fit u16: a 65536-pixel strip can safely shrink to a 16-pixel image.
/// This resource cap follows the existing CCITT column limit; the filter-work estimate below
/// additionally bounds tables/scratch before entering the opaque PicScale planner.
const MAX_RESAMPLING_SOURCE_SIDE: u32 = 1 << 20;
const MAX_RESAMPLING_PLAN_BYTES: usize = 128 << 20;

/// Kition PDF patch: source, intermediate and RGBA byte lengths, independent of backend sides.
fn source_byte_len(width: u32, height: u32, channels: usize, max_pixels: u64) -> Option<usize> {
    if width == 0 || height == 0 || !matches!(channels, 1 | 3 | 4) {
        return None;
    }
    let pixels = u64::from(width).checked_mul(u64::from(height))?;
    if pixels > max_pixels {
        return None;
    }
    let pixels = usize::try_from(pixels).ok()?;
    let rgba_len = pixels.checked_mul(4)?;
    if rgba_len > isize::MAX as usize {
        return None;
    }
    pixels.checked_mul(channels)
}

/// Kition PDF patch: only final pixmaps/padded glyphs need to fit the backend's u16 sides.
fn image_byte_len(width: u32, height: u32, channels: usize, max_pixels: u64) -> Option<usize> {
    if width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
        return None;
    }
    source_byte_len(width, height, channels, max_pixels)
}

/// Kition PDF patch: conservative CatmullRom planning work for one axis in PicScale 0.7.12.
/// Its four-tap kernel grows on downscaling. Include float/32-aligned i16 weights, temporary
/// float/f64/usize tap arrays and two copies of the bounds. This is a size estimate, not a
/// fallible reserve inside PicScale or a bound on total renderer memory.
fn filter_plan_bytes(source: u32, target: u32) -> Option<usize> {
    if source == target {
        return Some(0);
    }
    let (source, target) = (usize::try_from(source).ok()?, usize::try_from(target).ok()?);
    if target == 0 {
        return None;
    }
    let taps = source.checked_mul(4)?.div_ceil(target).max(4);
    let aligned_taps = taps.div_ceil(32).checked_mul(32)?;
    let float_weights = taps.checked_mul(target)?.checked_mul(size_of::<f32>())?;
    let fixed_weights = aligned_taps
        .checked_mul(target)?
        .checked_mul(size_of::<i16>())?;
    let tap_scratch = taps.checked_mul(size_of::<f32>() + size_of::<f64>() + size_of::<usize>())?;
    let bounds = target.checked_mul(4)?.checked_mul(size_of::<usize>())?;
    float_weights
        .checked_add(fixed_weights)?
        .checked_add(tap_scratch)?
        .checked_add(bounds)
}

/// Kition PDF patch: alpha-aware RGBA plans also need source-sized premultiplication scratch.
/// Bound the combined scratch before planning, including a conservative crossed intermediate
/// (or four rows for a short target) when both convolution axes change.
fn resampling_scratch_byte_len(
    source: (u32, u32),
    target: (u32, u32),
    channels: usize,
    max_pixels: u64,
) -> Option<usize> {
    if source == target {
        return Some(0);
    }
    let alpha = if channels == 4 {
        source_byte_len(source.0, source.1, channels, max_pixels)?
    } else {
        0
    };
    let filter = if source.0 != target.0 && source.1 != target.1 {
        source_byte_len(source.0, target.1.max(4), channels, max_pixels)?
    } else {
        0
    };
    let len = alpha.checked_add(filter)?;
    let limit = max_pixels.saturating_mul(4).min(isize::MAX as u64);
    if u64::try_from(len).ok()? > limit {
        return None;
    }
    Some(len)
}

/// Kition PDF patch: validate output, crossed intermediate and filter-work bounds before planning.
/// Source width × target height also bounds PicScale's smaller single-threaded four-row scratch.
fn resampling_byte_len(
    source: (u32, u32),
    target: (u32, u32),
    channels: usize,
    data_len: usize,
    max_pixels: u64,
) -> Option<usize> {
    if source_byte_len(source.0, source.1, channels, max_pixels)? != data_len {
        return None;
    }
    let target_len = image_byte_len(target.0, target.1, channels, max_pixels)?;
    resampling_scratch_byte_len(source, target, channels, max_pixels)?;
    source_byte_len(source.0, target.1, channels, max_pixels)?;
    if source.0 > MAX_RESAMPLING_SOURCE_SIDE || source.1 > MAX_RESAMPLING_SOURCE_SIDE {
        return None;
    }
    let plan_bytes = filter_plan_bytes(source.0, target.0)?
        .checked_add(filter_plan_bytes(source.1, target.1)?)?;
    if plan_bytes > MAX_RESAMPLING_PLAN_BYTES {
        return None;
    }
    Some(target_len)
}

/// Kition PDF patch: image dimensions selected before allocating resampling buffers.
/// Exported for Kition PDF's small, metadata-only regression tests. `None` rejects invalid
/// sources/scales or an over-budget target/intermediate; the caller may retain a valid source.
#[doc(hidden)]
pub fn image_resampling_size(
    width: u32,
    height: u32,
    channels: usize,
    data_len: usize,
    x_scale: f32,
    y_scale: f32,
    max_pixels: u64,
) -> Option<(u32, u32)> {
    if !x_scale.is_finite() || !y_scale.is_finite() || x_scale <= 0.0 || y_scale <= 0.0 {
        return None;
    }
    let target = if x_scale < 1.0 || y_scale < 1.0 {
        let side = |size: u32, scale: f32| {
            (size as f32 * scale)
                .ceil()
                .max(1.0)
                .min((u16::MAX / 2) as f32) as u32
        };
        (side(width, x_scale), side(height, y_scale))
    } else {
        (width, height)
    };
    resampling_byte_len((width, height), target, channels, data_len, max_pixels)?;
    Some(target)
}

/// Kition PDF patch: allocation failures in image conversion/resampling keep the renderer alive.
fn image_buffer(len: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    out.try_reserve_exact(len).ok()?;
    Some(out)
}

/// Kition PDF patch: most pieces one dashed stroke may be cut into. Stroke expansion keeps every
/// dash, so a pattern that is tiny next to its path asks for billions: a fuzzed `/D [[11] 0]` on
/// a line from x = 92234775807 allocated over 5 GB, and `[0.000001] 0 d` on a 20 pt line over
/// 10 GB. Measured in release builds: a stroke cut into 250,000 pieces renders in about 16 ms and
/// 75 MB at its peak (a million took 80 ms and 290 MB). Dotted and dashed lines in real documents
/// stay far below this.
const MAX_DASHES_PER_STROKE: f64 = 250_000.0;

/// Kition PDF patch: the dash pattern to stroke `path` with. Empty (a solid line) when the pattern
/// is invalid, or when it could cut the path into more than [`MAX_DASHES_PER_STROKE`] pieces.
/// ISO 32000-2 §8.4.3.6 requires nonnegative entries that are not all zero; a pattern whose
/// period isn't positive (`[-1 -1] 0 d`) made kurbo's search for the starting dash loop for ever.
fn usable_dash_pattern(path: &BezPath, dashes: &[f32]) -> Vec<f64> {
    if dashes.iter().any(|d| !d.is_finite() || *d < 0.0) {
        return Vec::new();
    }
    let sum: f64 = dashes.iter().map(|d| f64::from(*d)).sum();
    // kurbo repeats an odd-length pattern once, as SVG does, so a period covers it twice.
    let (period, per_period) = if dashes.len() % 2 == 1 {
        (2.0 * sum, dashes.len().saturating_mul(2))
    } else {
        (sum, dashes.len())
    };
    if !period.is_finite() || period <= 0.0 {
        return Vec::new();
    }
    let (length, restarts) = control_polygon_length(path);
    // kurbo restarts the pattern at every subpath and after every `ClosePath`, so each restart
    // can begin up to a whole period of pieces however short its segment is.
    let pieces = (length / period + restarts as f64) * per_period as f64;
    if !pieces.is_finite() || pieces > MAX_DASHES_PER_STROKE {
        return Vec::new();
    }
    dashes.iter().map(|d| f64::from(*d)).collect()
}

/// Bounds the input elements kurbo can retain or visit before its first yield.
const MAX_DASH_HAIRLINE_INPUT_ELEMENTS: usize = 250_000;

/// Prepare all centerlines before painting, retaining user-space arc lengths and closed seams.
/// Kurbo still allocates its bounded first-dash stash internally with infallible allocation.
/// On failure the caller retains the prior dashed-stroke path; no partial result is painted.
fn device_dash_path(
    path: &BezPath,
    transform: Affine,
    phase: f64,
    dashes: &[f64],
    cap: kurbo::Cap,
) -> Option<BezPath> {
    let count = path.elements().len();
    if count > MAX_DASH_HAIRLINE_INPUT_ELEMENTS
        || !path.is_finite()
        || !transform.as_coeffs().iter().all(|value| value.is_finite())
        || dashes.is_empty()
    {
        return None;
    }
    let limit = count
        .checked_mul(3)?
        .checked_add(2 * MAX_DASHES_PER_STROKE as usize)?;
    // A zero-length on-dash needs a tangent to draw a round/square cap. Kurbo's
    // emitted degenerate segment has lost that tangent. Recover it for a single
    // straight source segment or disconnected straight pairs. The pairs share a
    // bounded semantic walk, including pure-zero even patterns; unsupported sources
    // and exact terminal-zero boundaries retain legacy stroking.
    // Zero runs are identified semantically, never by a geometric epsilon.
    let has_zero_on = dashes
        .iter()
        .enumerate()
        .any(|(i, value)| *value == 0.0 && (dashes.len() % 2 == 1 || i % 2 == 0));
    let cap_delta = if has_zero_on && cap != kurbo::Cap::Butt {
        let pure_zero_even =
            dashes.len().is_multiple_of(2) && dashes.iter().step_by(2).all(|value| *value == 0.0);
        // Coincident pure-dot events retain their previous fallback until overlap
        // coverage is characterized independently. Mixed zero-gap support is unchanged.
        if count != 2 && pure_zero_even && dashes.iter().skip(1).step_by(2).any(|gap| *gap <= 0.0) {
            return None;
        }
        if count != 2 || !pure_zero_even {
            return if count == 2 {
                mixed_line_dash_path(path, transform, phase, dashes, limit)
            } else {
                separated_mixed_line_dash_path(
                    path,
                    transform,
                    phase,
                    dashes,
                    limit,
                    MAX_DASHES_PER_STROKE as usize,
                    MAX_DASHES_PER_STROKE as usize,
                )
            };
        }
        let [kurbo::PathEl::MoveTo(start), kurbo::PathEl::LineTo(end)] = path.elements() else {
            return None;
        };
        let tangent = *end - *start;
        let [a, b, c, d, _, _] = transform.as_coeffs();
        let tangent = Vec2::new(a * tangent.x + c * tangent.y, b * tangent.x + d * tangent.y);
        let length = tangent.hypot();
        if !length.is_finite() || length == 0.0 {
            return None;
        }
        Some(tangent * (0.0001 / length))
    } else {
        None
    };
    // Each input element can survive, restart or close a subpath; each estimated dash
    // transition can add a move and a split segment. This deliberately overestimates.
    let phase = if phase.is_finite() { phase } else { 0.0 };
    collect_device_dash(
        kurbo::dash(path.elements().iter().copied(), phase, dashes),
        transform,
        limit,
        cap_delta,
    )
}

/// Semantic splitting is restricted to mixed capped dashes on one straight source segment.
/// Pure dots and general positive/zero-gap paths retain their established preparation paths.
fn mixed_line_dash_path(
    path: &BezPath,
    transform: Affine,
    phase: f64,
    dashes: &[f64],
    max_elements: usize,
) -> Option<BezPath> {
    let [kurbo::PathEl::MoveTo(start), kurbo::PathEl::LineTo(end)] = path.elements() else {
        return None;
    };
    let source = *end - *start;
    let length = source.hypot();
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let tangent = Vec2::new(a * source.x + c * source.y, b * source.x + d * source.y);
    let device_length = tangent.hypot();
    if !length.is_finite() || length == 0.0 || !device_length.is_finite() || device_length == 0.0 {
        return None;
    }
    let cap_delta = tangent * (0.0001 / device_length);
    let mut output = Vec::new();
    let phase_steps = dashes
        .len()
        .checked_mul(if dashes.len() % 2 == 1 { 2 } else { 1 })?;
    mixed_dash_events(
        length,
        phase,
        dashes,
        phase_steps,
        MAX_DASHES_PER_STROKE as usize,
        |from, to, is_zero| {
            let first = transform * (*start + source * (from / length));
            let last = if is_zero {
                first + cap_delta
            } else {
                transform * (*start + source * (to / length))
            };
            if !first.is_finite()
                || !last.is_finite()
                || output.len().checked_add(2)? > max_elements
            {
                return None;
            }
            if output.capacity() - output.len() < 2 {
                output
                    .try_reserve_exact(
                        (max_elements - output.len()).min(output.capacity().max(1024)),
                    )
                    .ok()?;
            }
            output.push(kurbo::PathEl::MoveTo(first));
            output.push(kurbo::PathEl::LineTo(last));
            Some(())
        },
    )?;
    Some(BezPath::from_vec(output))
}

/// Validated phase state is prepared once, then copied at each disconnected subpath.
#[derive(Clone, Copy)]
struct MixedDashPattern<'a> {
    dashes: &'a [f64],
    entries: usize,
    index: usize,
    remaining: f64,
}

struct MixedDashBudget {
    events: usize,
    terminal: usize,
}

fn prepare_mixed_dash_pattern(
    phase: f64,
    dashes: &[f64],
    phase_limit: usize,
) -> Option<MixedDashPattern<'_>> {
    let repeats = if dashes.len() % 2 == 1 { 2 } else { 1 };
    let entries = dashes.len().checked_mul(repeats)?;
    if entries == 0
        || entries > MAX_DASHES_PER_STROKE as usize
        || dashes
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return None;
    }
    let period = dashes.iter().sum::<f64>() * repeats as f64;
    if !period.is_finite() || period <= 0.0 {
        return None;
    }
    let phase = if phase.is_finite() { phase } else { 0.0 };
    let mut offset = phase.rem_euclid(period);
    let mut index = 0;
    for _ in 0..phase_limit.min(entries) {
        let value = *dashes.get(index % dashes.len())?;
        if offset == 0.0 || offset < value {
            return Some(MixedDashPattern {
                dashes,
                entries,
                index,
                remaining: value - offset,
            });
        }
        // Keep the existing exact-positive-end cap semantics outside this repair.
        if offset == value && value > 0.0 && index % 2 == 0 {
            return None;
        }
        offset -= value;
        index = index.checked_add(1)? % entries;
    }
    None
}

/// Preserve the established single-line entry point and its phase/terminal bounds.
fn mixed_dash_events(
    length: f64,
    phase: f64,
    dashes: &[f64],
    phase_limit: usize,
    event_limit: usize,
    emit: impl FnMut(f64, f64, bool) -> Option<()>,
) -> Option<()> {
    let pattern = prepare_mixed_dash_pattern(phase, dashes, phase_limit)?;
    let mut budget = MixedDashBudget {
        events: event_limit.min(MAX_DASHES_PER_STROKE as usize),
        terminal: phase_limit,
    };
    prepared_mixed_dash_events(length, pattern, &mut budget, emit)
}

/// Each semantic visit and terminal lookup consumes a shared stroke-wide budget.
/// Explicit zeros remain distinct from positive intervals, which must advance position.
fn prepared_mixed_dash_events(
    length: f64,
    pattern: MixedDashPattern<'_>,
    budget: &mut MixedDashBudget,
    mut emit: impl FnMut(f64, f64, bool) -> Option<()>,
) -> Option<()> {
    if !length.is_finite() || length <= 0.0 {
        return None;
    }
    let MixedDashPattern {
        dashes,
        entries,
        mut index,
        mut remaining,
    } = pattern;
    let mut position = 0.0;
    loop {
        budget.events = budget.events.checked_sub(1)?;
        let active = index % 2 == 0;
        if remaining == 0.0 {
            if active {
                emit(position, position, true)?;
            }
        } else {
            let step = remaining.min(length - position);
            let next = position + step;
            if !next.is_finite() || next <= position {
                return None;
            }
            if active {
                emit(position, next, false)?;
            }
            position = next;
            if position == length && step < remaining {
                return Some(());
            }
        }
        index = index.checked_add(1)? % entries;
        remaining = *dashes.get(index % dashes.len())?;
        if position == length {
            // An explicit terminal zero on-entry has uncertain cap semantics. Look
            // through intervening zero gaps, bounded by the cycle and shared budget.
            for _ in 0..entries {
                budget.terminal = budget.terminal.checked_sub(1)?;
                if remaining > 0.0 {
                    return Some(());
                }
                if index % 2 == 0 {
                    return None;
                }
                index = index.checked_add(1)? % entries;
                remaining = *dashes.get(index % dashes.len())?;
            }
            return None;
        }
    }
}

fn separated_line_geometry(
    elements: &[kurbo::PathEl],
    transform: Affine,
) -> Option<(Point, Vec2, f64, Vec2)> {
    let [kurbo::PathEl::MoveTo(start), kurbo::PathEl::LineTo(end)] = elements else {
        return None;
    };
    if !start.is_finite()
        || !end.is_finite()
        || !(transform * *start).is_finite()
        || !(transform * *end).is_finite()
    {
        return None;
    }
    let source = *end - *start;
    let length = source.hypot();
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let tangent = Vec2::new(a * source.x + c * source.y, b * source.x + d * source.y);
    let device_length = tangent.hypot();
    if !length.is_finite() || length == 0.0 || !device_length.is_finite() || device_length == 0.0 {
        return None;
    }
    Some((*start, source, length, tangent * (0.0001 / device_length)))
}

/// Only disconnected MoveTo/LineTo pairs are accepted; no joins, curves or closed paths.
/// Preflight visits at most the existing source-element cap and allocates no subpath list.
fn separated_mixed_line_dash_path(
    path: &BezPath,
    transform: Affine,
    phase: f64,
    dashes: &[f64],
    max_elements: usize,
    event_limit: usize,
    terminal_limit: usize,
) -> Option<BezPath> {
    let elements = path.elements();
    if elements.len() <= 2
        || elements.len() > MAX_DASH_HAIRLINE_INPUT_ELEMENTS
        || !elements.len().is_multiple_of(2)
        || !transform.as_coeffs().iter().all(|value| value.is_finite())
    {
        return None;
    }
    for pair in elements.chunks_exact(2) {
        separated_line_geometry(pair, transform)?;
    }
    let pattern = prepare_mixed_dash_pattern(phase, dashes, MAX_DASHES_PER_STROKE as usize)?;
    let mut budget = MixedDashBudget {
        events: event_limit.min(MAX_DASHES_PER_STROKE as usize),
        terminal: terminal_limit.min(MAX_DASHES_PER_STROKE as usize),
    };
    let mut output = Vec::new();
    for pair in elements.chunks_exact(2) {
        let (start, source, length, cap_delta) = separated_line_geometry(pair, transform)?;
        prepared_mixed_dash_events(length, pattern, &mut budget, |from, to, is_zero| {
            let first = transform * (start + source * (from / length));
            let last = if is_zero {
                first + cap_delta
            } else {
                transform * (start + source * (to / length))
            };
            if !first.is_finite()
                || !last.is_finite()
                || output.len().checked_add(2)? > max_elements
            {
                return None;
            }
            if output.capacity() - output.len() < 2 {
                output
                    .try_reserve_exact(
                        (max_elements - output.len()).min(output.capacity().max(1024)),
                    )
                    .ok()?;
            }
            output.push(kurbo::PathEl::MoveTo(first));
            output.push(kurbo::PathEl::LineTo(last));
            Some(())
        })?;
    }
    Some(BezPath::from_vec(output))
}

fn collect_device_dash(
    elements: impl Iterator<Item = kurbo::PathEl>,
    transform: Affine,
    limit: usize,
    cap_delta: Option<Vec2>,
) -> Option<BezPath> {
    let mut output = Vec::new();
    let mut last = Point::ORIGIN;
    for element in elements {
        if output.len() == limit {
            return None;
        }
        // cap_delta is present only for a validated single straight source with an
        // even pattern whose on entries are ALL zero. Kurbo may represent such a run
        // with unequal endpoints from rounding; its semantic length is still zero.
        // Never classify a positive dash by a geometric near-zero threshold.
        let mut device_element = transform * element;
        if let (Some(delta), kurbo::PathEl::LineTo(_)) = (cap_delta, element) {
            device_element = kurbo::PathEl::LineTo(transform * last + delta);
        }
        if let kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) = element {
            last = p;
        }
        if !device_element.is_finite() {
            return None;
        }
        if output.len() == output.capacity() {
            output
                .try_reserve_exact((limit - output.len()).min(output.capacity().max(1024)))
                .ok()?;
        }
        output.push(device_element);
    }
    Some(BezPath::from_vec(output))
}

/// Kition PDF patch: the length of `path`'s control polygon, which a curve never exceeds, and how
/// many times kurbo restarts a dash pattern along it (each `MoveTo` and `ClosePath`).
fn control_polygon_length(path: &BezPath) -> (f64, usize) {
    let (mut length, mut restarts) = (0.0, 0usize);
    let (mut start, mut last) = (Point::ORIGIN, Point::ORIGIN);
    for el in path.elements() {
        match *el {
            kurbo::PathEl::MoveTo(p) => {
                (start, last) = (p, p);
                restarts = restarts.saturating_add(1);
            }
            kurbo::PathEl::LineTo(p) => {
                length += last.distance(p);
                last = p;
            }
            kurbo::PathEl::QuadTo(c, p) => {
                length += last.distance(c) + c.distance(p);
                last = p;
            }
            kurbo::PathEl::CurveTo(c1, c2, p) => {
                length += last.distance(c1) + c1.distance(c2) + c2.distance(p);
                last = p;
            }
            kurbo::PathEl::ClosePath => {
                length += last.distance(start);
                last = start;
                restarts = restarts.saturating_add(1);
            }
        }
    }
    (length, restarts)
}

/// Kition PDF patch: how far, in user space, drawing may place geometry outside a path's exact
/// outline. vello expands strokes to a tolerance of at most 0.25 user units (`TOL / max(|a|, |d|,
/// 1)` in vello_common's `flatten::stroke`), and kurbo approximates round joins and caps to 1e-3.
/// Fills are flattened into chords, which stay inside the outline's bounds.
const DRAW_SLACK: f64 = 0.5;

thread_local! {
    /// Kition PDF patch: whether [`may_paint_canvas`] may skip paths on this thread. Turned off
    /// only by Kition PDF's regression test, which renders each tile with and without skipping.
    static SKIP_OFFSCREEN_PATHS: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Kition PDF patch: exported for its regression test (see [`SKIP_OFFSCREEN_PATHS`]).
#[doc(hidden)]
pub fn set_skip_offscreen_paths(skip: bool) {
    SKIP_OFFSCREEN_PATHS.with(|s| s.set(skip));
}

/// Kition PDF patch: whether a path drawn with `transform` can paint a pixel of a `width` ×
/// `height` canvas. `reach` is how far its paint extends beyond the path, in user space: 0 for a
/// fill; for a stroke, half its width times the larger of its miter limit (kurbo draws a miter
/// only when its tip is within `limit × width / 2` of the vertex) and √2 (square caps). The box
/// of the path's points and control points (a curve stays inside their hull) grown by that and
/// [`DRAW_SLACK`], mapped to the device and grown by 2 pixels for antialiasing, contains
/// everything it paints: a path whose box misses the canvas is skipped without changing a pixel,
/// so a tile of a large page no longer strokes and fills every path on the page. Bounds that
/// aren't finite (an empty path) are drawn as before.
fn may_paint_canvas(
    path: &BezPath,
    transform: Affine,
    reach: f64,
    width: u16,
    height: u16,
) -> bool {
    if !SKIP_OFFSCREEN_PATHS.with(std::cell::Cell::get) {
        return true;
    }
    let mut points = Rect::new(
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut add = |p: Point| points = points.union_pt(p);
    for el in path.elements() {
        match *el {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => add(p),
            kurbo::PathEl::QuadTo(c, p) => {
                add(c);
                add(p);
            }
            kurbo::PathEl::CurveTo(c1, c2, p) => {
                add(c1);
                add(c2);
                add(p);
            }
            kurbo::PathEl::ClosePath => {}
        }
    }
    let grow = reach + DRAW_SLACK;
    let device = transform
        .transform_rect_bbox(points.inflate(grow, grow))
        .inflate(2.0, 2.0);
    let finite = [device.x0, device.y0, device.x1, device.y1]
        .iter()
        .all(|v| v.is_finite());
    !finite
        || (device.x0 < f64::from(width)
            && device.x1 > 0.0
            && device.y0 < f64::from(height)
            && device.y1 > 0.0)
}

pub(crate) struct Renderer {
    pub(crate) ctx: RenderContext,
    pub(crate) inside_pattern: bool,
    pub(crate) is_page_root: bool,
    pub(crate) soft_mask_cache: HashMap<u128, Mask>,
    pub(crate) outline_cache: Rc<std::cell::RefCell<HashMap<u128, Rc<BezPath>>>>,
    pub(crate) cur_mask: Option<Mask>,
    pub(crate) in_type3_glyph: bool,
    pub(crate) scaler: Scaler,
    // TODO: Remove this once vello_cpu bug with non-transparent images is fixed
    image_transparency_stack: Vec<bool>,
}

#[derive(Clone, Copy)]
enum ImagePixelFormat {
    Luma,
    Rgb,
    Rgba,
}

impl Renderer {
    pub(crate) fn new(
        width: u16,
        height: u16,
        settings: RenderSettings,
        cache: &RenderCache<'_>,
    ) -> Self {
        Self {
            ctx: RenderContext::new_with(width, height, settings),
            inside_pattern: false,
            is_page_root: false,
            soft_mask_cache: HashMap::default(),
            outline_cache: cache.outline_cache.clone(),
            cur_mask: None,
            in_type3_glyph: false,
            scaler: Scaler::new(ResamplingFunction::CatmullRom),
            image_transparency_stack: Vec::new(),
        }
    }

    /// Kition PDF patch: returns the stroke's reach for [`may_paint_canvas`].
    fn set_stroke_properties(
        &mut self,
        stroke_props: &StrokeProps,
        is_text: bool,
        path: &BezPath,
    ) -> f64 {
        let threshold = if is_text { 0.25 } else { 1.0 };

        // Best-effort attempt to ensure a line width of at least 1.0, as required by the PDF
        // specification. If we are stroking text, we reduce the threshold as it will otherwise
        // lead to very bold-looking text at low resolutions.
        let min_factor = max_factor(self.ctx.transform());
        let mut line_width = stroke_props.line_width.max(0.01);
        let transformed_width = line_width * min_factor;

        // Only enforce line width if not inside of pattern or type 3 glyph.
        if transformed_width < threshold && !self.inside_pattern && !self.in_type3_glyph {
            line_width /= transformed_width;
            line_width *= threshold;
        }

        // Kition PDF patch: a stroke far wider than the canvas looks the same as one a few canvases
        // wide, but its geometry grows with the width: a fuzzed `/LW 9223372036854775807`
        // allocated 10 GB in stroke expansion.
        let widest = 4.0 * (f32::from(self.ctx.width()) + f32::from(self.ctx.height()));
        if min_factor.is_finite() && min_factor > 0.0 && line_width * min_factor > widest {
            line_width = widest / min_factor;
        }

        // Preserve the previous zero-entry approximation outside the device hairline path.
        // Parsing now retains exact zeros so eligible hairlines can keep dash positions.
        let legacy_dashes = stroke_props.dash_array.contains(&0.0).then(|| {
            let mut dashes = stroke_props.dash_array.clone();
            for dash in &mut dashes {
                if *dash == 0.0 {
                    *dash = 0.01;
                }
            }
            dashes
        });
        let dash_array = legacy_dashes
            .as_deref()
            .unwrap_or(stroke_props.dash_array.as_slice());

        let stroke = kurbo::Stroke {
            width: line_width as f64,
            join: stroke_props.line_join,
            miter_limit: stroke_props.miter_limit as f64,
            start_cap: stroke_props.line_cap,
            end_cap: stroke_props.line_cap,
            // Kition PDF patch: see `usable_dash_pattern`.
            dash_pattern: usable_dash_pattern(path, dash_array).into(),
            dash_offset: if stroke_props.dash_offset.is_finite() {
                stroke_props.dash_offset as f64
            } else {
                0.0
            },
        };

        // A miter limit that isn't finite leaves miters unbounded: never skipped.
        let miter = stroke.miter_limit.abs();
        let reach = stroke.width / 2.0
            * if miter.is_finite() {
                miter.max(std::f64::consts::SQRT_2)
            } else {
                f64::INFINITY
            };
        self.ctx.set_stroke(stroke);
        reach
    }

    fn draw_image_with_alpha_mask(&mut self, image_data: ImageData, alpha_data: LumaData) {
        // Kition PDF patch: mismatched masks need a dummy RGB staging image. Validate and
        // reserve it before creating/pushing any mask, so failure skips the whole image.
        let Some(len) = source_byte_len(alpha_data.width, alpha_data.height, 3, MAX_IMAGE_PIXELS)
        else {
            log::warn!(
                "Image alpha-mask staging dimensions exceeded the buffer limit; image skipped"
            );
            return;
        };
        let Some(mut dummy_rgb) = image_buffer(len) else {
            log::warn!("Image alpha-mask staging buffer could not be allocated; image skipped");
            return;
        };
        dummy_rgb.resize(len, 0);
        let mask = {
            let transform = *self.ctx.transform()
                * Affine::scale_non_uniform(
                    image_data.width() as f64 / alpha_data.width as f64,
                    image_data.height() as f64 / alpha_data.height as f64,
                );
            let mut renderer = Self {
                ctx: RenderContext::new_with(
                    self.ctx.width(),
                    self.ctx.height(),
                    derive_settings(self.ctx.render_settings()),
                ),
                inside_pattern: false,
                is_page_root: false,
                soft_mask_cache: HashMap::default(),
                outline_cache: self.outline_cache.clone(),
                cur_mask: None,
                in_type3_glyph: false,
                scaler: self.scaler,
                image_transparency_stack: Vec::new(),
            };
            let mut mask_pix = Pixmap::new(self.ctx.width(), self.ctx.height());
            let rgb_data = ImageData::Rgb(RgbData {
                data: dummy_rgb,
                width: alpha_data.width,
                height: alpha_data.height,
                interpolate: alpha_data.interpolate,
                scale_factors: alpha_data.scale_factors,
            });
            renderer.ctx.set_transform(transform);
            // Note that there is a circle between `draw_image` and `draw_image_with_alpha_mask`,
            // but `draw_image_with_alpha_mask` is only called if the dimensions or interpolate
            // values between alpha_data and rgb_data don't match, which they do here.
            renderer.draw_image(rgb_data, Some(alpha_data));
            renderer.ctx.flush();
            let mut resources = vello_cpu::Resources::default();
            renderer.ctx.render_to_pixmap(&mut resources, &mut mask_pix);
            Mask::new_alpha(&mask_pix)
        };

        self.ctx.push_mask_layer(mask);
        self.image_transparency_stack.push(true);
        self.draw_image(image_data, None);
        self.image_transparency_stack.pop();
        self.ctx.pop_layer();
    }

    fn resize_image_data(
        &self,
        data: Vec<u8>,
        src_width: u32,
        src_height: u32,
        new_width: u32,
        new_height: u32,
        pixel_format: ImagePixelFormat,
    ) -> (Vec<u8>, u32, u32) {
        let resized = match pixel_format {
            ImagePixelFormat::Luma => self.resize_image_data_impl::<1>(
                data,
                src_width,
                src_height,
                new_width,
                new_height,
                |scaler, source_size, target_size| {
                    scaler.plan_planar_resampling(source_size, target_size)
                },
            ),
            ImagePixelFormat::Rgb => self.resize_image_data_impl::<3>(
                data,
                src_width,
                src_height,
                new_width,
                new_height,
                |scaler, source_size, target_size| {
                    scaler.plan_rgb_resampling(source_size, target_size)
                },
            ),
            ImagePixelFormat::Rgba => self.resize_image_data_impl::<4>(
                data,
                src_width,
                src_height,
                new_width,
                new_height,
                |scaler, source_size, target_size| {
                    scaler.plan_rgba_resampling(source_size, target_size, true)
                },
            ),
        };
        match resized {
            Ok(out) => (out, new_width, new_height),
            Err(original) => {
                log::warn!(
                    "Image resampling failed or exceeded its buffer limit; falling back to source data"
                );
                (original, src_width, src_height)
            }
        }
    }

    fn resize_image_data_impl<const N: usize>(
        &self,
        data: Vec<u8>,
        src_width: u32,
        src_height: u32,
        new_width: u32,
        new_height: u32,
        plan: impl FnOnce(
            &Scaler,
            ImageSize,
            ImageSize,
        ) -> Result<Arc<Resampling<u8, N>>, PicScaleError>,
    ) -> Result<Vec<u8>, Vec<u8>> {
        // Kition PDF patch: validate before constructing a plan or allocating its output/scratch.
        // Returning the owned input preserves both its pixels and dimensions on any failure.
        let resized = (|| {
            let len = resampling_byte_len(
                (src_width, src_height),
                (new_width, new_height),
                N,
                data.len(),
                MAX_IMAGE_PIXELS,
            )?;
            let source_size = ImageSize::new(src_width as usize, src_height as usize);
            let target_size = ImageSize::new(new_width as usize, new_height as usize);
            let src = ImageStore::<u8, N>::from_slice(&data, source_size.width, source_size.height)
                .ok()?;
            let plan = plan(&self.scaler, source_size, target_size).ok()?;
            let scratch_len = plan.scratch_size();
            let scratch_bound = resampling_scratch_byte_len(
                (src_width, src_height),
                (new_width, new_height),
                N,
                MAX_IMAGE_PIXELS,
            )?;
            if scratch_len > scratch_bound {
                return None;
            }
            let mut out = image_buffer(len)?;
            out.resize(len, 0);
            let mut dst =
                ImageStoreMut::<u8, N>::from_slice(&mut out, target_size.width, target_size.height)
                    .ok()?;
            let mut scratch = image_buffer(scratch_len)?;
            scratch.resize(scratch_len, 0);
            plan.resample_with_scratch(&src, &mut dst, &mut scratch)
                .ok()?;
            Some(out)
        })();
        match resized {
            Some(out) => Ok(out),
            None => Err(data),
        }
    }

    fn draw_image(&mut self, image_data: ImageData, alpha_data: Option<LumaData>) {
        let cur_transform = *self.ctx.transform();
        let (x_scale, y_scale) = {
            let (x, y) = x_y_advances(&cur_transform);
            (x.length() as f32, y.length() as f32)
        };
        // Kition PDF patch: do not send non-finite or collapsed geometry to the rasterizer.
        if !cur_transform.as_coeffs().iter().all(|v| v.is_finite())
            || !x_scale.is_finite()
            || !y_scale.is_finite()
            || x_scale <= 0.0
            || y_scale <= 0.0
        {
            log::warn!("Image has an invalid transform and was skipped");
            return;
        }
        let (source_width, source_height) = (image_data.width(), image_data.height());
        let channels = match &image_data {
            ImageData::Luma(_) => 1,
            ImageData::Rgb(_) => 3,
        };
        let data_len = match &image_data {
            ImageData::Luma(a) => a.data.len(),
            ImageData::Rgb(a) => a.data.len(),
        };
        if source_byte_len(source_width, source_height, channels, MAX_IMAGE_PIXELS)
            != Some(data_len)
        {
            log::warn!("Image has invalid or over-limit source dimensions/data and was skipped");
            return;
        }
        let interpolate = image_data.interpolate();
        if let Some(a) = &alpha_data {
            if source_byte_len(a.width, a.height, 1, MAX_IMAGE_PIXELS) != Some(a.data.len()) {
                log::warn!("Image has invalid or over-limit alpha dimensions/data and was skipped");
                return;
            }
            if a.width != source_width || a.height != source_height || a.interpolate != interpolate
            {
                if let Some(alpha) = alpha_data {
                    self.draw_image_with_alpha_mask(image_data, alpha);
                }
                return;
            }
        }
        let mut quality = if interpolate {
            ImageQuality::Medium
        } else {
            ImageQuality::Low
        };
        let has_alpha = alpha_data.is_some();
        let mut may_have_transparency = has_alpha || self.force_images_may_have_transparency();
        let requested_resize = x_scale < 1.0 || y_scale < 1.0;
        let (new_width, new_height) = match image_resampling_size(
            source_width,
            source_height,
            channels,
            data_len,
            x_scale,
            y_scale,
            MAX_IMAGE_PIXELS,
        ) {
            Some(target) => target,
            None => {
                if image_byte_len(source_width, source_height, 4, MAX_IMAGE_PIXELS).is_none() {
                    log::warn!(
                        "Image resampling exceeded its buffer limit and the original resolution cannot fit the backend; image skipped"
                    );
                    return;
                }
                log::warn!(
                    "Image resampling exceeded its buffer limit; retaining the original resolution"
                );
                (source_width, source_height)
            }
        };
        if requested_resize && self.in_type3_glyph {
            quality = ImageQuality::High;
        }
        let needs_resize = (new_width, new_height) != (source_width, source_height);

        // Preserve the single-channel/RGB fast paths. With alpha, expand directly to RGBA;
        // every source and alpha length was validated before zipping or allocating.
        let (data, format) = match (image_data, alpha_data) {
            (ImageData::Luma(luma), None) => (luma.data, ImagePixelFormat::Luma),
            (ImageData::Rgb(rgb), None) => (rgb.data, ImagePixelFormat::Rgb),
            (image, Some(alpha)) => {
                let Some(len) = source_byte_len(source_width, source_height, 4, MAX_IMAGE_PIXELS)
                else {
                    return;
                };
                let Some(mut rgba) = image_buffer(len) else {
                    log::warn!(
                        "Image RGBA buffer could not be allocated and the image was skipped"
                    );
                    return;
                };
                match image {
                    ImageData::Luma(luma) => {
                        rgba.extend(
                            luma.data
                                .iter()
                                .zip(alpha.data)
                                .flat_map(|(g, a)| [*g, *g, *g, a]),
                        );
                    }
                    ImageData::Rgb(rgb) => {
                        rgba.extend(
                            rgb.data
                                .as_chunks::<3>()
                                .0
                                .iter()
                                .zip(alpha.data)
                                .flat_map(|([r, g, b], a)| [*r, *g, *b, a]),
                        );
                    }
                }
                (rgba, ImagePixelFormat::Rgba)
            }
        };
        let (data, mut img_width, mut img_height) = if needs_resize {
            self.resize_image_data(
                data,
                source_width,
                source_height,
                new_width,
                new_height,
                format,
            )
        } else {
            (data, source_width, source_height)
        };
        if image_byte_len(img_width, img_height, 4, MAX_IMAGE_PIXELS).is_none() {
            log::warn!(
                "Image resampling failed and the original resolution cannot fit the backend; image skipped"
            );
            return;
        }
        let mut additional_transform = Affine::scale_non_uniform(
            source_width as f64 / img_width as f64,
            source_height as f64 / img_height as f64,
        );
        let mut rgba_data = if matches!(format, ImagePixelFormat::Rgba) {
            data
        } else {
            let Some(len) = image_byte_len(img_width, img_height, 4, MAX_IMAGE_PIXELS) else {
                return;
            };
            let Some(mut rgba) = image_buffer(len) else {
                log::warn!("Image RGBA buffer could not be allocated and the image was skipped");
                return;
            };
            match format {
                ImagePixelFormat::Luma => rgba.extend(data.iter().flat_map(|g| [*g, *g, *g, 255])),
                ImagePixelFormat::Rgb => rgba.extend(
                    data.as_chunks::<3>()
                        .0
                        .iter()
                        .flat_map(|[r, g, b]| [*r, *g, *b, 255]),
                ),
                ImagePixelFormat::Rgba => {} // Handled above.
            }
            rgba
        };
        if has_alpha {
            let (chunks, _) = rgba_data.as_chunks_mut::<4>();
            for chunk in chunks {
                let [r, g, b, a] = *chunk;
                *chunk = AlphaColor::from_rgba8(r, g, b, a)
                    .premultiply()
                    .to_rgba8()
                    .to_u8_array();
            }
        }
        // Type 3 glyphs get a transparent two-pixel frame for interpolation. Check the padded
        // RGBA allocation and u16 dimensions too, before narrowing or growing either side.
        if self.in_type3_glyph {
            let (Some(width), Some(height)) = (img_width.checked_add(4), img_height.checked_add(4))
            else {
                return;
            };
            let Some(len) = image_byte_len(width, height, 4, MAX_IMAGE_PIXELS) else {
                log::warn!(
                    "Image glyph padding exceeded its buffer limit and the image was skipped"
                );
                return;
            };
            let Some(mut padded) = image_buffer(len) else {
                log::warn!("Image glyph padding could not be allocated and the image was skipped");
                return;
            };
            let row_len = width as usize * 4; // Checked RGBA length above bounds every row.
            padded.resize(row_len * 2, 0);
            for row in rgba_data.chunks_exact(img_width as usize * 4) {
                padded.extend([0; 8]);
                padded.extend_from_slice(row);
                padded.extend([0; 8]);
            }
            padded.resize(len, 0);
            (img_width, img_height) = (width, height);
            additional_transform *= Affine::translate((-2.0, -2.0));
            may_have_transparency = true;
            rgba_data = padded;
        }
        let pixmap = Pixmap::from_parts_with_opacity(
            bytemuck::cast_vec(rgba_data),
            img_width as u16,
            img_height as u16,
            may_have_transparency,
        );
        self.draw_pixmap(
            Arc::new(pixmap),
            quality,
            cur_transform * additional_transform,
        );
    }

    fn push_clip_path_inner(&mut self, clip_path: &BezPath, fill: FillRule) {
        let old_transform = *self.ctx.transform();

        self.ctx.set_fill_rule(convert_fill_rule(fill));
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.push_clip_path(clip_path);

        self.ctx.set_transform(old_transform);
    }

    fn draw_pixmap(&mut self, pixmap: Arc<Pixmap>, quality: ImageQuality, transform: Affine) {
        let (width, height) = (pixmap.width(), pixmap.height());
        let image = Image {
            image: ImageSource::Pixmap(pixmap),
            sampler: ImageSampler {
                x_extend: peniko::Extend::Pad,
                y_extend: peniko::Extend::Pad,
                quality,
                alpha: 1.0,
            },
        };

        self.ctx.set_transform(transform);
        self.ctx.set_paint(image);
        self.ctx
            .fill_rect(&Rect::new(0.0, 0.0, width as f64, height as f64));
    }

    #[must_use]
    fn set_paint(
        &mut self,
        paint: &Paint<'_>,
        path: &BezPath,
        is_stroke: bool,
    ) -> (Option<BezPath>, bool) {
        let mut paint_transform = Affine::IDENTITY;
        let mut clip_path = None;
        let mut skip_fill = false;

        let paint: PaintType = match paint.clone() {
            Paint::Color(c) => {
                let c = c.to_rgba().to_rgba8();
                AlphaColor::from_rgba8(c[0], c[1], c[2], c[3]).into()
            }
            Paint::Pattern(p) => {
                let path_transform = self.ctx.transform();

                match *p {
                    Pattern::Shading(s) => 'shading_paint: {
                        clip_path = s.shading.clip_path.clone();
                        let mut bbox = (*path_transform * path.clone()).bounding_box();

                        if is_stroke {
                            // Try to account for stroke in bbox.
                            let (a1, a2) = x_y_advances(path_transform);
                            let factor = a1.length().max(a2.length()) * self.ctx.stroke().width;
                            bbox = bbox.inflate(factor, factor);
                        }

                        bbox = bbox.intersect(Rect::new(
                            0.0,
                            0.0,
                            self.ctx.width() as f64,
                            self.ctx.height() as f64,
                        ));

                        // Kition PDF patch: mesh shadings are sampled only inside `bbox` (and a
                        // pixel around it: texels at a fractional edge look one pixel further).
                        let encoded = s.encode_within(Some(bbox.inflate(1.0, 1.0)));
                        // Kition PDF patch: at the explicit page root, an all-zero source
                        // under ordinary SrcOver is an identity paint. Keep state and clip
                        // handling, but avoid queuing repeated transparent full-page fills.
                        // Nested groups, masks, patterns, Type3 and strokes keep image paints.
                        skip_fill = encoded.constant_sampled_color().is_some_and(|sample| {
                            self.can_skip_transparent_mesh_fill(
                                is_stroke,
                                AlphaColor::<Srgb>::new(sample).premultiply().to_rgba8(),
                            )
                        });
                        if self.can_optimize_root_mesh_fill(is_stroke)
                            && let Some(sample) = encoded.constant_sampled_color()
                        {
                            let pixel = AlphaColor::<Srgb>::new(sample).premultiply().to_rgba8();
                            if pixel.a == 255 {
                                // Preserve image-byte quantization, never the original float
                                // sample. Other contexts retain their existing image handling.
                                break 'shading_paint AlphaColor::from_rgba8(
                                    pixel.r, pixel.g, pixel.b, pixel.a,
                                )
                                .into();
                            }
                        }
                        let (image, width, height, transform, may_have_transparency) =
                            render_shading_texture(bbox, &encoded);
                        let may_have_transparency =
                            may_have_transparency || self.force_images_may_have_transparency();
                        paint_transform = path_transform.inverse() * transform;

                        let pixmap = Pixmap::from_parts_with_opacity(
                            image,
                            width as u16,
                            height as u16,
                            may_have_transparency,
                        );

                        let image = Image {
                            image: ImageSource::Pixmap(Arc::new(pixmap)),
                            sampler: ImageSampler {
                                x_extend: peniko::Extend::Repeat,
                                y_extend: peniko::Extend::Repeat,
                                quality: ImageQuality::Medium,
                                alpha: 1.0,
                            },
                        };

                        PaintType::Image(image)
                    }
                    Pattern::Tiling(t) => {
                        const MAX_PIXMAP_SIZE: f32 = 3000.0;
                        // TODO: Raise this limit and perform downsampling if reached
                        // (see pdftc_100k_0138.pdf).
                        const MIN_PIXMAP_SIZE: f32 = 1.0;

                        let bbox = t.bbox;
                        let max_x_scale = MAX_PIXMAP_SIZE / bbox.width() as f32;
                        let min_x_scale = MIN_PIXMAP_SIZE / bbox.width() as f32;
                        let max_y_scale = MAX_PIXMAP_SIZE / bbox.height() as f32;
                        let min_y_scale = MIN_PIXMAP_SIZE / bbox.height() as f32;

                        let (mut xs, mut ys) = {
                            let (x, y) = x_y_advances(&(t.matrix));
                            (x.length() as f32, y.length() as f32)
                        };
                        xs = xs.max(min_x_scale).min(max_x_scale);
                        ys = ys.max(min_y_scale).min(max_y_scale);
                        // Kition PDF patch: the cell pixmap is `step × scale` pixels, which only the
                        // u16 cast below bounded (a fuzzed /XStep far beyond /BBox gave 65535² cells).
                        xs = tiling_cell_scale(xs, t.x_step, MAX_PIXMAP_SIZE);
                        ys = tiling_cell_scale(ys, t.y_step, MAX_PIXMAP_SIZE);

                        let x_step = xs * t.x_step;
                        let y_step = ys * t.y_step;

                        let scaled_width = bbox.width() as f32 * xs;
                        let scaled_height = bbox.height() as f32 * ys;
                        let pix_width = x_step.abs().round() as u16;
                        let pix_height = y_step.abs().round() as u16;

                        let mut renderer = Self {
                            ctx: RenderContext::new_with(
                                pix_width,
                                pix_height,
                                derive_settings(self.ctx.render_settings()),
                            ),
                            cur_mask: None,
                            inside_pattern: true,
                            is_page_root: false,
                            soft_mask_cache: HashMap::default(),
                            outline_cache: self.outline_cache.clone(),
                            in_type3_glyph: false,
                            scaler: self.scaler,
                            image_transparency_stack: Vec::new(),
                        };
                        let mut initial_transform = Affine::scale_non_uniform(xs as f64, ys as f64)
                            * Affine::translate((-bbox.x0, -bbox.y0));
                        t.interpret(&mut renderer, initial_transform, is_stroke);
                        let mut pix = Pixmap::new(pix_width, pix_height);
                        renderer.ctx.flush();
                        let mut resources = vello_cpu::Resources::default();
                        renderer.ctx.render_to_pixmap(&mut resources, &mut pix);

                        // TODO: Fix these
                        if x_step < 0.0 {
                            initial_transform *=
                                Affine::new([-1.0, 0.0, 0.0, 1.0, scaled_width as f64, 0.0]);
                        }

                        if y_step < 0.0 {
                            initial_transform *=
                                Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, scaled_height as f64]);
                        }

                        paint_transform =
                            path_transform.inverse() * t.matrix * initial_transform.inverse();

                        let image = Image {
                            image: ImageSource::Pixmap(Arc::new(pix)),
                            sampler: ImageSampler {
                                x_extend: peniko::Extend::Repeat,
                                y_extend: peniko::Extend::Repeat,
                                quality: ImageQuality::Medium,
                                alpha: 1.0,
                            },
                        };

                        PaintType::Image(image)
                    }
                }
            }
        };

        self.ctx.set_paint_transform(paint_transform);
        self.ctx.set_paint(paint);

        (clip_path, skip_fill)
    }

    fn stroke_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'_>,
        stroke_props: &StrokeProps,
        is_text: bool,
    ) {
        self.ctx.set_transform(transform);
        // Kition PDF patch: ordinary zero-width color strokes have one device-pixel
        // width. Split dashes in user space first, preserving arc lengths and phase.
        if stroke_props.line_width == 0.0
            && !is_text
            && !self.inside_pattern
            && !self.in_type3_glyph
            && matches!(paint, Paint::Color(_))
        {
            if !transform.as_coeffs().iter().all(|value| value.is_finite()) || !path.is_finite() {
                return;
            }
            let dashes = usable_dash_pattern(path, &stroke_props.dash_array);
            let device_path = if dashes.is_empty() {
                Some(transform * path.clone())
            } else if path.elements().len() <= MAX_DASH_HAIRLINE_INPUT_ELEMENTS {
                // Cull against the transformed undashed centerline before subdivision.
                // Device-space reach is sound even for singular/anisotropic transforms.
                let miter = f64::from(stroke_props.miter_limit).abs();
                let reach = if miter.is_finite() {
                    0.5 * miter.max(std::f64::consts::SQRT_2)
                } else {
                    f64::INFINITY
                };
                let centerline = transform * path.clone();
                if !centerline.is_finite() {
                    return;
                }
                if !may_paint_canvas(
                    &centerline,
                    Affine::IDENTITY,
                    reach,
                    self.ctx.width(),
                    self.ctx.height(),
                ) {
                    return;
                }
                drop(centerline);
                device_dash_path(
                    path,
                    transform,
                    f64::from(stroke_props.dash_offset),
                    &dashes,
                    stroke_props.line_cap,
                )
            } else {
                None
            };
            if let Some(device_path) = device_path {
                if !device_path.is_finite() {
                    return;
                }
                let mut device_stroke = stroke_props.clone();
                device_stroke.line_width = 1.0;
                device_stroke.dash_array.clear();
                device_stroke.dash_offset = 0.0;
                // One combined stroke retains opacity at intersections. Positive width
                // bypasses this branch; existing clips are already in device space.
                self.stroke_path(&device_path, Affine::IDENTITY, paint, &device_stroke, false);
                self.ctx.set_transform(transform);
                return;
            }
            // Excess input or failed preparation retains the previous dashed behavior.
        }
        let reach = self.set_stroke_properties(stroke_props, is_text, path);
        // Kition PDF patch: see `may_paint_canvas`.
        if !may_paint_canvas(path, transform, reach, self.ctx.width(), self.ctx.height()) {
            return;
        }

        let (clip_path, _) = self.set_paint(paint, path, true);
        if let Some(clip_path) = clip_path.as_ref() {
            self.push_clip_path_inner(clip_path, FillRule::NonZero);
        }
        self.ctx.stroke_path(path);
        if clip_path.is_some() {
            self.ctx.pop_clip_path();
        }
    }

    fn fill_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'_>,
        fill_rule: FillRule,
    ) {
        self.ctx.set_fill_rule(convert_fill_rule(fill_rule));
        self.ctx.set_transform(transform);
        // Kition PDF patch: see `may_paint_canvas`.
        if !may_paint_canvas(path, transform, 0.0, self.ctx.width(), self.ctx.height()) {
            return;
        }

        let (clip_path, skip_fill) = self.set_paint(paint, path, false);
        if let Some(clip_path) = clip_path.as_ref() {
            self.push_clip_path_inner(clip_path, fill_rule);
        }

        if !skip_fill {
            self.ctx.fill_path(path);
        }

        if clip_path.is_some() {
            self.ctx.pop_clip_path();
        }
    }

    fn fill_glyph<'a>(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'a>,
    ) {
        match glyph {
            Glyph::Outline(o) => {
                let base_outline = self.cached_outline(o);

                self.fill_path(
                    base_outline.as_ref(),
                    transform * glyph_transform,
                    paint,
                    FillRule::NonZero,
                );
            }
            Glyph::Type3(s) => {
                let was_page_root = self.is_page_root;
                self.is_page_root = false;
                self.in_type3_glyph = true;
                s.interpret(self, transform, glyph_transform, paint);
                self.in_type3_glyph = false;
                self.is_page_root = was_page_root;
            }
        }
    }

    fn stroke_glyph<'a>(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'a>,
        stroke_props: &StrokeProps,
    ) {
        match glyph {
            Glyph::Outline(o) => {
                let base_outline = self.cached_outline(o);

                self.stroke_path(
                    &(glyph_transform * base_outline.as_ref().clone()),
                    transform,
                    paint,
                    stroke_props,
                    true,
                );
            }
            Glyph::Type3(s) => {
                let was_page_root = self.is_page_root;
                self.is_page_root = false;
                s.interpret(self, transform, glyph_transform, paint);
                self.is_page_root = was_page_root;
            }
        }
    }

    fn cached_outline(&self, glyph: &hayro_interpret::font::OutlineGlyph) -> Rc<BezPath> {
        let id = glyph.identifier().cache_key();

        if let Some(path) = self.outline_cache.borrow().get(&id) {
            return path.clone();
        }

        let path = Rc::new(glyph.outline());
        self.outline_cache.borrow_mut().insert(id, path.clone());
        path
    }

    fn can_optimize_root_mesh_fill(&self, is_stroke: bool) -> bool {
        !is_stroke
            && self.is_page_root
            && self.image_transparency_stack.len() == 1
            && self.cur_mask.is_none()
            && !self.inside_pattern
            && !self.in_type3_glyph
            && self.ctx.blend_mode() == peniko::BlendMode::default()
    }

    fn can_skip_transparent_mesh_fill(&self, is_stroke: bool, pixel: PremulRgba8) -> bool {
        self.can_optimize_root_mesh_fill(is_stroke)
            // Invalid transfer output can leave nonzero RGB even when alpha rounds
            // down to zero; require all four quantized channels to vanish.
            && pixel.to_u8_array() == [0; 4]
    }

    fn force_images_may_have_transparency(&self) -> bool {
        self.cur_mask.is_some()
            || self
                .image_transparency_stack
                .last()
                .copied()
                .unwrap_or(false)
            || self.ctx.blend_mode() != peniko::BlendMode::default()
    }
}

impl<'a> Device<'a> for Renderer {
    fn draw_image(&mut self, image: hayro_interpret::Image<'a, '_>, mut transform: Affine) {
        self.ctx.set_paint_transform(Affine::IDENTITY);
        self.ctx.set_aliasing_threshold(Some(1));

        let target_width = (transform * Point::new(image.width() as f64, 0.0))
            .to_vec2()
            .length()
            .ceil() as u32;
        let target_height = (transform * Point::new(0.0, image.height() as f64))
            .to_vec2()
            .length()
            .ceil() as u32;

        match image {
            hayro_interpret::Image::Stencil(s) => {
                s.with_stencil(
                    |stencil, paint| {
                        transform *= Affine::scale_non_uniform(
                            stencil.scale_factors.0 as f64,
                            stencil.scale_factors.1 as f64,
                        );

                        match paint {
                            Paint::Color(c) => {
                                let color = c.to_rgba().to_rgba8();
                                let (rgb_bytes, alpha) = (
                                    stencil
                                        .data
                                        .iter()
                                        .flat_map(|_| [color[0], color[1], color[2]])
                                        .collect::<Vec<u8>>(),
                                    color[3],
                                );

                                let blend_mode = self.ctx.blend_mode();
                                let push_layer =
                                    alpha != 255 || blend_mode != peniko::BlendMode::default();
                                self.ctx.set_transform(transform);
                                if push_layer {
                                    self.ctx.push_layer(
                                        None,
                                        Some(blend_mode),
                                        Some(alpha as f32 / 255.0),
                                        None,
                                        None,
                                    );
                                }
                                let old_rule = *self.ctx.fill_rule();
                                self.ctx.set_fill_rule(Fill::NonZero);

                                let rgb_data = ImageData::Rgb(RgbData {
                                    data: rgb_bytes,
                                    width: stencil.width,
                                    height: stencil.height,
                                    interpolate: stencil.interpolate,
                                    scale_factors: stencil.scale_factors,
                                });
                                self.draw_image(rgb_data, Some(stencil));

                                if push_layer {
                                    self.ctx.pop_layer();
                                }

                                self.ctx.set_fill_rule(old_rule);
                            }
                            Paint::Pattern(_) => {
                                let (width, height) = (self.ctx.width(), self.ctx.height());
                                let stencil_rect = Rect::new(
                                    0.0,
                                    0.0,
                                    stencil.width as f64,
                                    stencil.height as f64,
                                );
                                let mask_pix = {
                                    let rgb_bytes = ImageData::Rgb(RgbData {
                                        data: vec![
                                            255;
                                            stencil.width as usize
                                                * stencil.height as usize
                                                * 3
                                        ],
                                        width: stencil.width,
                                        height: stencil.height,
                                        interpolate: stencil.interpolate,
                                        scale_factors: stencil.scale_factors,
                                    });
                                    let mut sub_renderer = Self {
                                        ctx: RenderContext::new_with(
                                            width,
                                            height,
                                            derive_settings(self.ctx.render_settings()),
                                        ),
                                        inside_pattern: false,
                                        is_page_root: false,
                                        soft_mask_cache: HashMap::default(),
                                        outline_cache: self.outline_cache.clone(),
                                        cur_mask: None,
                                        in_type3_glyph: false,
                                        scaler: self.scaler,
                                        image_transparency_stack: Vec::new(),
                                    };
                                    let mut sub_pix = Pixmap::new(width, height);
                                    sub_renderer.ctx.set_transform(transform);
                                    sub_renderer.draw_image(rgb_bytes, Some(stencil));
                                    sub_renderer.ctx.flush();
                                    let mut resources = vello_cpu::Resources::default();
                                    sub_renderer
                                        .ctx
                                        .render_to_pixmap(&mut resources, &mut sub_pix);
                                    sub_pix
                                };

                                self.ctx.push_layer(
                                    None,
                                    Some(self.ctx.blend_mode()),
                                    None,
                                    Some(Mask::new_luminance(&mask_pix)),
                                    None,
                                );
                                self.ctx.set_transform(transform);

                                let (clip_path, _) =
                                    self.set_paint(paint, &stencil_rect.to_path(0.1), true);
                                if let Some(clip_path) = clip_path.as_ref() {
                                    self.push_clip_path_inner(clip_path, FillRule::NonZero);
                                }
                                self.ctx.fill_rect(&stencil_rect);
                                if clip_path.is_some() {
                                    self.ctx.pop_clip_path();
                                }

                                self.ctx.pop_layer();
                            }
                        };
                    },
                    Some((target_width, target_height)),
                );
            }
            hayro_interpret::Image::Raster(r) => {
                r.with_rgba(
                    |image, alpha| {
                        let (sx, sy) = image.scale_factors();
                        transform *= Affine::scale_non_uniform(sx as f64, sy as f64);
                        self.ctx.set_transform(transform);
                        self.draw_image(image, alpha);
                    },
                    Some((target_width, target_height)),
                );
            }
        }

        self.ctx.set_aliasing_threshold(None);
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        self.push_clip_path_inner(&clip_path.path, clip_path.fill);
    }

    fn push_transparency_group(
        &mut self,
        opacity: f32,
        mask: Option<SoftMask<'_>>,
        blend_mode: BlendMode,
    ) {
        let settings = *self.ctx.render_settings();
        let force_images_may_have_transparency = mask.is_some()
            || blend_mode != BlendMode::Normal
            || self
                .image_transparency_stack
                .last()
                .copied()
                .unwrap_or(false);
        self.ctx.push_layer(
            None,
            Some(convert_blend_mode(blend_mode)),
            Some(opacity),
            // TODO: Deduplicate
            mask.map(|m| {
                let width = self.ctx.width();
                let height = self.ctx.height();

                self.soft_mask_cache
                    .entry(m.cache_key())
                    .or_insert_with(|| draw_soft_mask(&m, settings, width, height))
                    .clone()
            }),
            None,
        );
        self.image_transparency_stack
            .push(force_images_may_have_transparency);
    }

    fn pop_clip_path(&mut self) {
        self.ctx.pop_clip_path();
    }

    fn pop_transparency_group(&mut self) {
        self.image_transparency_stack.pop();
        self.ctx.pop_layer();
    }

    fn set_soft_mask(&mut self, mask: Option<SoftMask<'_>>) {
        let settings = *self.ctx.render_settings();
        self.cur_mask = mask.map(|m| {
            let width = self.ctx.width();
            let height = self.ctx.height();

            self.soft_mask_cache
                .entry(m.cache_key())
                .or_insert_with(|| draw_soft_mask(&m, settings, width, height))
                .clone()
        });
        if let Some(mask) = self.cur_mask.clone() {
            self.ctx.set_mask(mask);
        } else {
            self.ctx.reset_mask();
        }
    }

    fn draw_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &PathDrawMode,
    ) {
        match draw_mode {
            PathDrawMode::Fill(f) => {
                Self::fill_path(self, path, transform, paint, *f);
            }
            PathDrawMode::Stroke(s) => {
                Self::stroke_path(self, path, transform, paint, s, false);
            }
        }
    }

    fn draw_rect(
        &mut self,
        rect: &Rect,
        transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &PathDrawMode,
    ) {
        let path = rect.to_path(0.1);
        match draw_mode {
            PathDrawMode::Fill(fill_rule) => {
                self.ctx.set_fill_rule(convert_fill_rule(*fill_rule));
                self.ctx.set_transform(transform);
                // Kition PDF patch: see `may_paint_canvas`.
                if !may_paint_canvas(&path, transform, 0.0, self.ctx.width(), self.ctx.height()) {
                    return;
                }

                let (clip_path, skip_fill) = self.set_paint(paint, &path, false);
                if let Some(clip_path) = clip_path.as_ref() {
                    self.push_clip_path_inner(clip_path, *fill_rule);
                }

                if !skip_fill {
                    self.ctx.fill_rect(rect);
                }

                if clip_path.is_some() {
                    self.ctx.pop_clip_path();
                }
            }
            PathDrawMode::Stroke(s) => {
                Self::stroke_path(self, &path, transform, paint, s, false);
            }
        }
    }

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'a>,
        draw_mode: &GlyphDrawMode,
    ) {
        match draw_mode {
            GlyphDrawMode::Fill => {
                Self::fill_glyph(self, glyph, transform, glyph_transform, paint);
            }
            GlyphDrawMode::Stroke(s) => {
                Self::stroke_glyph(self, glyph, transform, glyph_transform, paint, s);
            }
            GlyphDrawMode::Invisible => {
                // Don't render invisible text for visual output
            }
        }
    }

    fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        self.ctx.set_blend_mode(convert_blend_mode(blend_mode));
    }
}

// TODO: Deduplicate with hayro-svg?
fn render_shading_texture(
    path_bbox: Rect,
    shading_pattern: &EncodedShadingPattern,
) -> (Vec<PremulRgba8>, u32, u32, Affine, bool) {
    // Kition PDF patch: keep the existing repeated-image and shape pipeline, but
    // avoid a full-page texture for every empty mesh after its budget is spent.
    if let Some(sample) = shading_pattern.constant_sampled_color() {
        let pixel = AlphaColor::<Srgb>::new(sample).premultiply().to_rgba8();
        return (
            vec![pixel],
            1,
            1,
            Affine::translate((path_bbox.x0, path_bbox.y0)),
            pixel.a != 255,
        );
    }

    let base_width = (path_bbox.width() as f32).max(1.0);
    let base_height = (path_bbox.height() as f32).max(1.0);

    let width = (base_width).ceil() as u32;
    let height = (base_height).ceil() as u32;

    let (x_advance, y_advance) = x_y_advances(&shading_pattern.base_transform);

    let mut buf = vec![PremulRgba8::from_u32(0); width as usize * height as usize];
    let mut start_point = shading_pattern.base_transform
        * Affine::translate((0.5, 0.5))
        * Point::new(path_bbox.x0, path_bbox.y0);
    let mut may_have_transparency = false;

    for row in buf.chunks_exact_mut(width as usize) {
        let mut point = start_point;

        for pixel in row {
            let sample = shading_pattern.sample(point);
            *pixel = AlphaColor::<Srgb>::new(sample).premultiply().to_rgba8();
            may_have_transparency |= pixel.a != 255;

            point += x_advance;
        }

        start_point += y_advance;
    }

    (
        buf,
        width,
        height,
        Affine::translate((path_bbox.x0, path_bbox.y0)),
        may_have_transparency,
    )
}

fn draw_soft_mask(mask: &SoftMask<'_>, settings: RenderSettings, width: u16, height: u16) -> Mask {
    let mut renderer = Renderer {
        ctx: RenderContext::new_with(width, height, derive_settings(&settings)),
        inside_pattern: false,
        is_page_root: false,
        cur_mask: None,
        soft_mask_cache: HashMap::default(),
        outline_cache: Rc::new(std::cell::RefCell::new(HashMap::new())),
        in_type3_glyph: false,
        scaler: Scaler::new(ResamplingFunction::CatmullRom),
        image_transparency_stack: Vec::new(),
    };

    let bg_color = mask.background_color().to_rgba();
    let apply_bg = bg_color.to_rgba8() != BLACK.to_rgba8().to_u8_array();

    if apply_bg {
        renderer
            .ctx
            .set_paint(AlphaColor::<Srgb>::new(bg_color.components()));
        renderer
            .ctx
            .fill_rect(&Rect::new(0.0, 0.0, width as f64, height as f64));
        renderer.ctx.push_layer(None, None, None, None, None);
    }

    mask.interpret(&mut renderer);

    if apply_bg {
        renderer.ctx.pop_layer();
    }

    let mut pix = Pixmap::new(width, height);
    renderer.ctx.flush();
    let mut resources = vello_cpu::Resources::default();
    renderer.ctx.render_to_pixmap(&mut resources, &mut pix);

    let mut rendered_mask = match mask.mask_type() {
        MaskType::Luminosity => Mask::new_luminance(&pix),
        MaskType::Alpha => Mask::new_alpha(&pix),
    };

    if let Some(transfer_function) = mask.transfer_function() {
        let mut map = Vec::new();

        for y in 0..rendered_mask.height() {
            for x in 0..rendered_mask.width() {
                map.push(
                    (transfer_function.apply(rendered_mask.sample(x, y) as f32 / 255.0) * 255.0
                        + 0.5) as u8,
                );
            }
        }

        rendered_mask = Mask::from_parts(map, rendered_mask.width(), rendered_mask.height());
    }

    rendered_mask
}

pub(crate) fn max_factor(transform: &Affine) -> f32 {
    let scale_skew_transform = {
        let c = transform.as_coeffs();
        Affine::new([c[0], c[1], c[2], c[3], 0.0, 0.0])
    };

    let x_advance = scale_skew_transform * Point::new(1.0, 0.0);
    let y_advance = scale_skew_transform * Point::new(0.0, 1.0);

    x_advance
        .to_vec2()
        .length()
        .max(y_advance.to_vec2().length()) as f32
}

pub(crate) fn x_y_advances(transform: &Affine) -> (Vec2, Vec2) {
    let scale_skew_transform = {
        let c = transform.as_coeffs();
        Affine::new([c[0], c[1], c[2], c[3], 0.0, 0.0])
    };

    let x_advance = scale_skew_transform * Point::new(1.0, 0.0);
    let y_advance = scale_skew_transform * Point::new(0.0, 1.0);

    (
        Vec2::new(x_advance.x, x_advance.y),
        Vec2::new(y_advance.x, y_advance.y),
    )
}

fn convert_fill_rule(fill_rule: FillRule) -> Fill {
    match fill_rule {
        FillRule::NonZero => Fill::NonZero,
        FillRule::EvenOdd => Fill::EvenOdd,
    }
}

fn convert_blend_mode(blend_mode: BlendMode) -> peniko::BlendMode {
    let mix = match blend_mode {
        BlendMode::Normal => Mix::Normal,
        BlendMode::Multiply => Mix::Multiply,
        BlendMode::Screen => Mix::Screen,
        BlendMode::Overlay => Mix::Overlay,
        BlendMode::Darken => Mix::Darken,
        BlendMode::Lighten => Mix::Lighten,
        BlendMode::ColorDodge => Mix::ColorDodge,
        BlendMode::ColorBurn => Mix::ColorBurn,
        BlendMode::HardLight => Mix::HardLight,
        BlendMode::SoftLight => Mix::SoftLight,
        BlendMode::Difference => Mix::Difference,
        BlendMode::Exclusion => Mix::Exclusion,
        BlendMode::Hue => Mix::Hue,
        BlendMode::Saturation => Mix::Saturation,
        BlendMode::Color => Mix::Color,
        BlendMode::Luminosity => Mix::Luminosity,
    };

    peniko::BlendMode::new(mix, Compose::SrcOver)
}

/// Kition PDF patch: the scale for a tiling cell, lowered so that the cell pixmap (`|step| ×
/// scale` pixels a side) stays within `max_pixels`. Geometry is unchanged (the pattern transform
/// is derived from the scale); an over-large step only renders at lower resolution.
pub fn tiling_cell_scale(scale: f32, step: f32, max_pixels: f32) -> f32 {
    let step = step.abs();
    if step.is_finite() && step > 0.0 && scale * step > max_pixels {
        max_pixels / step
    } else {
        scale
    }
}

#[cfg(test)]
mod mesh_paint_tests {
    use super::*;

    #[test]
    fn transparent_mesh_fill_requires_the_explicit_unmasked_page_wrapper() {
        let cache = RenderCache::new();
        let mut renderer = Renderer::new(2, 2, RenderSettings::default(), &cache);
        let zero = PremulRgba8::from_u32(0);
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.push_transparency_group(1.0, None, BlendMode::Normal);
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.is_page_root = true;
        assert!(renderer.can_skip_transparent_mesh_fill(false, zero));
        assert!(!renderer.can_skip_transparent_mesh_fill(true, zero));
        assert!(!renderer.can_skip_transparent_mesh_fill(false, PremulRgba8::from_u32(1)));
        renderer.cur_mask = Some(Mask::from_parts(vec![255; 4], 2, 2));
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.cur_mask = None;
        renderer
            .ctx
            .set_blend_mode(peniko::BlendMode::new(Mix::Multiply, Compose::SrcOver));
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.ctx.set_blend_mode(peniko::BlendMode::default());
        renderer.inside_pattern = true;
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.inside_pattern = false;
        renderer.in_type3_glyph = true;
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.in_type3_glyph = false;
        renderer.push_transparency_group(1.0, None, BlendMode::Normal);
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.pop_transparency_group();
        assert!(renderer.can_skip_transparent_mesh_fill(false, zero));
        renderer.pop_transparency_group();
        assert!(!renderer.can_skip_transparent_mesh_fill(false, zero));
    }
}

#[cfg(test)]
mod dash_hairline_tests {
    use super::*;
    use kurbo::{Cap, PathEl};
    use std::cell::Cell;

    fn line() -> BezPath {
        BezPath::from_vec(vec![
            PathEl::MoveTo((0.0, 0.0).into()),
            PathEl::LineTo((96.0, 0.0).into()),
        ])
    }

    #[test]
    fn input_bound_includes_unyielded_closed_stash() {
        let mut elements = vec![PathEl::MoveTo(Point::ORIGIN)];
        elements.resize(
            MAX_DASH_HAIRLINE_INPUT_ELEMENTS - 1,
            PathEl::LineTo(Point::ORIGIN),
        );
        elements.push(PathEl::ClosePath);
        let path = BezPath::from_vec(elements);
        assert!(device_dash_path(&path, Affine::IDENTITY, 0.0, &[1.0, 1.0], Cap::Butt).is_some());
        let mut excessive = path.clone();
        excessive.line_to(Point::ORIGIN);
        assert!(
            device_dash_path(&excessive, Affine::IDENTITY, 0.0, &[1.0, 1.0], Cap::Butt).is_none()
        );
        let visited = Cell::new(0);
        let mut dashed = kurbo::dash(
            path.elements()
                .iter()
                .copied()
                .inspect(|_| visited.set(visited.get() + 1)),
            0.0,
            &[1.0, 1.0],
        );
        assert!(dashed.next().is_some());
        assert_eq!(visited.get(), MAX_DASH_HAIRLINE_INPUT_ELEMENTS);
    }

    #[test]
    fn output_limit_discards_partial_result_after_at_most_one_extra_yield() {
        let visited = Cell::new(0);
        let elements = std::iter::repeat(PathEl::MoveTo(Point::ORIGIN))
            .inspect(|_| visited.set(visited.get() + 1));
        assert!(collect_device_dash(elements, Affine::IDENTITY, 7, None).is_none());
        assert_eq!(visited.get(), 8);
        assert!(
            collect_device_dash(line().elements().iter().copied(), Affine::IDENTITY, 2, None)
                .is_some()
        );
        assert!(collect_device_dash(std::iter::empty(), Affine::IDENTITY, 0, None).is_some());
    }

    #[test]
    fn rejects_nonfinite_output_and_sanitizes_invalid_or_excessive_patterns() {
        let path = line();
        assert!(
            device_dash_path(
                &path,
                Affine::scale(f64::INFINITY),
                0.0,
                &[1.0, 1.0],
                Cap::Butt
            )
            .is_none()
        );
        assert!(
            collect_device_dash(
                [PathEl::MoveTo((f64::MAX, 0.0).into())].into_iter(),
                Affine::scale(2.0),
                1,
                None
            )
            .is_none()
        );
        for pattern in [&[0.0, 0.0][..], &[-1.0, -1.0], &[f32::NAN], &[0.000001]] {
            assert!(usable_dash_pattern(&path, pattern).is_empty());
        }
        assert_eq!(usable_dash_pattern(&path, &[0.0, 24.0]), [0.0, 24.0]);
        assert_eq!(
            device_dash_path(&path, Affine::IDENTITY, f64::NAN, &[12.0, 12.0], Cap::Butt),
            device_dash_path(&path, Affine::IDENTITY, 0.0, &[12.0, 12.0], Cap::Butt)
        );
    }

    #[test]
    fn zero_cap_delta_follows_transformed_tangent_and_requires_provable_source() {
        let transform = Affine::new([1.0, 1.0, 0.0, 0.01, 0.0, 0.0]);
        let dashed = device_dash_path(&line(), transform, 0.0, &[0.0, 24.0], Cap::Square).unwrap();
        for pair in dashed.elements().chunks_exact(2) {
            let [PathEl::MoveTo(a), PathEl::LineTo(b)] = pair else {
                panic!("expected isolated dot")
            };
            let delta = *b - *a;
            assert!((delta.x - delta.y).abs() < 1e-12);
            assert!((delta.hypot() - 0.0001).abs() < 1e-12);
        }
        assert!(
            device_dash_path(
                &line(),
                Affine::scale_non_uniform(0.0, 1.0),
                0.0,
                &[0.0, 24.0],
                Cap::Square
            )
            .is_none()
        );
        let mut compound = line();
        compound.line_to((96.0, 24.0));
        assert!(
            device_dash_path(&compound, Affine::IDENTITY, 0.0, &[0.0, 24.0], Cap::Round).is_none()
        );
        assert!(
            device_dash_path(
                &line(),
                Affine::IDENTITY,
                18.0,
                &[0.0, 12.0, 6.0, 12.0],
                Cap::Round
            )
            .is_none()
        );
        let ninety = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::ORIGIN),
            PathEl::LineTo((90.0, 0.0).into()),
        ]);
        let raw: Vec<_> =
            kurbo::dash(ninety.elements().iter().copied(), 0.0, &[0.0, 24.0]).collect();
        assert!(
            raw.windows(2)
                .any(|pair| matches!(pair, [PathEl::MoveTo(a), PathEl::LineTo(b)] if a != b))
        );
        let dots =
            device_dash_path(&ninety, Affine::IDENTITY, 0.0, &[0.0, 24.0], Cap::Square).unwrap();
        for pair in dots.elements().chunks_exact(2) {
            let [PathEl::MoveTo(a), PathEl::LineTo(b)] = pair else {
                panic!("expected isolated dot")
            };
            assert!(((*b - *a).hypot() - 0.0001).abs() < 1e-12);
        }
        // Zero gaps do not require a cap tangent and keep the general path support.
        assert!(
            device_dash_path(&compound, Affine::IDENTITY, 0.0, &[24.0, 0.0], Cap::Round).is_some()
        );
        let collapsed = collect_device_dash(
            line().elements().iter().copied(),
            Affine::scale_non_uniform(0.0, 1.0),
            2,
            None,
        )
        .unwrap();
        assert_eq!(
            collapsed.elements(),
            &[PathEl::MoveTo(Point::ORIGIN), PathEl::LineTo(Point::ORIGIN)]
        );
    }

    #[test]
    fn positive_phase_boundary_retains_zero_segment_without_inventing_tangent() {
        // Characterization: kurbo emits an exact degenerate initial run at this boundary.
        // The backend's existing omission of its caps remains outside the zero-entry fix.
        let dashed =
            device_dash_path(&line(), Affine::IDENTITY, 12.0, &[12.0, 12.0], Cap::Square).unwrap();
        assert!(
            dashed
                .elements()
                .windows(2)
                .any(|pair| matches!(pair, [PathEl::MoveTo(a), PathEl::LineTo(b)] if a == b))
        );
    }

    #[test]
    fn closed_all_on_path_keeps_its_seam_and_large_phase_reduces_exactly() {
        let path = Rect::new(0.0, 0.0, 24.0, 24.0).to_path(0.1);
        let dashed =
            device_dash_path(&path, Affine::IDENTITY, 0.0, &[1000.0, 12.0], Cap::Round).unwrap();
        assert!(dashed.elements().contains(&PathEl::ClosePath));
        assert_eq!(
            device_dash_path(
                &line(),
                Affine::IDENTITY,
                240006.0,
                &[12.0, 12.0],
                Cap::Butt
            ),
            device_dash_path(&line(), Affine::IDENTITY, 6.0, &[12.0, 12.0], Cap::Butt)
        );
    }
}

#[cfg(test)]
mod mixed_dash_tests {
    use super::*;
    use kurbo::{Cap, PathEl};

    fn events(length: f64, phase: f64, pattern: &[f64]) -> Option<Vec<(f64, f64, bool)>> {
        let mut result = Vec::new();
        mixed_dash_events(length, phase, pattern, 32, 128, |from, to, zero| {
            result.push((from, to, zero));
            Some(())
        })?;
        Some(result)
    }

    fn line(length: f64) -> BezPath {
        BezPath::from_vec(vec![
            PathEl::MoveTo(Point::ORIGIN),
            PathEl::LineTo((length, 0.0).into()),
        ])
    }

    #[test]
    fn mixed_even_phases_match_independent_spans() {
        let pattern = [0.0, 12.0, 6.0, 12.0];
        assert_eq!(
            events(84.0, 0.0, &pattern).unwrap(),
            vec![
                (0.0, 0.0, true),
                (12.0, 18.0, false),
                (30.0, 30.0, true),
                (42.0, 48.0, false),
                (60.0, 60.0, true),
                (72.0, 78.0, false),
            ]
        );
        let shifted = vec![
            (9.0, 15.0, false),
            (27.0, 27.0, true),
            (39.0, 45.0, false),
            (57.0, 57.0, true),
            (69.0, 75.0, false),
        ];
        for phase in [3.0, 300003.0, -27.0] {
            assert_eq!(events(84.0, phase, &pattern).unwrap(), shifted);
        }
        assert_eq!(
            events(84.0, 15.0, &pattern).unwrap(),
            vec![
                (0.0, 3.0, false),
                (15.0, 15.0, true),
                (27.0, 33.0, false),
                (45.0, 45.0, true),
                (57.0, 63.0, false),
                (75.0, 75.0, true),
            ]
        );
    }

    #[test]
    fn odd_pattern_uses_doubled_parity_without_merging_runs() {
        assert_eq!(
            events(84.0, 3.0, &[0.0, 8.0, 4.0]).unwrap(),
            vec![
                (5.0, 9.0, false),
                (9.0, 17.0, false),
                (21.0, 21.0, true),
                (29.0, 33.0, false),
                (33.0, 41.0, false),
                (45.0, 45.0, true),
                (53.0, 57.0, false),
                (57.0, 65.0, false),
                (69.0, 69.0, true),
                (77.0, 81.0, false),
                (81.0, 84.0, false),
            ]
        );
    }

    #[test]
    fn explicit_start_zero_and_gap_boundary_survive_but_positive_end_falls_back() {
        assert_eq!(
            events(5.0, 0.0, &[0.0, 12.0, 6.0, 12.0]).unwrap(),
            vec![(0.0, 0.0, true)]
        );
        assert_eq!(
            events(5.0, 12.0, &[0.0, 12.0, 6.0, 12.0]).unwrap(),
            vec![(0.0, 5.0, false)]
        );
        assert_eq!(
            events(5.0, 12.0, &[0.0, 12.0, 0.0, 0.0, 6.0, 12.0]).unwrap(),
            vec![(0.0, 0.0, true), (0.0, 5.0, false),]
        );
        assert!(events(84.0, 18.0, &[0.0, 12.0, 6.0, 12.0]).is_none());
    }

    #[test]
    fn terminal_explicit_zero_falls_back_even_through_zero_gap() {
        assert!(events(30.0, 0.0, &[0.0, 12.0, 6.0, 12.0]).is_none());
        assert!(events(6.0, 0.0, &[6.0, 0.0, 0.0, 12.0]).is_none());
        assert!(events(6.0, 0.0, &[6.0, 0.0, 4.0, 12.0]).is_some());
        assert!(
            mixed_line_dash_path(
                &line(30.0),
                Affine::IDENTITY,
                0.0,
                &[0.0, 12.0, 6.0, 12.0],
                100
            )
            .is_none()
        );
    }

    #[test]
    fn tiny_positive_intervals_are_not_dots_and_nonprogress_fails() {
        let tiny = 2.0_f64.powi(-40);
        let trace = events(13.0, 0.0, &[0.0, 12.0, tiny, 12.0]).unwrap();
        assert_eq!(trace, vec![(0.0, 0.0, true), (12.0, 12.0 + tiny, false)]);
        let huge = 2.0_f64.powi(53);
        assert!(events(huge + 4.0, 0.0, &[0.0, huge, 1.0, 12.0]).is_none());
    }

    #[test]
    fn phase_event_callback_and_output_limits_discard_incomplete_preparation() {
        let pattern = [0.0, 12.0, 6.0, 12.0];
        assert!(mixed_dash_events(84.0, 3.0, &pattern, 1, 128, |_, _, _| Some(())).is_none());
        assert!(mixed_dash_events(84.0, 0.0, &pattern, 4, 1, |_, _, _| Some(())).is_none());
        let mut emitted = 0;
        assert!(
            mixed_dash_events(84.0, 0.0, &pattern, 4, 128, |_, _, _| {
                emitted += 1;
                (emitted < 2).then_some(())
            })
            .is_none()
        );
        assert_eq!(emitted, 2);
        for limit in [0, 1, 2, 11] {
            assert!(
                mixed_line_dash_path(&line(84.0), Affine::IDENTITY, 0.0, &pattern, limit).is_none()
            );
        }
        assert_eq!(
            mixed_line_dash_path(&line(84.0), Affine::IDENTITY, 0.0, &pattern, 12)
                .unwrap()
                .elements()
                .len(),
            12
        );
    }

    #[test]
    fn invalid_patterns_and_nonfinite_geometry_fall_back() {
        for pattern in [
            &[][..],
            &[0.0, 0.0],
            &[-1.0, 12.0],
            &[f64::NAN, 12.0],
            &[f64::MAX, f64::MAX],
        ] {
            assert!(events(84.0, 0.0, pattern).is_none());
        }
        for length in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(events(length, 0.0, &[0.0, 12.0, 6.0, 12.0]).is_none());
        }
        let pattern = [0.0, 12.0, 6.0, 12.0];
        assert_eq!(
            events(84.0, f64::NAN, &pattern),
            events(84.0, 0.0, &pattern)
        );
        assert!(
            mixed_line_dash_path(&line(84.0), Affine::scale(f64::MAX), 0.0, &pattern, 128)
                .is_none()
        );
        let excessive = vec![1.0; MAX_DASHES_PER_STROKE as usize + 1];
        assert!(events(1.0, 0.0, &excessive).is_none());
    }

    #[test]
    fn mixed_caps_require_a_single_noncollapsed_straight_tangent() {
        let pattern = [0.0, 12.0, 6.0, 12.0];
        let mut compound = line(84.0);
        compound.line_to((84.0, 12.0));
        let mut curve = BezPath::new();
        curve.move_to(Point::ORIGIN);
        curve.curve_to((12.0, 0.0), (30.0, 10.0), (84.0, 0.0));
        for path in [compound, curve, line(0.0)] {
            assert!(device_dash_path(&path, Affine::IDENTITY, 0.0, &pattern, Cap::Round).is_none());
        }
        assert!(
            device_dash_path(
                &line(84.0),
                Affine::scale_non_uniform(0.0, 1.0),
                0.0,
                &pattern,
                Cap::Square
            )
            .is_none()
        );
        let output = device_dash_path(
            &line(84.0),
            Affine::new([1.0, 1.0, 0.0, 0.01, 0.0, 0.0]),
            0.0,
            &pattern,
            Cap::Round,
        )
        .unwrap();
        let [PathEl::MoveTo(a), PathEl::LineTo(b), ..] = output.elements() else {
            panic!("missing first dot")
        };
        let delta = *b - *a;
        assert!((delta.x - delta.y).abs() < 1e-12);
        assert!((delta.hypot() - 0.0001).abs() < 1e-12);
    }
}

#[cfg(test)]
mod separated_dash_tests {
    use super::*;
    use kurbo::{Cap, PathEl};

    fn lines(lengths: &[f64]) -> BezPath {
        let mut path = BezPath::new();
        for (index, &length) in lengths.iter().enumerate() {
            let y = index as f64 * 20.0;
            path.move_to((0.0, y));
            path.line_to((length, y));
        }
        path
    }

    #[test]
    fn copied_phase_state_restarts_each_unequal_subpath() {
        let pattern = prepare_mixed_dash_pattern(3.0, &[0.0, 12.0, 6.0, 12.0], 4).unwrap();
        let mut budget = MixedDashBudget {
            events: 100,
            terminal: 100,
        };
        for (length, expected) in [
            (
                54.0,
                vec![(9.0, 15.0, false), (27.0, 27.0, true), (39.0, 45.0, false)],
            ),
            (38.0, vec![(9.0, 15.0, false), (27.0, 27.0, true)]),
        ] {
            let mut actual = Vec::new();
            prepared_mixed_dash_events(length, pattern, &mut budget, |from, to, zero| {
                actual.push((from, to, zero));
                Some(())
            })
            .unwrap();
            assert_eq!(actual, expected);
        }
        assert_eq!(budget.events, 88); // Seven visits, then five; gaps count too.
        assert_eq!(budget.terminal, 100);
    }

    #[test]
    fn aggregate_event_and_output_limits_do_not_restart_per_line() {
        let path = lines(&[54.0, 44.0]);
        let pattern = [0.0, 12.0, 6.0, 12.0];
        // Each line needs at most eight semantic visits, but together they need fifteen.
        for limit in [0, 8, 14] {
            assert!(
                separated_mixed_line_dash_path(
                    &path,
                    Affine::IDENTITY,
                    0.0,
                    &pattern,
                    16,
                    limit,
                    100
                )
                .is_none()
            );
        }
        assert!(
            separated_mixed_line_dash_path(&path, Affine::IDENTITY, 0.0, &pattern, 15, 15, 100)
                .is_none()
        );
        let output =
            separated_mixed_line_dash_path(&path, Affine::IDENTITY, 0.0, &pattern, 16, 15, 100)
                .unwrap();
        assert_eq!(output.elements().len(), 16);
    }

    #[test]
    fn aggregate_terminal_lookups_include_zero_gaps() {
        let path = lines(&[18.0, 18.0]);
        let pattern = [0.0, 12.0, 6.0, 0.0, 4.0, 12.0];
        // Each endpoint looks through a zero off-entry to the next positive on-entry.
        for limit in [0, 2, 3] {
            assert!(
                separated_mixed_line_dash_path(&path, Affine::IDENTITY, 0.0, &pattern, 8, 6, limit)
                    .is_none()
            );
        }
        assert!(
            separated_mixed_line_dash_path(&path, Affine::IDENTITY, 0.0, &pattern, 8, 6, 4)
                .is_some()
        );
    }

    #[test]
    fn late_terminal_zero_and_exact_positive_phase_end_discard_whole_path() {
        let pattern = [0.0, 12.0, 6.0, 12.0];
        assert!(
            separated_mixed_line_dash_path(
                &lines(&[54.0, 30.0]),
                Affine::IDENTITY,
                0.0,
                &pattern,
                100,
                100,
                100
            )
            .is_none()
        );
        assert!(
            separated_mixed_line_dash_path(
                &lines(&[54.0, 44.0]),
                Affine::IDENTITY,
                18.0,
                &pattern,
                100,
                100,
                100
            )
            .is_none()
        );
        let zero_gap = [6.0, 0.0, 0.0, 12.0];
        assert!(
            separated_mixed_line_dash_path(
                &lines(&[4.0, 6.0]),
                Affine::IDENTITY,
                0.0,
                &zero_gap,
                100,
                100,
                100
            )
            .is_none()
        );
    }

    #[test]
    fn preflight_rejects_later_connected_curved_closed_or_collapsed_sources() {
        let mut connected = lines(&[54.0, 44.0]);
        connected.line_to((44.0, 40.0));
        let mut curve = lines(&[54.0]);
        curve.move_to((0.0, 20.0));
        curve.curve_to((10.0, 20.0), (20.0, 30.0), (30.0, 20.0));
        let mut closed = lines(&[54.0, 44.0]);
        closed.close_path();
        let mut stray_move = lines(&[54.0]);
        stray_move.move_to((0.0, 20.0));
        stray_move.move_to((44.0, 20.0));
        let pattern = [0.0, 12.0, 6.0, 12.0];
        for path in [
            connected,
            curve,
            closed,
            stray_move,
            lines(&[54.0, 0.0]),
            lines(&[54.0, f64::NAN]),
        ] {
            assert!(device_dash_path(&path, Affine::IDENTITY, 0.0, &pattern, Cap::Round).is_none());
        }
        let path = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::ORIGIN),
            PathEl::LineTo((54.0, 0.0).into()),
            PathEl::MoveTo((80.0, 0.0).into()),
            PathEl::LineTo((80.0, 44.0).into()),
        ]);
        assert!(
            device_dash_path(
                &path,
                Affine::scale_non_uniform(1.0, 0.0),
                0.0,
                &pattern,
                Cap::Square
            )
            .is_none()
        );
        // Separate pure-dot sources now use the same bounded semantic walk.
        assert!(
            device_dash_path(&path, Affine::IDENTITY, 0.0, &[0.0, 24.0], Cap::Square).is_some()
        );
    }

    #[test]
    fn each_subpath_recovers_its_own_transformed_square_cap_tangent() {
        let path = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::ORIGIN),
            PathEl::LineTo((54.0, 0.0).into()),
            PathEl::MoveTo((80.0, 0.0).into()),
            PathEl::LineTo((80.0, 44.0).into()),
        ]);
        let transform = Affine::new([1.0, 0.0, 0.25, 0.5, 0.0, 0.0]);
        let output =
            device_dash_path(&path, transform, 0.0, &[0.0, 12.0, 6.0, 12.0], Cap::Square).unwrap();
        for (offset, expected_start, direction) in [
            (0, Point::ORIGIN, Vec2::new(1.0, 0.0)),
            (8, Point::new(80.0, 0.0), Vec2::new(1.0, 2.0)),
        ] {
            let [PathEl::MoveTo(start), PathEl::LineTo(end)] =
                &output.elements()[offset..offset + 2]
            else {
                panic!("expected a dot")
            };
            assert_eq!(*start, expected_start);
            let delta = *end - *start;
            assert!((delta.hypot() - 0.0001).abs() < 1e-12);
            assert!(delta.cross(direction).abs() < 1e-12);
        }
    }

    #[test]
    fn preflight_rejects_late_overflow_even_when_that_line_emits_nothing() {
        let path = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::ORIGIN),
            PathEl::LineTo((54.0, 0.0).into()),
            PathEl::MoveTo((1e308, 0.0).into()),
            PathEl::LineTo((1e308, 1.0).into()),
        ]);
        // The second source and its transformed tangent are finite. Its transformed
        // position overflows, but phase 3 puts its entire unit length in a gap.
        assert!(
            separated_mixed_line_dash_path(
                &path,
                Affine::scale_non_uniform(2.0, 1.0),
                3.0,
                &[0.0, 12.0, 6.0, 12.0],
                100,
                100,
                100,
            )
            .is_none()
        );
    }

    #[test]
    fn late_nonprogress_discards_previous_subpath_output() {
        let huge = 2.0_f64.powi(53);
        let pattern = [0.0, huge, 1.0, 12.0];
        assert!(
            separated_mixed_line_dash_path(
                &lines(&[54.0, huge + 4.0]),
                Affine::IDENTITY,
                0.0,
                &pattern,
                100,
                100,
                100
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod pure_zero_separated_tests {
    use super::*;
    use kurbo::{Cap, PathEl};

    fn lines(lengths: &[f64]) -> BezPath {
        let mut path = BezPath::new();
        for (index, length) in lengths.iter().enumerate() {
            path.move_to((0.0, index as f64 * 20.0));
            path.line_to((*length, index as f64 * 20.0));
        }
        path
    }

    #[test]
    fn independent_dot_tables_cover_gap_phases_periods_and_coincident_events() {
        for (dashes, phase, first, second) in [
            (vec![0.0, 12.0], 3.0, vec![9.0, 21.0], vec![9.0, 21.0, 33.0]),
            (
                vec![0.0, 12.0],
                27.0,
                vec![9.0, 21.0],
                vec![9.0, 21.0, 33.0],
            ),
            (
                vec![0.0, 8.0, 0.0, 12.0],
                3.0,
                vec![5.0, 17.0, 25.0],
                vec![5.0, 17.0, 25.0, 37.0],
            ),
            (
                vec![0.0, 0.0, 0.0, 12.0],
                3.0,
                vec![9.0, 9.0, 21.0, 21.0],
                vec![9.0, 9.0, 21.0, 21.0, 33.0, 33.0],
            ),
            (
                vec![0.0, 12.0],
                0.0,
                vec![0.0, 12.0, 24.0],
                vec![0.0, 12.0, 24.0, 36.0],
            ),
            (
                vec![0.0, 8.0, 0.0, 12.0],
                8.0,
                vec![0.0, 12.0, 20.0],
                vec![0.0, 12.0, 20.0, 32.0, 40.0],
            ),
        ] {
            let pattern = prepare_mixed_dash_pattern(phase, &dashes, 8).unwrap();
            let mut budget = MixedDashBudget {
                events: 100,
                terminal: 100,
            };
            for (length, expected) in [(31.0, first), (43.0, second)] {
                let mut actual = Vec::new();
                prepared_mixed_dash_events(length, pattern, &mut budget, |from, to, zero| {
                    assert!(zero);
                    assert_eq!(from, to);
                    actual.push(from);
                    Some(())
                })
                .unwrap();
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn pure_dot_global_event_and_output_bounds_are_shared_by_all_subpaths() {
        let path = lines(&[31.0, 43.0]);
        for (elements, events) in [(9, 12), (10, 11)] {
            assert!(
                separated_mixed_line_dash_path(
                    &path,
                    Affine::IDENTITY,
                    3.0,
                    &[0.0, 12.0],
                    elements,
                    events,
                    0
                )
                .is_none()
            );
        }
        let output =
            separated_mixed_line_dash_path(&path, Affine::IDENTITY, 3.0, &[0.0, 12.0], 10, 12, 0)
                .unwrap();
        assert_eq!(output.elements().len(), 10);
    }

    #[test]
    fn late_terminal_dot_preserves_fallback_and_empty_subpath_is_allowed() {
        for cap in [Cap::Round, Cap::Square] {
            for dashes in [&[0.0, 12.0][..], &[0.0, 0.0, 0.0, 12.0][..]] {
                assert!(
                    device_dash_path(&lines(&[31.0, 45.0]), Affine::IDENTITY, 3.0, dashes, cap)
                        .is_none()
                );
                assert!(
                    device_dash_path(&lines(&[31.0, 48.0]), Affine::IDENTITY, 0.0, dashes, cap)
                        .is_none()
                );
            }
            let output = device_dash_path(
                &lines(&[31.0, 4.0]),
                Affine::IDENTITY,
                3.0,
                &[0.0, 12.0],
                cap,
            )
            .unwrap();
            assert_eq!(output.elements().len(), 4);
        }
    }

    #[test]
    fn coincident_pure_dot_events_retain_whole_path_fallback() {
        for cap in [Cap::Round, Cap::Square] {
            for phase in [0.0, 3.0] {
                assert!(
                    device_dash_path(
                        &lines(&[31.0, 43.0]),
                        Affine::IDENTITY,
                        phase,
                        &[0.0, 0.0, 0.0, 12.0],
                        cap
                    )
                    .is_none()
                );
                // The established single-source preparation remains available.
                assert!(
                    device_dash_path(
                        &lines(&[31.0]),
                        Affine::IDENTITY,
                        phase,
                        &[0.0, 0.0, 0.0, 12.0],
                        cap
                    )
                    .is_some()
                );
            }
        }
    }

    #[test]
    fn pure_dot_tangents_follow_reversed_and_sheared_sources() {
        let mut path = BezPath::new();
        path.move_to((43.0, 0.0));
        path.line_to((0.0, 0.0));
        path.move_to((80.0, 0.0));
        path.line_to((80.0, 31.0));
        let transform = Affine::new([1.0, 0.0, 0.25, 0.5, 0.0, 0.0]);
        let output = device_dash_path(&path, transform, 3.0, &[0.0, 12.0], Cap::Square).unwrap();
        for (offset, first, direction) in [
            (0, Point::new(34.0, 0.0), Vec2::new(-1.0, 0.0)),
            (6, Point::new(82.25, 4.5), Vec2::new(1.0, 2.0)),
        ] {
            let [PathEl::MoveTo(start), PathEl::LineTo(end)] =
                &output.elements()[offset..offset + 2]
            else {
                panic!("expected a dot");
            };
            assert_eq!(*start, first);
            let delta = *end - *start;
            assert!((delta.hypot() - 0.0001).abs() < 1e-12);
            assert!(delta.cross(direction).abs() < 1e-12);
            assert!(delta.dot(direction) > 0.0);
        }
    }

    #[test]
    fn pure_dot_preflight_rejects_unsupported_and_late_invalid_sources() {
        let mut connected = lines(&[31.0, 43.0]);
        connected.line_to((50.0, 20.0));
        let mut closed = lines(&[31.0, 43.0]);
        closed.close_path();
        let mut curve = lines(&[31.0]);
        curve.move_to((0.0, 20.0));
        curve.curve_to((10.0, 20.0), (20.0, 30.0), (30.0, 20.0));
        for path in [
            connected,
            closed,
            curve,
            lines(&[31.0, 0.0]),
            lines(&[31.0, f64::INFINITY]),
        ] {
            assert!(
                device_dash_path(&path, Affine::IDENTITY, 3.0, &[0.0, 12.0], Cap::Round).is_none()
            );
        }
        let path = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::ORIGIN),
            PathEl::LineTo((31.0, 0.0).into()),
            PathEl::MoveTo((80.0, 0.0).into()),
            PathEl::LineTo((80.0, 4.0).into()),
        ]);
        assert!(
            device_dash_path(
                &path,
                Affine::scale_non_uniform(1.0, 0.0),
                3.0,
                &[0.0, 12.0],
                Cap::Square
            )
            .is_none()
        );
    }
}
