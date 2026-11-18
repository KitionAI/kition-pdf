use crate::font::UNITS_PER_EM;
use crate::font::outline::OutlinePath;
use kurbo::BezPath;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::GlyphMetrics;
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, Target};
use skrifa::raw::TableProvider;
use skrifa::raw::ps::cff::{
    CffFontRef, Subfont,
    charset::{Charset, CharsetKind, CustomCharset},
    v1::Cff,
};
use skrifa::raw::ps::string::Sid;
use skrifa::raw::ps::type1::Type1Font;
use skrifa::raw::tables::post::DEFAULT_GLYPH_NAMES;
use skrifa::raw::{FontData as ReadFontData, FontRead};
use skrifa::{FontRef, GlyphId, MetadataProvider, OutlineGlyphCollection};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use yoke::{Yoke, Yokeable};

type FontData = Arc<dyn AsRef<[u8]> + Send + Sync>;
type OpenTypeFontYoke = Yoke<OTFYoke<'static>, FontData>;
type CffFontYoke = Yoke<CFFYoke<'static>, FontData>;

/// A font blob for type 1 fonts.
#[derive(Clone)]
pub(crate) struct Type1FontBlob(Arc<Type1Font>);

impl Debug for Type1FontBlob {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Type1 Font {{ .. }}")
    }
}

impl Type1FontBlob {
    pub(crate) fn new(data: FontData) -> Option<Self> {
        let table = Type1Font::new(data.as_ref().as_ref()).ok()?;
        Some(Self(Arc::new(table)))
    }

    pub(crate) fn table(&self) -> &Type1Font {
        self.0.as_ref()
    }

    pub(crate) fn outline_glyph(&self, gid: GlyphId) -> BezPath {
        let mut path = OutlinePath::new();
        let _ = self.table().draw(gid, Some(UNITS_PER_EM), &mut path);

        path.take()
    }
}

/// A font blob for CFF-based fonts.
#[derive(Clone)]
pub(crate) struct CffFontBlob(Arc<CffFontYoke>);

impl Debug for CffFontBlob {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Type1 Font {{ .. }}")
    }
}

impl CffFontBlob {
    pub(crate) fn new(data: FontData) -> Option<Self> {
        let font = CffFontRef::new(data.as_ref().as_ref(), 0, None).ok()?;
        let cff = Cff::read(ReadFontData::new(data.as_ref().as_ref())).ok()?;
        let charset = font.charset();
        let subfonts = (0..font.num_subfonts())
            .map(|index| font.subfont(index, &[]).ok())
            .collect::<Option<Vec<_>>>()?;

        let yoke = Yoke::<CFFYoke<'static>, FontData>::attach_to_cart(data.clone(), |data| {
            let bytes = data.as_ref();
            let font = CffFontRef::new(bytes, 0, None).unwrap();
            let cff = Cff::read(ReadFontData::new(bytes)).unwrap();
            let charset = font.charset();
            let subfonts = (0..font.num_subfonts())
                .map(|index| font.subfont(index, &[]).unwrap())
                .collect();
            CFFYoke {
                font,
                cff,
                charset,
                subfonts,
            }
        });

        let _ = (cff, charset, subfonts);
        Some(Self(Arc::new(yoke)))
    }

    pub(crate) fn font_data(&self) -> FontData {
        self.0.backing_cart().clone()
    }

    pub(crate) fn font(&self) -> &CffFontRef<'_> {
        &self.0.as_ref().get().font
    }

    fn charset(&self) -> Option<&Charset<'_>> {
        self.0.as_ref().get().charset.as_ref()
    }

    fn subfont(&self, glyph: GlyphId) -> Option<&Subfont> {
        let index = self.font().subfont_index(glyph)? as usize;
        self.0.as_ref().get().subfonts.get(index)
    }

    pub(crate) fn outline_glyph(&self, glyph: GlyphId) -> BezPath {
        let mut path = OutlinePath::new();
        let Some(subfont) = self.subfont(glyph) else {
            return BezPath::new();
        };

        let _ = self
            .font()
            .draw(subfont, glyph, &[], Some(UNITS_PER_EM), &mut path);

        path.take()
    }

    pub(crate) fn glyph_names(&self) -> Vec<(GlyphId, String)> {
        let Some(charset) = self.charset() else {
            return Vec::new();
        };

        // TODO: Avoid collecting here.
        charset
            .iter()
            .filter_map(|(gid, sid)| {
                let bytes = self.0.as_ref().get().cff.string(sid)?;
                let name = std::str::from_utf8(bytes).ok()?.to_string();
                Some((gid, name))
            })
            .collect()
    }

    pub(crate) fn glyph_index_by_name(&self, name: &str) -> Option<GlyphId> {
        // TODO: This is probably slow to do repeatedly?
        self.charset()?.iter().find_map(|(gid, sid)| {
            let bytes = self.0.as_ref().get().cff.string(sid)?;
            (bytes == name.as_bytes()).then_some(gid)
        })
    }

    pub(crate) fn glyph_index_by_cid(&self, cid: u16) -> Option<GlyphId> {
        bounded_cff_glyph_index(self.charset()?, self.num_glyphs(), cid)
    }

    pub(crate) fn glyph_index(&self, code: u8) -> Option<GlyphId> {
        self.font()
            .encoding()
            .and_then(|encoding| encoding.map(code))
    }

    pub(crate) fn num_glyphs(&self) -> u32 {
        self.font().num_glyphs()
    }

    pub(crate) fn is_cid(&self) -> bool {
        self.font().is_cid()
    }
}

