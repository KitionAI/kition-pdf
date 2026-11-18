//! Encoding shading patterns for easy sampling.

use crate::color::{AlphaColor, ColorComponents, ColorSpace};
use crate::interpret::state::ActiveTransferFunction;
use crate::pattern::ShadingPattern;
use crate::shading::{
    PATCH_TRIANGLE_COUNT, PATCH_TRIANGLE_INFLATION, ShadingFunction, ShadingType, Triangle,
};
use kurbo::{Affine, Point, Rect};
use rustc_hash::FxHashMap;
use smallvec::{ToSmallVec, smallvec};

/// A shading pattern that was encoded so it can be sampled.
#[derive(Debug)]
pub struct EncodedShadingPattern {
    /// The base transform of the shading pattern.
    pub base_transform: Affine,
    pub(crate) color_space: ColorSpace,
    pub(crate) background_color: AlphaColor,
    pub(crate) shading_type: EncodedShadingType,
    pub(crate) opacity: f32,
    pub(crate) transfer_function: Option<ActiveTransferFunction>,
}

impl EncodedShadingPattern {
    /// Kition PDF patch: an empty sampled mesh has one color everywhere. Use the
    /// ordinary sampler so background, opacity, and transfer behavior stay identical.
    pub fn constant_sampled_color(&self) -> Option<[f32; 4]> {
        match &self.shading_type {
            EncodedShadingType::Sampled { samples, .. } if samples.is_empty() => {
                Some(self.sample(Point::ZERO))
            }
            _ => None,
        }
    }

    /// Sample the shading at the given position.
    #[inline]
    pub fn sample(&self, pos: Point) -> [f32; 4] {
        self.shading_type
            .eval(pos, self.background_color, &self.color_space)
            .map(|v| {
                let mut components = v.components();
                components[3] *= self.opacity;

                if let Some(tf) = &self.transfer_function {
                    return tf.apply(&AlphaColor::new(components)).components();
                }

                components
            })
            .unwrap_or([0.0, 0.0, 0.0, 0.0])
    }
}

impl ShadingPattern {
    /// Encode the shading pattern.
    pub fn encode(&self) -> EncodedShadingPattern {
        self.encode_within(None)
    }

    /// Kition PDF patch: encode the shading pattern for drawing inside `area` (device space) only.
    /// Mesh shadings are sampled pixel by pixel, and only there (see `TriangleSampler`).
    pub fn encode_within(&self, area: Option<Rect>) -> EncodedShadingPattern {
        let base_transform;

        let shading_type = match self.shading.shading_type.as_ref() {
            ShadingType::FunctionBased {
                domain,
                matrix,
                function,
            } => {
                base_transform = (self.matrix * *matrix).inverse();
                encode_function_shading(domain, function)
            }
            ShadingType::RadialAxial {
                coords,
                domain,
                function,
                extend,
                axial,
            } => {
                let (encoded, initial_transform) =
                    encode_axial_shading(*coords, *domain, function, *extend, *axial);

                base_transform = initial_transform * self.matrix.inverse();

                encoded
            }
            ShadingType::TriangleMesh {
                triangles,
                function,
            } => {
                let full_transform = self.matrix;
                let mut sampler = TriangleSampler::new(area);
                sampler.sample(triangles, full_transform);
                let samples = sampler.map;

                base_transform = Affine::IDENTITY;

                EncodedShadingType::Sampled {
                    samples,
                    function: function.clone(),
                }
            }
            ShadingType::CoonsPatchMesh { patches, function } => {
                // Kition PDF patch: one patch's triangles at a time (each is 722); collecting all
                // of them first held 150 KB per patch before any limit applied.
                let full_transform = self.matrix;
                let mut sampler = TriangleSampler::new(area);
                let mut triangles = vec![];
                for patch in patches {
                    if !mesh_budget_left() {
                        break;
                    }
                    if patch_outside_area(&patch.control_points, full_transform, area, true) {
                        if !take_mesh_budget(PATCH_TRIANGLE_COUNT) {
                            break;
                        }
                        continue;
                    }
                    triangles.clear();
                    patch.to_triangles(&mut triangles);
                    if !sampler.sample(&triangles, full_transform) {
                        break;
                    }
                }
                let samples = sampler.map;

                base_transform = Affine::IDENTITY;

                EncodedShadingType::Sampled {
                    samples,
                    function: function.clone(),
                }
            }
            ShadingType::TensorProductPatchMesh { patches, function } => {
                // Kition PDF patch: one patch's triangles at a time, as for Coons patches.
                let full_transform = self.matrix;
                let mut sampler = TriangleSampler::new(area);
                let mut triangles = vec![];
                for patch in patches {
                    if !mesh_budget_left() {
                        break;
                    }
                    if patch_outside_area(&patch.control_points, full_transform, area, false) {
                        if !take_mesh_budget(PATCH_TRIANGLE_COUNT) {
                            break;
                        }
                        continue;
                    }
                    triangles.clear();
                    patch.to_triangles(&mut triangles);
                    if !sampler.sample(&triangles, full_transform) {
                        break;
                    }
                }
                let samples = sampler.map;

                base_transform = Affine::IDENTITY;

                EncodedShadingType::Sampled {
                    samples,
                    function: function.clone(),
                }
            }
            ShadingType::Dummy => {
                base_transform = Affine::IDENTITY;

                EncodedShadingType::Dummy
            }
        };

        let color_space = self.shading.color_space.clone();

        let background_color = self
            .shading
            .background
            .as_ref()
            .map(|b| color_space.to_rgba(b, 1.0, false))
            .unwrap_or(AlphaColor::TRANSPARENT);

        EncodedShadingPattern {
            color_space,
            background_color,
            shading_type,
            base_transform,
            opacity: self.opacity,
            transfer_function: self.transfer_function.clone(),
        }
    }
}

