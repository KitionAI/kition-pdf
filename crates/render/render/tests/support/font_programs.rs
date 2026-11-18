//! Contributor-original unhinted geometric font programs, built from format fields.
//! No font bytes, outlines, names, or instructions are copied from another font.
use std::collections::BTreeMap;

pub const LEFT: &[(i16, i16)] = &[(100, 0), (100, 700), (300, 700), (300, 0)];
pub const RIGHT: &[(i16, i16)] = &[(500, 0), (500, 700), (700, 700), (700, 200), (900, 200), (900, 0)];

fn u16_at(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}
fn u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
fn dict_num(out: &mut Vec<u8>, value: usize) {
    out.push(29);
    out.extend_from_slice(&(value as i32).to_be_bytes());
}
fn index(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut out = (entries.len() as u16).to_be_bytes().to_vec();
    if entries.is_empty() {
        return out;
    }
    out.push(2);
    let mut offset = 1_u16;
    out.extend_from_slice(&offset.to_be_bytes());
    for entry in entries {
        offset += entry.len() as u16;
        out.extend_from_slice(&offset.to_be_bytes());
    }
    for entry in entries {
        out.extend_from_slice(entry);
    }
    out
}
fn charstring(points: &[(i16, i16)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut previous = (0, 0);
    for (i, &(x, y)) in points.iter().enumerate() {
        for delta in [x - previous.0, y - previous.1] {
            out.push(28);
            out.extend_from_slice(&delta.to_be_bytes());
        }
        out.push(if i == 0 { 21 } else { 5 });
        previous = (x, y);
    }
    out.push(14);
    out
}

/// CFF1 with three glyphs: empty .notdef, left bar, right L. Charset entries are
/// CIDs for CID-keyed fonts; name-keyed controls use the standard SIDs for A/B.
pub fn cff(cid: bool, swapped: bool) -> Vec<u8> {
    let ids = if cid { if swapped { [2_u16, 1] } else { [1, 2] } } else { [34, 35] };
    let mut charset = vec![0];
    for id in ids {
        charset.extend_from_slice(&id.to_be_bytes());
    }
    cff_with_charset(cid, &charset, &[])
}

/// The same original program with an explicitly authored charset and .notdef.
/// Bytes after the two charset entries may be original padding for bounds tests.
pub fn cff_with_charset(cid: bool, charset: &[u8], notdef: &[(i16, i16)]) -> Vec<u8> {
    cff_with_glyphs(cid, charset, &[notdef, LEFT, RIGHT])
}

/// Additional original glyphs support malformed-range controls; an empty charset
/// selects the predefined ISOAdobe SID table rather than an explicit table.
pub fn cff_with_glyphs(cid: bool, charset: &[u8], glyphs: &[&[(i16, i16)]]) -> Vec<u8> {
    let names = index(&[b"KitionGeometry".to_vec()]);
    let strings = index(&[b"Adobe".to_vec(), b"Identity".to_vec()]);
    let global_subrs = index(&[]);
    let charstrings = index(&glyphs.iter().map(|points| charstring(points)).collect::<Vec<_>>());
    let private = vec![29, 0, 0, 3, 232, 20]; // defaultWidthX = 1000.
    let top = |charset_at, chars_at, fd_at, select_at, private_at| {
        let mut data = Vec::new();
        if cid {
            for value in [391, 392, 0] {
                dict_num(&mut data, value);
            }
            data.extend_from_slice(&[12, 30]); // ROS.
            dict_num(&mut data, glyphs.len());
            data.extend_from_slice(&[12, 34]); // CIDCount.
        }
        for value in [0, 0, 1000, 700] {
            dict_num(&mut data, value);
        }
        data.push(5); // FontBBox.
        dict_num(&mut data, charset_at);
        data.push(15);
        dict_num(&mut data, chars_at);
        data.push(17);
        if cid {
            dict_num(&mut data, fd_at);
            data.extend_from_slice(&[12, 36]);
            dict_num(&mut data, select_at);
            data.extend_from_slice(&[12, 37]);
        } else {
            dict_num(&mut data, private.len());
            dict_num(&mut data, private_at);
            data.push(18);
        }
        data
    };
    let fd = |private_at| {
        let mut data = Vec::new();
        dict_num(&mut data, private.len());
        dict_num(&mut data, private_at);
        data.push(18);
        index(&[data])
    };
    let prefix_len = 4 + names.len() + index(&[top(0, 0, 0, 0, 0)]).len() + strings.len() + global_subrs.len();
    let chars_at = prefix_len + charset.len();
    let fd_at = chars_at + charstrings.len();
    let select_at = fd_at + if cid { fd(0).len() } else { 0 };
    let private_at = select_at + if cid { glyphs.len() + 1 } else { 0 };
    let mut out = vec![1, 0, 4, 4];
    out.extend(names);
    out.extend(index(&[top(if charset.is_empty() { 0 } else { prefix_len }, chars_at, fd_at, select_at, private_at)]));
    out.extend(strings);
    out.extend(global_subrs);
    assert_eq!(out.len(), prefix_len);
    out.extend(charset);
    out.extend(charstrings);
    if cid {
        out.extend(fd(private_at));
        out.extend(std::iter::repeat_n(0, glyphs.len() + 1)); // FDSelect format 0, all glyphs use FD 0.
    }
    assert_eq!(out.len(), private_at);
    out.extend(private);
    out
}

fn glyf(points: &[(i16, i16)]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in [1_i16, points.iter().map(|p| p.0).min().unwrap(), 0, points.iter().map(|p| p.0).max().unwrap(), 700] {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out.extend_from_slice(&((points.len() - 1) as u16).to_be_bytes());
    out.extend_from_slice(&0_u16.to_be_bytes()); // No TrueType instructions.
    out.extend(std::iter::repeat_n(1, points.len())); // On-curve points, signed 16-bit deltas.
    for axis in [0, 1] {
        let mut previous = 0;
        for &(x, y) in points {
            let value = if axis == 0 { x } else { y };
            out.extend_from_slice(&(value - previous).to_be_bytes());
            previous = value;
        }
    }
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out
}

fn names() -> Vec<u8> {
    let entries = [(1_u16, "Kition Geometry"), (2, "Regular"), (4, "Kition Geometry Regular"), (6, "KitionGeometry")];
    let mut out = vec![0; 6 + entries.len() * 12];
    u16_at(&mut out, 2, entries.len() as u16);
    u16_at(&mut out, 4, (6 + entries.len() * 12) as u16);
    let mut strings = Vec::new();
    for (i, (id, name)) in entries.iter().enumerate() {
        let bytes: Vec<u8> = name.encode_utf16().flat_map(u16::to_be_bytes).collect();
        for (j, value) in [3_u16, 1, 0x0409, *id, bytes.len() as u16, strings.len() as u16].into_iter().enumerate() {
            u16_at(&mut out, 6 + i * 12 + j * 2, value);
        }
        strings.extend(bytes);
    }
    out.extend(strings);
    out
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0_u32, |sum, chunk| {
        let mut value = [0; 4];
        value[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(value))
    })
}

/// Complete minimal sfnt tables, with either original TrueType contours or a
/// supplied original CFF table. Complete major tables avoid repair-path probes.
pub fn opentype(cff: Option<Vec<u8>>) -> Vec<u8> {
    let is_cff = cff.is_some();
    let mut tables = BTreeMap::new();
    let mut head = vec![0; 54];
    for (at, value) in [(0, 0x00010000), (4, 0x00010000), (12, 0x5f0f3cf5)] {
        u32_at(&mut head, at, value);
    }
    for (at, value) in [(16, 3), (18, 1000), (40, 1000), (42, 700), (46, 8), (48, 2), (50, if is_cff { 0 } else { 1 })] {
        u16_at(&mut head, at, value);
    }
    tables.insert(*b"head", head);
    let mut hhea = vec![0; 36];
    u32_at(&mut hhea, 0, 0x00010000);
    for (at, value) in [(4, 800), (6, -200), (10, 1000), (16, 900), (18, 1), (34, 3)] {
        u16_at(&mut hhea, at, value as u16);
    }
    tables.insert(*b"hhea", hhea);
    let mut hmtx = Vec::new();
    for bearing in [0_i16, 100, 500] {
        hmtx.extend_from_slice(&1000_u16.to_be_bytes());
        hmtx.extend_from_slice(&bearing.to_be_bytes());
    }
    tables.insert(*b"hmtx", hmtx);
    let mut maxp = vec![0; if is_cff { 6 } else { 32 }];
    u32_at(&mut maxp, 0, if is_cff { 0x00005000 } else { 0x00010000 });
    u16_at(&mut maxp, 4, 3);
    if !is_cff {
        for (at, value) in [(6, 6), (8, 1), (14, 2)] {
            u16_at(&mut maxp, at, value);
        }
    }
    tables.insert(*b"maxp", maxp);
    let mut post = vec![0; 32];
    u32_at(&mut post, 0, 0x00030000);
    u16_at(&mut post, 8, (-100_i16) as u16);
    u16_at(&mut post, 10, 50);
    tables.insert(*b"post", post);
    tables.insert(*b"name", names());
    let mut cmap = vec![0; 40];
    for (at, value) in [(2, 1), (4, 3), (6, 10), (12, 12)] {
        u16_at(&mut cmap, at, value);
    }
    for (at, value) in [(8, 12), (16, 28), (24, 1), (28, 65), (32, 66), (36, 1)] {
        u32_at(&mut cmap, at, value);
    }
    tables.insert(*b"cmap", cmap);
    let mut os2 = vec![0; 78];
    for (at, value) in [(2, 1000), (4, 400), (6, 5), (62, 64), (64, 65), (66, 66), (68, 800), (70, -200), (74, 800), (76, 200)] {
        u16_at(&mut os2, at, value as u16);
    }
    u32_at(&mut os2, 42, 1);
    os2[58..62].copy_from_slice(b"KITI");
    tables.insert(*b"OS/2", os2);
    if let Some(cff) = cff {
        tables.insert(*b"CFF ", cff);
    } else {
        let mut contours = glyf(LEFT);
        let mut loca = vec![0; 8]; // Empty .notdef uses offsets 0 and 0.
        loca.extend_from_slice(&(contours.len() as u32).to_be_bytes());
        contours.extend(glyf(RIGHT));
        loca.extend_from_slice(&(contours.len() as u32).to_be_bytes());
        tables.insert(*b"glyf", contours);
        tables.insert(*b"loca", loca);
    }
    let count = tables.len() as u16;
    let power = 1_u16 << count.ilog2();
    let mut out = vec![0; 12 + tables.len() * 16];
    out[..4].copy_from_slice(if is_cff { b"OTTO" } else { &[0, 1, 0, 0] });
    for (at, value) in [(4, count), (6, power * 16), (8, count.ilog2() as u16), (10, count * 16 - power * 16)] {
        u16_at(&mut out, at, value);
    }
    let mut head_offset = 0;
    for (i, (tag, bytes)) in tables.into_iter().enumerate() {
        let offset = out.len();
        let record = 12 + i * 16;
        out[record..record + 4].copy_from_slice(&tag);
        u32_at(&mut out, record + 4, checksum(&bytes));
        u32_at(&mut out, record + 8, offset as u32);
        u32_at(&mut out, record + 12, bytes.len() as u32);
        if tag == *b"head" {
            head_offset = offset;
        }
        out.extend(bytes);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    let adjustment = 0xb1b0afba_u32.wrapping_sub(checksum(&out));
    u32_at(&mut out, head_offset + 8, adjustment);
    assert_eq!(checksum(&out), 0xb1b0afba);
    out
}