// CFF charset tables have no byte length. The parsed slices can include later
// font data, so limit lookup to the glyphs in CharStrings before inspecting them.
fn bounded_cff_glyph_index(charset: &Charset<'_>, num_glyphs: u32, cid: u16) -> Option<GlyphId> {
    if num_glyphs == 0 {
        return None;
    }
    if cid == 0 {
        return Some(GlyphId::NOTDEF);
    }
    match charset.kind() {
        CharsetKind::Custom(CustomCharset::Format0(format)) => {
            let count = usize::try_from(num_glyphs.checked_sub(1)?).ok()?;
            let position =
                format.glyph().iter().take(count).position(|entry| entry.get() == cid)?;
            u32::try_from(position).ok()?.checked_add(1).map(GlyphId::new)
        }
        CharsetKind::Custom(CustomCharset::Format1(format)) => bounded_cff_range_index(
            cid,
            num_glyphs,
            format.ranges().iter().map(|range| (range.first.get(), u16::from(range.n_left))),
        ),
        CharsetKind::Custom(CustomCharset::Format2(format)) => bounded_cff_range_index(
            cid,
            num_glyphs,
            format.ranges().iter().map(|range| (range.first.get(), range.n_left.get())),
        ),
        // Predefined charsets are finite tables, but can describe more glyphs
        // than this font actually contains.
        CharsetKind::IsoAdobe | CharsetKind::Expert | CharsetKind::ExpertSubset => {
            charset.glyph_id(Sid::new(cid)).filter(|glyph| glyph.to_u32() < num_glyphs)
        }
    }
}

fn bounded_cff_range_index(
    cid: u16,
    num_glyphs: u32,
    ranges: impl IntoIterator<Item = (u16, u16)>,
) -> Option<GlyphId> {
    let mut ranges = ranges.into_iter();
    let mut first_gid = 1_u32;
    // Check the glyph limit before reading another range. Range lookup stays
    // proportional to range count, without expanding large CJK ranges.
    while first_gid < num_glyphs {
        let (first_cid, n_left) = ranges.next()?;
        let remaining = num_glyphs.checked_sub(first_gid)?;
        let covered = u32::from(n_left).checked_add(1)?.min(remaining);
        if let Some(offset) = u32::from(cid).checked_sub(u32::from(first_cid))
            && offset < covered
        {
            return first_gid.checked_add(offset).map(GlyphId::new);
        }
        first_gid = first_gid.checked_add(covered)?;
    }
    None
}

/// A font blob for OpenType fonts.
#[derive(Clone)]
pub(crate) struct OpenTypeFontBlob {
    yoke: Arc<OpenTypeFontYoke>,
    cff_blob: Option<CffFontBlob>,
}

impl Debug for OpenTypeFontBlob {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpenType Font {{ .. }}")
    }
}