fn encode_axial_shading(
    coords: [f32; 6],
    domain: [f32; 2],
    function: &ShadingFunction,
    extend: [bool; 2],
    is_axial: bool,
) -> (EncodedShadingType, Affine) {
    let initial_transform;

    let params = if is_axial {
        let [x_0, y_0, x_1, y_1, _, _] = coords;

        initial_transform = ts_from_line_to_line(
            Point::new(x_0 as f64, y_0 as f64),
            Point::new(x_1 as f64, y_1 as f64),
            Point::ZERO,
            Point::new(1.0, 0.0),
        );

        RadialAxialParams::Axial
    } else {
        let [x_0, y_0, r0, x_1, y_1, r_1] = coords;

        initial_transform = Affine::translate((-x_0 as f64, -y_0 as f64));
        let new_x1 = x_1 - x_0;
        let new_y1 = y_1 - y_0;

        let p1 = Point::new(new_x1 as f64, new_y1 as f64);
        let r = Point::new(r0 as f64, r_1 as f64);

        RadialAxialParams::Radial { p1, r }
    };

    (
        EncodedShadingType::RadialAxial {
            function: function.clone(),
            params,
            domain,
            extend,
        },
        initial_transform,
    )
}

/// Kition PDF patch: pixel visits one mesh shading may make, per pixel of the area it is drawn in
/// and per triangle. Sampling walks each triangle's bounding box (rounded out to whole pixels),
/// so triangles far off the page (a fuzzed patch decoded onto ±40000) asked for 65536 × 65536
/// samples, and many triangles covering the same pixels repeat the work. A triangle's box is
/// about twice its area, and a sub-pixel triangle's box is at most 2 × 2 pixels.
const MESH_VISITS_PER_PIXEL: u64 = 8;
const MESH_VISITS_PER_TRIANGLE: u64 = 4;

/// Kition PDF patch: the visits allowed when no area is given (the public `encode`).
const MESH_VISITS_WITHOUT_AREA: u64 = 1 << 24;

/// Kition PDF patch: mesh triangles one page may sample in all, across every shading painted on
/// it. A shading is triangulated again each time it is painted (2^16 patches are 47 million
/// triangles, several seconds), so painting one a thousand times took hours. A 200 × 200
/// gradient mesh is 29 million triangles.
const MAX_PAGE_MESH_TRIANGLES: u64 = 1 << 25;