impl OpenTypeFontBlob {
    pub(crate) fn new(data: FontData, index: u32) -> Option<Self> {
        // Check first whether the font is valid so we can unwrap in the closure.
        let f = FontRef::from_index(data.as_ref().as_ref(), index).ok()?;
        // Reject fonts with invalid post table version, fixes pdf.js issue 9462. Not sure if there
        // is a better fix, for some reason skrifa accepts the font which is completely invalid.
        let invalid = f.post().is_ok_and(|p| {
            !matches!(
                p.version().to_major_minor(),
                (1, 0) | (2, 0) | (2, 5) | (3, 0)
            )
        });

        if invalid {
            return None;
        }

        // We store this separately because we want to be able to cache the subfonts
        // of a CFF OpenType font, which is not possible using the current skrifa API.
        // Hopefully there will be some way in the future.
        let cff_blob = f
            .cff()
            .ok()
            .map(|cff| Arc::new(cff.offset_data().as_ref().to_vec()) as FontData)
            .and_then(CffFontBlob::new);

        let font_ref_yoke =
            Yoke::<OTFYoke<'static>, FontData>::attach_to_cart(data.clone(), |data| {
                let font_ref = FontRef::from_index(data.as_ref(), index).unwrap();

                let hinting_instance = if font_ref.outline_glyphs().require_interpreter() {
                    HintingInstance::new(
                        &font_ref.outline_glyphs(),
                        Size::new(UNITS_PER_EM),
                        LocationRef::default(),
                        HintingOptions {
                            engine: Engine::Interpreter,
                            target: Target::Mono,
                        },
                    )
                    .ok()
                } else {
                    None
                };

                OTFYoke {
                    font_ref: font_ref.clone(),
                    outline_glyphs: font_ref.outline_glyphs(),
                    hinting_instance,
                    glyph_metrics: font_ref
                        .glyph_metrics(Size::new(UNITS_PER_EM), LocationRef::default()),
                }
            });

        Some(Self {
            yoke: Arc::new(font_ref_yoke),
            cff_blob,
        })
    }

    pub(crate) fn font_data(&self) -> FontData {
        self.yoke.backing_cart().clone()
    }

    /// Map a CID only when the cached CFF table is CID-keyed. `None` leaves
    /// non-CID OpenType fonts on their existing direct-GID path; a missing CID
    /// in a CID-keyed font explicitly selects `.notdef`.
    pub(crate) fn glyph_index_by_cid(&self, cid: u16) -> Option<GlyphId> {
        let cff = self.cff_blob.as_ref().filter(|cff| cff.is_cid())?;
        Some(
            cff.glyph_index_by_cid(cid)
                // Retain a defensive bound before selecting an outline from the
                // shared bounded charset lookup.
                .filter(|glyph| glyph.to_u32() < cff.num_glyphs())
                .unwrap_or(GlyphId::NOTDEF),
        )
    }

    pub(crate) fn font_ref(&self) -> &FontRef<'_> {
        &self.yoke.as_ref().get().font_ref
    }

    pub(crate) fn glyph_metrics(&self) -> &GlyphMetrics<'_> {
        &self.yoke.as_ref().get().glyph_metrics
    }

    pub(crate) fn glyph_names(&self) -> HashMap<String, GlyphId> {
        // Note: We don't call the `glyph_name` method provided by read-fonts because
        // calling it repeatedly is very slow.
        let mut glyph_names = HashMap::new();
        let Ok(post) = self.font_ref().post() else {
            return glyph_names;
        };

        match post.version().to_major_minor() {
            (1, 0) => {
                for (gid, name) in DEFAULT_GLYPH_NAMES.iter().enumerate() {
                    glyph_names.insert((*name).to_string(), GlyphId::new(gid as u32));
                }
            }
            (2, 0) => {
                let Some(name_indices) = post.glyph_name_index() else {
                    return glyph_names;
                };
                let Some(string_data) = post.string_data() else {
                    return glyph_names;
                };
                let custom_names = string_data
                    .iter()
                    .map(|entry| entry.ok().map(|name| name.as_str()))
                    .collect::<Vec<_>>();

                for (gid, idx) in name_indices.iter().enumerate() {
                    let idx = idx.get() as usize;
                    if let Some(name) = DEFAULT_GLYPH_NAMES.get(idx).copied().or_else(|| {
                        custom_names
                            .get(idx.saturating_sub(DEFAULT_GLYPH_NAMES.len()))
                            .copied()
                            .flatten()
                    }) {
                        glyph_names.insert(name.to_string(), GlyphId::new(gid as u32));
                    }
                }
            }
            _ => {}
        }

        glyph_names
    }

    fn outline_glyphs(&self) -> &OutlineGlyphCollection<'_> {
        &self.yoke.as_ref().get().outline_glyphs
    }

    pub(crate) fn outline_glyph(&self, glyph: GlyphId) -> BezPath {
        if let Some(cff_blob) = self.cff_blob.as_ref() {
            return cff_blob.outline_glyph(glyph);
        }

        let mut path = OutlinePath::new();

        let draw_settings = if let Some(instance) = self.yoke.get().hinting_instance.as_ref() {
            // Note: We always hint at the font size `UNITS_PER_EM`, which obviously isn't very useful. We don't do this
            // for better text quality (right now), but instead because there are some PDFs with obscure fonts that
            // actually render wrongly if hinting is disabled!
            DrawSettings::hinted(instance, false)
        } else {
            DrawSettings::unhinted(Size::new(UNITS_PER_EM), LocationRef::default())
        };

        let Some(outline) = self.outline_glyphs().get(glyph) else {
            return BezPath::new();
        };

        let _ = outline.draw(draw_settings, &mut path);
        path.take()
    }
}

#[derive(Yokeable, Clone)]
struct OTFYoke<'a> {
    font_ref: FontRef<'a>,
    glyph_metrics: GlyphMetrics<'a>,
    hinting_instance: Option<HintingInstance>,
    outline_glyphs: OutlineGlyphCollection<'a>,
}

#[derive(Yokeable, Clone)]
struct CFFYoke<'a> {
    font: CffFontRef<'a>,
    cff: Cff<'a>,
    charset: Option<Charset<'a>>,
    subfonts: Vec<Subfont>,
}

#[cfg(test)]
mod cff_charset_tests {
    use super::{bounded_cff_glyph_index, bounded_cff_range_index};
    use skrifa::GlyphId;
    use skrifa::raw::{FontData, ps::cff::charset::Charset};
    use std::cell::Cell;

    #[test]
    fn range_lookup_stops_before_reading_past_charstrings_count() {
        for count in [0, 1] {
            let visits = Cell::new(0);
            let ranges = [(1, 0)].into_iter().inspect(|_| visits.set(visits.get() + 1));
            assert_eq!(bounded_cff_range_index(1, count, ranges), None);
            assert_eq!(visits.get(), 0);
        }
        for (cid, count, expected) in
            [(2, 3, Some(2)), (3, 3, None), (65_534, 65_535, Some(65_534)), (65_535, 65_535, None)]
        {
            let visits = Cell::new(0);
            let ranges =
                [(1, u16::MAX), (3, 0)].into_iter().inspect(|_| visits.set(visits.get() + 1));
            assert_eq!(bounded_cff_range_index(cid, count, ranges).map(GlyphId::to_u32), expected);
            assert_eq!(visits.get(), 1, "one large range remains one lookup step");
        }
    }

    #[test]
    fn range_lookup_preserves_first_match_and_later_representable_cids() {
        let visits = Cell::new(0);
        let duplicate = [(2, 0), (2, 0)].into_iter().inspect(|_| visits.set(visits.get() + 1));
        assert_eq!(bounded_cff_range_index(2, 3, duplicate), Some(GlyphId::new(1)));
        assert_eq!(visits.get(), 1);
        let visits = Cell::new(0);
        let overflow =
            [(u16::MAX, 1), (1, 0), (2, 0)].into_iter().inspect(|_| visits.set(visits.get() + 1));
        assert_eq!(bounded_cff_range_index(1, 4, overflow), Some(GlyphId::new(3)));
        assert_eq!(visits.get(), 2);
    }

    #[test]
    fn predefined_charset_lookup_respects_empty_and_short_charstrings() {
        for offset in [0, 1, 2] {
            for count in [0, 1, 3] {
                let charset = Charset::new(FontData::new(&[]), offset, count).unwrap();
                for cid in [0, 1, 2, 229, u16::MAX] {
                    let expected =
                        charset.iter().find_map(|(gid, sid)| (sid.to_u16() == cid).then_some(gid));
                    assert_eq!(bounded_cff_glyph_index(&charset, count, cid), expected);
                }
            }
        }
    }
}