/// Kition PDF patch: reject a wholly off-area patch before generating its 722 triangles.
/// Tensor surfaces stay in their control hull. Coons surfaces do not: they are two
/// boundary blends minus the bilinear corner blend, so bound all three terms separately.
/// Unknown/nonfinite bounds and the public no-area encoder keep the existing path.
fn patch_outside_area<const N: usize>(
    points: &[Point; N],
    transform: Affine,
    area: Option<Rect>,
    coons: bool,
) -> bool {
    let Some(area) = area else { return false };
    let [a, b, c, d, e, f] = transform.as_coeffs();
    if ![a, b, c, d, e, f, area.x0, area.y0, area.x1, area.y1]
        .iter()
        .all(|v| v.is_finite())
    {
        return false;
    }
    // Include pre-cancellation operand magnitudes in the outward rounding allowance.
    let linear_scale = a.abs().max(b.abs()).max(c.abs()).max(d.abs());
    let translation_scale = e.abs().max(f.abs());
    let mut magnitude = 1.0_f64;
    for point in points {
        if !point.is_finite() {
            return false;
        }
        magnitude =
            magnitude.max((point.x.abs() + point.y.abs()) * linear_scale + translation_scale);
    }
    let points = points.map(|point| transform * point);
    if !magnitude.is_finite() || !points.iter().all(|point| point.is_finite()) {
        return false;
    }
    let bounds = |indices: &[usize]| -> Option<Rect> {
        let mut rect = Rect::new(
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for index in indices {
            rect = rect.union_pt(*points.get(*index)?);
        }
        Some(rect)
    };
    let rect = if coons {
        let (Some(sc), Some(sd), Some(sb)) = (
            bounds(&[0, 11, 10, 9, 3, 4, 5, 6]),
            bounds(&[0, 1, 2, 3, 9, 8, 7, 6]),
            bounds(&[0, 3, 6, 9]),
        ) else {
            return false;
        };
        Rect::new(
            sc.x0 + sd.x0 - sb.x1,
            sc.y0 + sd.y0 - sb.y1,
            sc.x1 + sd.x1 - sb.x0,
            sc.y1 + sd.y1 - sb.y0,
        )
    } else {
        points.iter().fold(
            Rect::new(
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |rect, point| rect.union_pt(*point),
        )
    };
    // One pixel also retains edge texels; the relative term covers arithmetic at large
    // coordinates, including cancellation in the affine map and Coons subtraction.
    let padding = 1.0 + 256.0 * f64::EPSILON * magnitude;
    // Triangulation expands each vertex away from its triangle centroid. Both lie
    // inside these bounds, so one full axis span times the excess factor is safe.
    let excess = PATCH_TRIANGLE_INFLATION - 1.0;
    let rect = rect.inflate(
        padding + excess * rect.width(),
        padding + excess * rect.height(),
    );
    if ![rect.x0, rect.y0, rect.x1, rect.y1]
        .iter()
        .all(|v| v.is_finite())
    {
        return false;
    }
    rect.x1 < area.x0 || rect.y1 < area.y0 || rect.x0 > area.x1 || rect.y0 > area.y1
}

std::thread_local! {
    static MESH_TRIANGLES_LEFT: core::cell::Cell<u64> =
        const { core::cell::Cell::new(MAX_PAGE_MESH_TRIANGLES) };
}

/// Kition PDF patch: a page starts with a fresh mesh triangle budget (called from
/// `Context::new`).
pub(crate) fn reset_mesh_budget() {
    MESH_TRIANGLES_LEFT.with(|n| n.set(MAX_PAGE_MESH_TRIANGLES));
}

/// Kition PDF patch: `false` once this page's mesh triangle budget is spent.
pub(crate) fn mesh_budget_left() -> bool {
    MESH_TRIANGLES_LEFT.with(|n| n.get() > 0)
}

// Culled patches consume the same budget as their triangles: repeated invisible shadings
// must not turn a bounded page into an unbounded sequence of control-point scans.
fn take_mesh_budget(triangles: usize) -> bool {
    let page_left = MESH_TRIANGLES_LEFT.with(core::cell::Cell::get);
    if page_left < triangles as u64 {
        MESH_TRIANGLES_LEFT.with(|n| n.set(0));
        warn!("page mesh triangle budget exceeded; the rest of the shading is not drawn");
        return false;
    }
    MESH_TRIANGLES_LEFT.with(|n| n.set(page_left - triangles as u64));
    true
}

/// Kition PDF patch: samples mesh triangles into a map with one entry per pixel, only inside
/// `area` (device space), within the visit budget above.
struct TriangleSampler {
    map: FxHashMap<(u16, u16), ColorComponents>,
    area: Rect,
    visits_left: u64,
}

impl TriangleSampler {
    fn new(area: Option<Rect>) -> Self {
        let visits_left = match area {
            Some(a) => (a.width().max(0.0).ceil() as u64)
                .saturating_mul(a.height().max(0.0).ceil() as u64)
                .saturating_mul(MESH_VISITS_PER_PIXEL),
            None => MESH_VISITS_WITHOUT_AREA,
        };
        Self {
            map: FxHashMap::default(),
            area: area.unwrap_or(Rect::new(0.0, 0.0, 65536.0, 65536.0)),
            visits_left,
        }
    }

    /// Sample `triangles`; `false` once a budget is spent (the rest is left unsampled).
    fn sample(&mut self, triangles: &[Triangle], transform: Affine) -> bool {
        if !take_mesh_budget(triangles.len()) {
            return false;
        }
        for t in triangles {
            let t = {
                let p0 = transform * t.p0.point;
                let p1 = transform * t.p1.point;
                let p2 = transform * t.p2.point;

                let mut v0 = t.p0.clone();
                v0.point = p0;
                let mut v1 = t.p1.clone();
                v1.point = p1;
                let mut v2 = t.p2.clone();
                v2.point = p2;

                Triangle::new(v0, v1, v2)
            };

            // Only the part of the triangle inside `area`; only such triangles add visits.
            let bbox = t.bounding_box().intersect(self.area);
            if bbox.width() <= 0.0 || bbox.height() <= 0.0 {
                continue;
            }
            self.visits_left = self.visits_left.saturating_add(MESH_VISITS_PER_TRIANGLE);

            for y in (bbox.y0.floor() as u16)..(bbox.y1.ceil() as u16) {
                for x in (bbox.x0.floor() as u16)..(bbox.x1.ceil() as u16) {
                    if self.visits_left == 0 {
                        warn!("mesh shading visit budget exceeded; the rest is not drawn");
                        return false;
                    }
                    self.visits_left -= 1;
                    let point = Point::new(x as f64, y as f64);
                    if t.contains_point(point) {
                        self.map.insert((x, y), t.interpolate(point));
                    }
                }
            }
        }
        true
    }
}

fn encode_function_shading(domain: &[f32; 4], function: &ShadingFunction) -> EncodedShadingType {
    let domain = Rect::new(
        domain[0] as f64,
        domain[2] as f64,
        domain[1] as f64,
        domain[3] as f64,
    );

    EncodedShadingType::FunctionBased {
        domain,
        function: function.clone(),
    }
}

#[derive(Debug)]
pub(crate) enum RadialAxialParams {
    Axial,
    Radial { p1: Point, r: Point },
}

#[derive(Debug)]
pub(crate) enum EncodedShadingType {
    FunctionBased {
        domain: Rect,
        function: ShadingFunction,
    },
    RadialAxial {
        function: ShadingFunction,
        params: RadialAxialParams,
        domain: [f32; 2],
        extend: [bool; 2],
    },
    Sampled {
        samples: FxHashMap<(u16, u16), ColorComponents>,
        function: Option<ShadingFunction>,
    },
    Dummy,
}

impl EncodedShadingType {
    pub(crate) fn eval(
        &self,
        pos: Point,
        bg_color: AlphaColor,
        color_space: &ColorSpace,
    ) -> Option<AlphaColor> {
        match self {
            Self::FunctionBased { domain, function } => {
                if !domain.contains(pos) {
                    Some(bg_color)
                } else {
                    let out = function.eval(&smallvec![pos.x as f32, pos.y as f32])?;
                    // TODO: Clamp out-of-range values.
                    Some(color_space.to_rgba(&out, 1.0, false))
                }
            }
            Self::RadialAxial {
                function,
                params,
                domain,
                extend,
            } => {
                let (t0, t1) = (domain[0], domain[1]);

                let mut t = match params {
                    RadialAxialParams::Axial => pos.x as f32,
                    RadialAxialParams::Radial { p1, r } => {
                        radial_pos(&pos, p1, *r, extend[0], extend[1]).unwrap_or(f32::MIN)
                    }
                };

                if t == f32::MIN {
                    return Some(bg_color);
                }

                if t < 0.0 {
                    if extend[0] {
                        t = 0.0;
                    } else {
                        return Some(bg_color);
                    }
                } else if t > 1.0 {
                    if extend[1] {
                        t = 1.0;
                    } else {
                        return Some(bg_color);
                    }
                }

                let t = t0 + (t1 - t0) * t;

                let val = function.eval(&smallvec![t])?;

                Some(color_space.to_rgba(&val, 1.0, false))
            }
            Self::Sampled { samples, function } => {
                let sample_point = (pos.x as u16, pos.y as u16);

                if let Some(color) = samples.get(&sample_point) {
                    if let Some(function) = function {
                        let val = function.eval(&color.to_smallvec())?;
                        Some(color_space.to_rgba(&val, 1.0, false))
                    } else {
                        Some(color_space.to_rgba(color, 1.0, false))
                    }
                } else {
                    Some(bg_color)
                }
            }
            Self::Dummy => Some(AlphaColor::TRANSPARENT),
        }
    }
}

fn ts_from_line_to_line(src1: Point, src2: Point, dst1: Point, dst2: Point) -> Affine {
    let unit_to_line1 = unit_to_line(src1, src2);
    let line1_to_unit = unit_to_line1.inverse();
    let unit_to_line2 = unit_to_line(dst1, dst2);

    unit_to_line2 * line1_to_unit
}

fn unit_to_line(p0: Point, p1: Point) -> Affine {
    Affine::new([
        p1.y - p0.y,
        p0.x - p1.x,
        p1.x - p0.x,
        p1.y - p0.y,
        p0.x,
        p0.y,
    ])
}

fn radial_pos(
    pos: &Point,
    p1: &Point,
    r: Point,
    min_extend: bool,
    max_extend: bool,
) -> Option<f32> {
    let r0 = r.x as f32;
    let dx = p1.x as f32;
    let dy = p1.y as f32;
    let dr = r.y as f32 - r0;

    let px = pos.x as f32;
    let py = pos.y as f32;

    let a = dx * dx + dy * dy - dr * dr;
    let b = -2.0 * (px * dx + py * dy + r0 * dr);
    let c = px * px + py * py - r0 * r0;

    let discriminant = b * b - 4.0 * a * c;

    // No solution available.
    if discriminant < 0.0 {
        return None;
    }

    if a.abs() < 1e-6 {
        if b.abs() < 1e-6 {
            return None;
        }

        let t = -c / b;

        if (!min_extend && t < 0.0) || (!max_extend && t > 1.0) {
            return None;
        }

        let r_t = r0 + dr * t;
        if r_t < 0.0 {
            return None;
        }

        return Some(t);
    }

    let sqrt_d = discriminant.sqrt();
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    let max = t1.max(t2);
    let mut take_max = Some(max);
    let min = t1.min(t2);
    let mut take_min = Some(min);

    if (!min_extend && min < 0.0) || r0 + dr * min < 0.0 {
        take_min = None;
    }

    if (!max_extend && max > 1.0) || r0 + dr * max < 0.0 {
        take_max = None;
    }

    match (take_min, take_max) {
        (Some(_), Some(max)) => Some(max),
        (Some(min), None) => Some(min),
        (None, Some(max)) => Some(max),
        (None, None) => None,
    }
}

#[cfg(test)]
mod mesh_bounds_tests {
    use super::*;
    use crate::shading::{CoonsPatch, TensorProductPatch};

    const AREA: Rect = Rect::new(-1.0, -1.0, 41.0, 41.0);

    fn colors() -> [ColorComponents; 4] {
        std::array::from_fn(|_| smallvec![0.0, 0.0, 1.0])
    }

    #[test]
    fn coons_subtraction_can_enter_the_page_outside_its_control_hull() {
        let mut points = [Point::new(80.0, 0.0); 12];
        for index in [0, 3, 6, 9] {
            points[index].x = 200.0;
        }
        for (index, point) in points.iter_mut().enumerate() {
            point.y = match index {
                0 | 9 | 10 | 11 => 0.0,
                1 | 8 => 40.0 / 3.0,
                2 | 7 => 80.0 / 3.0,
                _ => 40.0,
            };
        }
        let patch = CoonsPatch {
            control_points: points,
            colors: colors(),
        };
        assert!(points.iter().all(|point| point.x > AREA.x1));
        assert!(
            patch
                .map_coordinate(Point::new(0.5, 0.5))
                .distance(Point::new(20.0, 20.0))
                < 1e-10
        );
        assert!(!patch_outside_area(
            &points,
            Affine::IDENTITY,
            Some(AREA),
            true
        ));
    }

    #[test]
    fn nonfinite_unknown_and_touching_bounds_keep_the_original_path() {
        let points = [Point::new(100.0, 100.0); 16];
        assert!(patch_outside_area(
            &points,
            Affine::IDENTITY,
            Some(AREA),
            false
        ));
        assert!(!patch_outside_area(&points, Affine::IDENTITY, None, false));
        assert!(!patch_outside_area(
            &points,
            Affine::scale(f64::INFINITY),
            Some(AREA),
            false
        ));
        let mut malformed = points;
        malformed[12].x = f64::NAN;
        assert!(!patch_outside_area(
            &malformed,
            Affine::IDENTITY,
            Some(AREA),
            false
        ));
        assert!(!patch_outside_area(
            &points,
            Affine::IDENTITY,
            Some(Rect::new(0.0, 0.0, f64::NAN, 40.0)),
            false
        ));
        let touching = [Point::new(41.0, 20.0); 16];
        assert!(!patch_outside_area(
            &touching,
            Affine::IDENTITY,
            Some(AREA),
            false
        ));
        let huge = [Point::new(f64::MAX, f64::MAX); 16];
        assert!(!patch_outside_area(
            &huge,
            Affine::IDENTITY,
            Some(AREA),
            false
        ));
    }

    #[test]
    fn raster_triangle_inflation_can_reach_the_page_from_outside() {
        let mut points = [Point::ZERO; 16];
        for (index, (u, v)) in [
            (0, 0),
            (0, 1),
            (0, 2),
            (0, 3),
            (1, 3),
            (2, 3),
            (3, 3),
            (3, 2),
            (3, 1),
            (3, 0),
            (2, 0),
            (1, 0),
            (1, 1),
            (1, 2),
            (2, 2),
            (2, 1),
        ]
        .into_iter()
        .enumerate()
        {
            points[index] = Point::new(42.0 + u as f64 * 10000.0 / 3.0, v as f64 * 40.0 / 3.0);
        }
        let patch = TensorProductPatch {
            control_points: points,
            colors: colors(),
        };
        assert!(points.iter().all(|point| point.x > AREA.x1));
        let mut triangles = Vec::new();
        patch.to_triangles(&mut triangles);
        assert_eq!(triangles.len(), PATCH_TRIANGLE_COUNT);
        assert!(triangles.iter().any(|t| {
            [t.p0.point, t.p1.point, t.p2.point]
                .iter()
                .any(|p| p.x < 40.0)
        }));
        assert!(!patch_outside_area(
            &points,
            Affine::IDENTITY,
            Some(AREA),
            false
        ));
    }

    #[test]
    fn skipped_patches_spend_the_same_page_triangle_budget() {
        reset_mesh_budget();
        let patches = MAX_PAGE_MESH_TRIANGLES / PATCH_TRIANGLE_COUNT as u64;
        for _ in 0..patches {
            assert!(take_mesh_budget(PATCH_TRIANGLE_COUNT));
        }
        assert_eq!(
            MESH_TRIANGLES_LEFT.with(core::cell::Cell::get),
            MAX_PAGE_MESH_TRIANGLES % PATCH_TRIANGLE_COUNT as u64
        );
        assert!(!take_mesh_budget(PATCH_TRIANGLE_COUNT));
        assert!(!mesh_budget_left());
        assert!(!take_mesh_budget(PATCH_TRIANGLE_COUNT));
        reset_mesh_budget();
    }

    #[test]
    fn conservative_bounds_never_reject_sampled_triangles_across_affine_transforms() {
        let mut seed = 0x16a2_83c5_u64;
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 32) % 10001) as f64 / 10000.0
        };
        let transforms = [
            Affine::IDENTITY,
            Affine::new([-1.0, 0.25, 0.5, 1.0, 40.0, -10.0]),
            Affine::new([0.2, 1.0, -1.0, 0.1, 12.0, 20.0]),
        ];
        let mut rejected = 0;
        let mut retained = 0;
        for sample in 0..180 {
            let center = if sample % 3 == 0 {
                20.0
            } else {
                500.0 * (random() - 0.5)
            };
            let extent = 10.0 + 80.0 * random();
            let points: [Point; 16] = std::array::from_fn(|_| {
                Point::new(
                    center + extent * (random() - 0.5),
                    center + extent * (random() - 0.5),
                )
            });
            for transform in transforms {
                for coons in [false, true] {
                    let first: [Point; 12] = std::array::from_fn(|i| points[i]);
                    let outside = if coons {
                        patch_outside_area(&first, transform, Some(AREA), true)
                    } else {
                        patch_outside_area(&points, transform, Some(AREA), false)
                    };
                    if !outside {
                        retained += 1;
                        continue;
                    }
                    rejected += 1;
                    let mut triangles = Vec::new();
                    if coons {
                        CoonsPatch {
                            control_points: first,
                            colors: colors(),
                        }
                        .to_triangles(&mut triangles);
                    } else {
                        TensorProductPatch {
                            control_points: points,
                            colors: colors(),
                        }
                        .to_triangles(&mut triangles);
                    }
                    for t in triangles {
                        let bounds = [t.p0.point, t.p1.point, t.p2.point]
                            .into_iter()
                            .map(|p| transform * p)
                            .fold(
                                Rect::new(
                                    f64::INFINITY,
                                    f64::INFINITY,
                                    f64::NEG_INFINITY,
                                    f64::NEG_INFINITY,
                                ),
                                |r, p| r.union_pt(p),
                            );
                        assert!(
                            bounds.x1 < AREA.x0
                                || bounds.y1 < AREA.y0
                                || bounds.x0 > AREA.x1
                                || bounds.y0 > AREA.y1,
                            "culled a triangle that may reach the area"
                        );
                    }
                }
            }
        }
        assert!(rejected > 100 && retained > 100);
    }

    fn original_mesh_stream() -> Vec<u8> {
        let mut data = vec![0_u8];
        for point in [
            (0, 0),
            (0, 85),
            (0, 170),
            (0, 255),
            (85, 255),
            (170, 255),
            (255, 255),
            (255, 170),
            (255, 85),
            (255, 0),
            (170, 0),
            (85, 0),
        ] {
            data.extend_from_slice(&[point.0, point.1]);
        }
        data.extend_from_slice(&[0, 0, 255].repeat(4));
        let mut stream = format!("<< /ShadingType 6 /ColorSpace /DeviceRGB /BitsPerCoordinate 8 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [0 40 0 40 0 1 0 1 0 1] /Background [0 1 0] /BBox [5 5 35 35] /Length {} >>\nstream\n",data.len()).into_bytes();
        stream.extend(data);
        stream.extend_from_slice(b"\nendstream");
        stream
    }

    fn pattern(shading: crate::shading::Shading) -> ShadingPattern {
        ShadingPattern {
            shading: std::sync::Arc::new(shading),
            matrix: Affine::IDENTITY,
            opacity: 0.5,
            transfer_function: None,
        }
    }

    #[test]
    fn exhausted_direct_paints_keep_background_clip_opacity_and_nonmesh_shading() {
        use crate::{cache::Cache, shading::Shading};
        use hayro_syntax::object::{Dict, FromBytes, Stream};
        use kurbo::Shape;
        let bytes = original_mesh_stream();
        let stream = Stream::from_bytes(&bytes).unwrap();
        let cache = Cache::new();
        MESH_TRIANGLES_LEFT.with(|left| left.set(0));
        let direct = Shading::new_for_paint(stream.dict(), Some(&stream), &cache).unwrap();
        assert_eq!(
            direct.clip_path.as_ref().unwrap().bounding_box(),
            Rect::new(5.0, 5.0, 35.0, 35.0)
        );
        assert_eq!(
            pattern(direct)
                .encode_within(Some(AREA))
                .sample(Point::new(20.0, 20.0)),
            [0.0, 1.0, 0.0, 0.5]
        );
        let axial = Dict::from_bytes(b"<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 40 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [1 0 0] /N 1 >> /Extend [true true] >>").unwrap();
        let axial = pattern(Shading::new_for_paint(&axial, None, &cache).unwrap())
            .encode_within(Some(AREA));
        assert_eq!(
            axial.sample(axial.base_transform * Point::new(20.0, 20.0)),
            [1.0, 0.0, 0.0, 0.5]
        );
        reset_mesh_budget();
    }

    #[test]
    fn reusable_pattern_parsing_is_not_poisoned_by_an_exhausted_page() {
        use crate::{cache::Cache, shading::Shading};
        use hayro_syntax::object::{FromBytes, Stream};
        let bytes = original_mesh_stream();
        let stream = Stream::from_bytes(&bytes).unwrap();
        let cache = Cache::new();
        MESH_TRIANGLES_LEFT.with(|left| left.set(0));
        let retained = pattern(Shading::new(stream.dict(), Some(&stream), &cache).unwrap());
        assert!(
            matches!(retained.shading.shading_type.as_ref(),ShadingType::CoonsPatchMesh{patches,..} if patches.len()==1)
        );
        reset_mesh_budget();
        assert_eq!(
            retained
                .encode_within(Some(AREA))
                .sample(Point::new(20.0, 20.0)),
            [0.0, 0.0, 1.0, 0.5]
        );
        let fresh = pattern(Shading::new_for_paint(stream.dict(), Some(&stream), &cache).unwrap());
        assert_eq!(
            fresh
                .encode_within(Some(AREA))
                .sample(Point::new(20.0, 20.0)),
            [0.0, 0.0, 1.0, 0.5]
        );
        reset_mesh_budget();
    }

    #[test]
    fn exhausted_direct_meshes_still_require_the_original_dictionary_fields() {
        use crate::{cache::Cache, shading::Shading};
        use hayro_syntax::object::{FromBytes, Stream};
        let bytes = original_mesh_stream();
        for (from, to) in [
            ("/BitsPerCoordinate 8", ""),
            ("/BitsPerComponent 8", ""),
            ("/BitsPerFlag 8", ""),
            ("/Decode [0 40 0 40 0 1 0 1 0 1]", "/Decode [0 40 0]"),
            ("/ShadingType 6", "/ShadingType 5 /VerticesPerRow 0"),
            (
                "/ShadingType 6",
                "/ShadingType 5 /VerticesPerRow 4294967295",
            ),
        ] {
            let prefix = bytes.iter().position(|b| *b == b'\n').unwrap();
            let mut malformed = String::from_utf8(bytes[..prefix].to_vec())
                .unwrap()
                .replace(from, to)
                .into_bytes();
            malformed.extend_from_slice(&bytes[prefix..]);
            let stream = Stream::from_bytes(&malformed).unwrap();
            MESH_TRIANGLES_LEFT.with(|left| left.set(0));
            assert!(Shading::new_for_paint(stream.dict(), Some(&stream), &Cache::new()).is_none());
        }
        reset_mesh_budget();
    }

    #[test]
    fn constant_empty_mesh_color_preserves_background_opacity_and_transfer() {
        use crate::{cache::Cache, function::Function, shading::Shading};
        use hayro_syntax::object::{FromBytes, Object, Stream};
        let bytes = original_mesh_stream();
        let stream = Stream::from_bytes(&bytes).unwrap();
        MESH_TRIANGLES_LEFT.with(|left| left.set(0));
        let shading = Shading::new_for_paint(stream.dict(), Some(&stream), &Cache::new()).unwrap();
        let mut encoded = pattern(shading).encode_within(Some(AREA));
        assert_eq!(encoded.constant_sampled_color(), Some([0.0, 1.0, 0.0, 0.5]));
        let transfer =
            Object::from_bytes(b"<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>")
                .unwrap();
        encoded.transfer_function = Some(ActiveTransferFunction::Single(
            Function::new(&transfer).unwrap(),
        ));
        assert_eq!(encoded.constant_sampled_color(), Some([1.0, 0.0, 1.0, 0.5]));
        for point in [
            Point::new(-100.0, 17.0),
            Point::new(20.0, 20.0),
            Point::new(800.0, 800.0),
        ] {
            assert_eq!(
                encoded.constant_sampled_color(),
                Some(encoded.sample(point))
            );
        }
        encoded.background_color = AlphaColor::TRANSPARENT;
        assert_eq!(encoded.constant_sampled_color(), Some([1.0, 1.0, 1.0, 0.0]));
        let transparent = AlphaColor::new(
            AlphaColor::new(encoded.constant_sampled_color().unwrap()).premultiplied(),
        )
        .to_rgba8();
        assert_eq!(transparent, [0; 4]);
        encoded.transfer_function = None;
        encoded.background_color = AlphaColor::new([1000.0, 0.0, 0.0, 0.001]);
        let invalid = AlphaColor::new(
            AlphaColor::new(encoded.constant_sampled_color().unwrap()).premultiplied(),
        )
        .to_rgba8();
        assert_eq!(invalid[3], 0);
        assert_ne!(
            invalid, [0; 4],
            "alpha alone cannot identify transparent pixels"
        );

        if let EncodedShadingType::Sampled { samples, .. } = &mut encoded.shading_type {
            samples.insert((0, 0), smallvec![0.0, 0.0, 1.0]);
        }
        assert!(encoded.constant_sampled_color().is_none());
        encoded.shading_type = EncodedShadingType::Dummy;
        assert!(encoded.constant_sampled_color().is_none());
        reset_mesh_budget();
    }
}
