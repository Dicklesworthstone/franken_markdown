//! Clean-room TrueType / OpenType font reader.
//!
//! Parses the sfnt table directory plus the metric and character-map tables we
//! need to lay out and (later) embed text: `head` (units per em), `maxp` (glyph
//! count), `hhea`/`hmtx` (vertical metrics + advance widths), and `cmap`
//! (character → glyph, formats 4 and 12), and legacy `kern` format-0 pair
//! kerning. Latin-first, zero-dependency, and free of `unsafe`/`unwrap`/`panic`
//! — every read is bounds-checked.
//!
//! Outline parsing (`glyf`/`loca`/`CFF`) for PDF embedding + subsetting, and
//! ligatures / richer positioning (GPOS/GSUB), are later increments. This
//! module gives the real advance metrics the layout engine needs to replace the
//! base-14 approximations, and the `cmap` needed to map text to glyphs for
//! embedding.

/// A parsed font, owning its backing bytes.
#[derive(Debug, Clone)]
pub struct Font {
    data: Vec<u8>,
    /// Font design units per em (the coordinate scale; advances are in these).
    pub units_per_em: u16,
    /// Number of glyphs in the font.
    pub num_glyphs: u16,
    /// Typographic ascender (design units).
    pub ascent: i16,
    /// Typographic descender (design units, usually negative).
    pub descent: i16,
    /// Recommended extra line gap (design units).
    pub line_gap: i16,
    num_h_metrics: u16,
    hmtx_off: usize,
    cmap_off: usize,
    cmap_format: u16,
    /// `(offset, length)` of the `glyf` table, when the font has TrueType
    /// outlines. Absent for CFF/OpenType (`OTTO`) fonts.
    glyf: Option<(usize, usize)>,
    /// Offset of the `loca` table (glyph offsets into `glyf`).
    loca_off: Option<usize>,
    /// True when `loca` uses the 32-bit (long) offset format.
    loca_long: bool,
    /// `(pair_record_offset, pair_count)` for a legacy `kern` format-0 table.
    kern0: Option<(usize, u16)>,
}

/// Why a font failed to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    /// Not a recognized sfnt (`0x00010000`, `true`, or `OTTO`).
    BadMagic,
    /// A required table was absent.
    MissingTable(&'static str),
    /// The file ended before a required field could be read.
    Truncated,
    /// No usable Unicode `cmap` subtable (format 4 or 12) was found.
    NoUnicodeCmap,
}

impl core::fmt::Display for FontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not a TrueType/OpenType font"),
            Self::MissingTable(t) => write!(f, "missing required font table: {t}"),
            Self::Truncated => write!(f, "font data is truncated"),
            Self::NoUnicodeCmap => write!(f, "no usable Unicode cmap (format 4/12)"),
        }
    }
}

impl std::error::Error for FontError {}

fn be_u16(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(o)?, *d.get(o + 1)?]))
}
fn be_i16(d: &[u8], o: usize) -> Option<i16> {
    be_u16(d, o).map(|v| v as i16)
}
fn be_u32(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *d.get(o)?,
        *d.get(o + 1)?,
        *d.get(o + 2)?,
        *d.get(o + 3)?,
    ]))
}

/// Write a big-endian `u16` at `off` into a mutable buffer, bounds-checked.
fn write_u16(d: &mut [u8], off: usize, v: u16) -> Option<()> {
    let b = v.to_be_bytes();
    *d.get_mut(off)? = b[0];
    *d.get_mut(off + 1)? = b[1];
    Some(())
}

/// Write a big-endian `u32` at `off` into a mutable buffer, bounds-checked.
fn write_u32(d: &mut [u8], off: usize, v: u32) -> Option<()> {
    let b = v.to_be_bytes();
    *d.get_mut(off)? = b[0];
    *d.get_mut(off + 1)? = b[1];
    *d.get_mut(off + 2)? = b[2];
    *d.get_mut(off + 3)? = b[3];
    Some(())
}

/// sfnt table checksum: the wrapping `u32` sum of the table's bytes read as
/// big-endian 32-bit words, with the final partial word zero-padded.
fn table_checksum(d: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut chunks = d.chunks_exact(4);
    for c in &mut chunks {
        sum = sum.wrapping_add(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut buf = [0u8; 4];
        buf[..rem.len()].copy_from_slice(rem);
        sum = sum.wrapping_add(u32::from_be_bytes(buf));
    }
    sum
}

fn find_table(d: &[u8], tag: &[u8; 4]) -> Option<usize> {
    find_table_full(d, tag).map(|(off, _)| off)
}

/// Locate a table by tag, returning `(offset, length)`.
fn find_table_full(d: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    let num_tables = be_u16(d, 4)? as usize;
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        if d.get(rec..rec + 4)? == tag {
            return Some((be_u32(d, rec + 8)? as usize, be_u32(d, rec + 12)? as usize));
        }
    }
    None
}

/// Locate a legacy TrueType `kern` v0 format-0 horizontal pair table.
fn find_kern0(d: &[u8]) -> Option<(usize, u16)> {
    let (kern, kern_len) = find_table_full(d, b"kern")?;
    let table_end = kern.checked_add(kern_len)?;
    let version = be_u16(d, kern)?;
    let n_tables = be_u16(d, kern + 2)? as usize;
    if version != 0 {
        return None;
    }

    let mut sub = kern + 4;
    for _ in 0..n_tables {
        if sub.checked_add(6)? > table_end {
            return None;
        }
        let length = be_u16(d, sub + 2)? as usize;
        let coverage = be_u16(d, sub + 4)?;
        let format = coverage >> 8;
        let horizontal = coverage & 0x0001 != 0;
        let minimum = coverage & 0x0002 != 0;
        let pairs = sub + 14;
        if format == 0 && horizontal && !minimum && length >= 14 {
            let sub_end = sub.checked_add(length)?;
            if sub_end > table_end {
                return None;
            }
            let n_pairs = be_u16(d, sub + 6)?;
            let bytes_needed = (n_pairs as usize).checked_mul(6)?;
            if pairs.checked_add(bytes_needed)? <= sub_end {
                return Some((pairs, n_pairs));
            }
            return None;
        }
        if length == 0 {
            return None;
        }
        sub = sub.checked_add(length)?;
    }
    None
}

impl Font {
    /// Parse a font from its raw bytes (e.g. an `include_bytes!` blob).
    ///
    /// # Errors
    /// Returns a [`FontError`] for a non-sfnt file, a missing required table, a
    /// truncated file, or the absence of a usable Unicode `cmap`.
    pub fn parse(data: Vec<u8>) -> Result<Self, FontError> {
        let d = data.as_slice();
        let magic = be_u32(d, 0).ok_or(FontError::Truncated)?;
        // 0x00010000 = TrueType outlines; "true"; "OTTO" = CFF/OpenType.
        if magic != 0x0001_0000 && magic != 0x7472_7565 && magic != 0x4F54_544F {
            return Err(FontError::BadMagic);
        }

        let head = find_table(d, b"head").ok_or(FontError::MissingTable("head"))?;
        let maxp = find_table(d, b"maxp").ok_or(FontError::MissingTable("maxp"))?;
        let hhea = find_table(d, b"hhea").ok_or(FontError::MissingTable("hhea"))?;
        let hmtx = find_table(d, b"hmtx").ok_or(FontError::MissingTable("hmtx"))?;
        let cmap = find_table(d, b"cmap").ok_or(FontError::MissingTable("cmap"))?;

        let units_per_em = be_u16(d, head + 18).ok_or(FontError::Truncated)?;
        let num_glyphs = be_u16(d, maxp + 4).ok_or(FontError::Truncated)?;
        let ascent = be_i16(d, hhea + 4).ok_or(FontError::Truncated)?;
        let descent = be_i16(d, hhea + 6).ok_or(FontError::Truncated)?;
        let line_gap = be_i16(d, hhea + 8).ok_or(FontError::Truncated)?;
        let num_h_metrics = be_u16(d, hhea + 34).ok_or(FontError::Truncated)?;

        let (cmap_off, cmap_format) = select_cmap(d, cmap).ok_or(FontError::NoUnicodeCmap)?;

        // Outline tables are optional: present for TrueType (glyf) fonts, absent
        // for CFF/OpenType. Their absence is not an error here.
        let loca_long = be_i16(d, head + 50).unwrap_or(0) != 0;
        let loca_off = find_table(d, b"loca");
        let glyf = find_table_full(d, b"glyf");
        let kern0 = find_kern0(d);

        Ok(Self {
            data,
            units_per_em,
            num_glyphs,
            ascent,
            descent,
            line_gap,
            num_h_metrics,
            hmtx_off: hmtx,
            cmap_off,
            cmap_format,
            glyf,
            loca_off,
            loca_long,
            kern0,
        })
    }

    /// True when the font carries TrueType (`glyf`) outlines we can read/subset.
    #[must_use]
    pub fn has_glyf_outlines(&self) -> bool {
        self.glyf.is_some() && self.loca_off.is_some()
    }

    /// The `[start, end)` byte range of glyph `gid` within the `glyf` table.
    /// Returns `None` if the font has no `glyf`/`loca`, or `Some((s, s))` for an
    /// empty glyph (e.g. space).
    fn glyph_range(&self, gid: u16) -> Option<(usize, usize)> {
        let loca = self.loca_off?;
        let (glyf_off, glyf_len) = self.glyf?;
        let i = gid as usize;
        let (start, end) = if self.loca_long {
            (
                be_u32(&self.data, loca + i * 4)? as usize,
                be_u32(&self.data, loca + (i + 1) * 4)? as usize,
            )
        } else {
            // Short loca stores offsets / 2.
            (
                be_u16(&self.data, loca + i * 2)? as usize * 2,
                be_u16(&self.data, loca + (i + 1) * 2)? as usize * 2,
            )
        };
        if end < start || end > glyf_len {
            return None;
        }
        Some((glyf_off + start, glyf_off + end))
    }

    /// Raw `glyf` bytes for glyph `gid` (for subset embedding), or `None`.
    /// An empty (zero-length) glyph yields `Some(&[])`.
    #[must_use]
    pub fn glyph_data(&self, gid: u16) -> Option<&[u8]> {
        let (s, e) = self.glyph_range(gid)?;
        self.data.get(s..e)
    }

    /// Glyph bounding box `[xMin, yMin, xMax, yMax]` (design units), or `None`
    /// for an empty glyph / no outlines.
    #[must_use]
    pub fn glyph_bbox(&self, gid: u16) -> Option<[i16; 4]> {
        let (s, e) = self.glyph_range(gid)?;
        if e <= s {
            return None; // empty glyph (no contours)
        }
        Some([
            be_i16(&self.data, s + 2)?,
            be_i16(&self.data, s + 4)?,
            be_i16(&self.data, s + 6)?,
            be_i16(&self.data, s + 8)?,
        ])
    }

    /// True when glyph `gid` is a composite (built from component glyphs).
    #[must_use]
    pub fn is_composite(&self, gid: u16) -> bool {
        match self.glyph_range(gid) {
            Some((s, e)) if e > s => be_i16(&self.data, s).is_some_and(|n| n < 0),
            _ => false,
        }
    }

    /// Component glyph ids referenced by a composite glyph (for transitive
    /// subsetting). Empty for simple or empty glyphs.
    #[must_use]
    pub fn glyph_components(&self, gid: u16) -> Vec<u16> {
        const ARG_WORDS: u16 = 0x0001;
        const WE_HAVE_SCALE: u16 = 0x0008;
        const MORE: u16 = 0x0020;
        const X_Y_SCALE: u16 = 0x0040;
        const TWO_BY_TWO: u16 = 0x0080;

        let mut out = Vec::new();
        let Some((s, e)) = self.glyph_range(gid) else {
            return out;
        };
        if e <= s || !be_i16(&self.data, s).is_some_and(|n| n < 0) {
            return out;
        }
        let mut p = s + 10; // skip numberOfContours + bbox
        while let Some(flags) = be_u16(&self.data, p) {
            let Some(comp) = be_u16(&self.data, p + 2) else {
                break;
            };
            out.push(comp);
            p += 4;
            p += if flags & ARG_WORDS != 0 { 4 } else { 2 };
            if flags & WE_HAVE_SCALE != 0 {
                p += 2;
            } else if flags & X_Y_SCALE != 0 {
                p += 4;
            } else if flags & TWO_BY_TWO != 0 {
                p += 8;
            }
            if flags & MORE == 0 || p >= e {
                break;
            }
        }
        out
    }

    /// The advance width of glyph `gid` in design units. Glyphs past the
    /// `hmtx` metric run share the last advance (monospaced trailing run).
    #[must_use]
    pub fn advance_width(&self, gid: u16) -> u16 {
        let last = self.num_h_metrics.saturating_sub(1);
        let idx = gid.min(last) as usize;
        be_u16(&self.data, self.hmtx_off + idx * 4).unwrap_or(0)
    }

    /// The glyph id for a character, or `0` (`.notdef`) if unmapped.
    #[must_use]
    pub fn glyph_index(&self, ch: char) -> u16 {
        let cp = ch as u32;
        match self.cmap_format {
            4 => self.cmap4_lookup(cp).unwrap_or(0),
            12 => self.cmap12_lookup(cp).unwrap_or(0),
            _ => 0,
        }
    }

    /// Advance width of `ch` in 1/1000 em (PDF text-space units), `0` if unmapped.
    #[must_use]
    pub fn advance_1000(&self, ch: char) -> u32 {
        if self.units_per_em == 0 {
            return 0;
        }
        let aw = self.advance_width(self.glyph_index(ch)) as u32;
        aw * 1000 / self.units_per_em as u32
    }

    /// Kerning adjustment between two glyph ids in design units.
    ///
    /// Unsupported or absent kerning tables return zero. This currently supports
    /// legacy TrueType/Microsoft `kern` table version 0, format 0, horizontal
    /// pairs. GPOS pair positioning is tracked separately.
    #[must_use]
    pub fn kerning_between_glyphs(&self, left: u16, right: u16) -> i16 {
        let Some((pairs, n_pairs)) = self.kern0 else {
            return 0;
        };
        let target = ((left as u32) << 16) | right as u32;
        let mut lo = 0usize;
        let mut hi = n_pairs as usize;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let rec = pairs + mid * 6;
            let Some(l) = be_u16(&self.data, rec) else {
                return 0;
            };
            let Some(r) = be_u16(&self.data, rec + 2) else {
                return 0;
            };
            let key = ((l as u32) << 16) | r as u32;
            if key == target {
                return be_i16(&self.data, rec + 4).unwrap_or(0);
            }
            if key < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        0
    }

    /// Kerning adjustment between two characters in design units.
    #[must_use]
    pub fn kerning(&self, left: char, right: char) -> i16 {
        self.kerning_between_glyphs(self.glyph_index(left), self.glyph_index(right))
    }

    /// Kerning adjustment between two characters in 1/1000 em units.
    #[must_use]
    pub fn kerning_1000(&self, left: char, right: char) -> i32 {
        if self.units_per_em == 0 {
            return 0;
        }
        self.kerning(left, right) as i32 * 1000 / self.units_per_em as i32
    }

    fn cmap4_lookup(&self, cp: u32) -> Option<u16> {
        if cp > 0xFFFF {
            return Some(0);
        }
        let c = cp as u16;
        let d = &self.data;
        let base = self.cmap_off;
        let seg_x2 = be_u16(d, base + 6)? as usize;
        let seg_count = seg_x2 / 2;
        let end_codes = base + 14;
        let start_codes = end_codes + seg_x2 + 2; // +2 for reservedPad
        let id_deltas = start_codes + seg_x2;
        let id_range_offsets = id_deltas + seg_x2;
        for i in 0..seg_count {
            let end = be_u16(d, end_codes + i * 2)?;
            if c > end {
                continue;
            }
            let start = be_u16(d, start_codes + i * 2)?;
            if c < start {
                return Some(0);
            }
            let id_delta = be_u16(d, id_deltas + i * 2)?;
            let iro_pos = id_range_offsets + i * 2;
            let id_range_offset = be_u16(d, iro_pos)?;
            if id_range_offset == 0 {
                return Some(c.wrapping_add(id_delta));
            }
            let gi_addr = iro_pos + id_range_offset as usize + 2 * (c - start) as usize;
            let g = be_u16(d, gi_addr)?;
            return Some(if g == 0 { 0 } else { g.wrapping_add(id_delta) });
        }
        Some(0)
    }

    /// Build a new, minimal, valid TrueType (`glyf`) font containing glyph 0
    /// (`.notdef`) plus exactly the glyphs needed to render `keep` (mapped
    /// through the original `cmap`), transitively closing over composite
    /// components. Returns a fresh sfnt (`0x00010000`) suitable for a PDF
    /// `FontFile2`, or `None` on any failure (missing `glyf`/`loca`/required
    /// table, or a malformed read).
    #[must_use]
    pub fn subset(&self, keep: &[char]) -> Option<Vec<u8>> {
        // --- 1. Glyph closure ------------------------------------------------
        // Require TrueType outlines; CFF/`OTTO` fonts cannot be subset here.
        if !self.has_glyf_outlines() {
            return None;
        }
        let mut set: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
        set.insert(0);
        for &ch in keep {
            let gid = self.glyph_index(ch);
            if gid != 0 {
                set.insert(gid);
            }
        }
        // Transitively pull in composite components until the set is stable.
        loop {
            let mut added: Vec<u16> = Vec::new();
            for &gid in &set {
                if self.is_composite(gid) {
                    for c in self.glyph_components(gid) {
                        if !set.contains(&c) {
                            added.push(c);
                        }
                    }
                }
            }
            if added.is_empty() {
                break;
            }
            for c in added {
                set.insert(c);
            }
        }
        let old_gids: Vec<u16> = set.into_iter().collect(); // ascending, 0 first

        // --- 2. Renumber old -> new -----------------------------------------
        let mut new_of: std::collections::BTreeMap<u16, u16> = std::collections::BTreeMap::new();
        for (i, &g) in old_gids.iter().enumerate() {
            new_of.insert(g, i as u16);
        }
        let n = old_gids.len();
        let n_u16 = n as u16;

        // --- 3. Rebuild glyf + loca (long offsets) --------------------------
        let mut glyf_bytes: Vec<u8> = Vec::new();
        let mut loca: Vec<u32> = Vec::with_capacity(n + 1);
        for &old in &old_gids {
            loca.push(glyf_bytes.len() as u32);
            let gb = self.subset_glyph_bytes(old, &new_of)?;
            glyf_bytes.extend_from_slice(&gb);
            // Pad each glyph to a 4-byte multiple so the next glyph (and every
            // long-loca offset) is word-aligned.
            while glyf_bytes.len() % 4 != 0 {
                glyf_bytes.push(0);
            }
        }
        loca.push(glyf_bytes.len() as u32);
        let mut loca_bytes: Vec<u8> = Vec::with_capacity(loca.len() * 4);
        for o in &loca {
            loca_bytes.extend_from_slice(&o.to_be_bytes());
        }

        // --- 4. Metric/meta tables ------------------------------------------
        // maxp: original bytes with numGlyphs (u16 @ +4) set to n.
        let (maxp_off, maxp_len) = find_table_full(&self.data, b"maxp")?;
        let mut maxp = self.data.get(maxp_off..maxp_off + maxp_len)?.to_vec();
        write_u16(&mut maxp, 4, n_u16)?;

        // hhea: original bytes with numberOfHMetrics (u16 @ +34) set to n.
        let (hhea_off, hhea_len) = find_table_full(&self.data, b"hhea")?;
        let mut hhea = self.data.get(hhea_off..hhea_off + hhea_len)?.to_vec();
        write_u16(&mut hhea, 34, n_u16)?;

        // hmtx: n long metrics (advanceWidth, lsb=0), no trailing run.
        let mut hmtx: Vec<u8> = Vec::with_capacity(n * 4);
        for &old in &old_gids {
            hmtx.extend_from_slice(&self.advance_width(old).to_be_bytes());
            hmtx.extend_from_slice(&0i16.to_be_bytes());
        }

        // head: original bytes; zero checkSumAdjustment (@ +8), force long loca.
        let (head_off, head_len) = find_table_full(&self.data, b"head")?;
        let mut head = self.data.get(head_off..head_off + head_len)?.to_vec();
        write_u32(&mut head, 8, 0)?;
        write_u16(&mut head, 50, 1)?; // indexToLocFormat = 1 (long)

        // cmap: fresh single format-4 (3,1) subtable.
        let cmap = self.build_cmap4(keep, &new_of)?;

        // name: minimal valid table (format 0, count 0, stringOffset 6).
        let mut name: Vec<u8> = Vec::with_capacity(6);
        name.extend_from_slice(&0u16.to_be_bytes());
        name.extend_from_slice(&0u16.to_be_bytes());
        name.extend_from_slice(&6u16.to_be_bytes());

        // post: format 3.0, 32 bytes, all metric fields zero.
        let mut post: Vec<u8> = Vec::with_capacity(32);
        post.extend_from_slice(&0x0003_0000u32.to_be_bytes()); // version 3.0
        post.extend_from_slice(&0u32.to_be_bytes()); // italicAngle
        post.extend_from_slice(&0u16.to_be_bytes()); // underlinePosition
        post.extend_from_slice(&0u16.to_be_bytes()); // underlineThickness
        post.extend_from_slice(&0u32.to_be_bytes()); // isFixedPitch
        post.extend_from_slice(&0u32.to_be_bytes()); // minMemType42
        post.extend_from_slice(&0u32.to_be_bytes()); // maxMemType42
        post.extend_from_slice(&0u32.to_be_bytes()); // minMemType1
        post.extend_from_slice(&0u32.to_be_bytes()); // maxMemType1

        // --- 5. Assemble the sfnt -------------------------------------------
        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"head", head),
            (b"hhea", hhea),
            (b"maxp", maxp),
            (b"hmtx", hmtx),
            (b"loca", loca_bytes),
            (b"glyf", glyf_bytes),
            (b"cmap", cmap),
            (b"name", name),
            (b"post", post),
        ];
        tables.sort_by(|a, b| a.0.cmp(b.0)); // ascending by tag

        let num_tables = tables.len();
        // searchRange = (2^floor(log2(n)))*16, entrySelector = floor(log2(n)).
        let mut pw: usize = 1;
        let mut es: u16 = 0;
        while pw * 2 <= num_tables {
            pw *= 2;
            es += 1;
        }
        let search_range = (pw as u16).wrapping_mul(16);
        let entry_selector = es;
        let range_shift = (num_tables as u16)
            .wrapping_mul(16)
            .wrapping_sub(search_range);

        let dir_size = 12 + num_tables * 16;
        let mut body: Vec<u8> = Vec::new();
        // (tag, checksum, offset, length)
        let mut records: Vec<([u8; 4], u32, u32, u32)> = Vec::with_capacity(num_tables);
        let mut head_offset: usize = 0;
        for (tag, bytes) in &tables {
            // Align each table's file start to a 4-byte boundary.
            while (dir_size + body.len()) % 4 != 0 {
                body.push(0);
            }
            let table_offset = dir_size + body.len();
            if *tag == b"head" {
                head_offset = table_offset;
            }
            let checksum = table_checksum(bytes);
            records.push((**tag, checksum, table_offset as u32, bytes.len() as u32));
            body.extend_from_slice(bytes);
        }
        while body.len() % 4 != 0 {
            body.push(0);
        }

        let mut out: Vec<u8> = Vec::with_capacity(dir_size + body.len());
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // sfntVersion
        out.extend_from_slice(&(num_tables as u16).to_be_bytes());
        out.extend_from_slice(&search_range.to_be_bytes());
        out.extend_from_slice(&entry_selector.to_be_bytes());
        out.extend_from_slice(&range_shift.to_be_bytes());
        for (tag, checksum, toff, tlen) in &records {
            out.extend_from_slice(tag);
            out.extend_from_slice(&checksum.to_be_bytes());
            out.extend_from_slice(&toff.to_be_bytes());
            out.extend_from_slice(&tlen.to_be_bytes());
        }
        out.extend_from_slice(&body);

        // checkSumAdjustment: 0xB1B0AFBA - checksum(whole file with field zeroed).
        let file_checksum = table_checksum(&out);
        let adj = 0xB1B0_AFBAu32.wrapping_sub(file_checksum);
        write_u32(&mut out, head_offset + 8, adj)?;

        Some(out)
    }

    /// Glyph bytes for the subset: simple glyphs are copied verbatim; composite
    /// glyphs are copied with each component `glyphIndex` (u16) rewritten from
    /// its old gid to its new gid. Empty glyphs yield an empty `Vec`.
    fn subset_glyph_bytes(
        &self,
        old: u16,
        new_of: &std::collections::BTreeMap<u16, u16>,
    ) -> Option<Vec<u8>> {
        const ARG_WORDS: u16 = 0x0001;
        const WE_HAVE_SCALE: u16 = 0x0008;
        const MORE: u16 = 0x0020;
        const X_Y_SCALE: u16 = 0x0040;
        const TWO_BY_TWO: u16 = 0x0080;

        let data = self.glyph_data(old).unwrap_or(&[]);
        if data.is_empty() {
            return Some(Vec::new());
        }
        let num_contours = be_i16(data, 0)?;
        let mut out = data.to_vec();
        if num_contours >= 0 {
            return Some(out); // simple glyph: byte-identical copy
        }
        // Composite: walk component records, rewriting each glyphIndex.
        let mut p = 10usize; // skip numberOfContours + 4x i16 bbox
        loop {
            let flags = be_u16(&out, p)?;
            let comp_old = be_u16(&out, p + 2)?;
            let comp_new = *new_of.get(&comp_old)?;
            let nb = comp_new.to_be_bytes();
            *out.get_mut(p + 2)? = nb[0];
            *out.get_mut(p + 3)? = nb[1];
            p += 4;
            p += if flags & ARG_WORDS != 0 { 4 } else { 2 };
            if flags & WE_HAVE_SCALE != 0 {
                p += 2;
            } else if flags & X_Y_SCALE != 0 {
                p += 4;
            } else if flags & TWO_BY_TWO != 0 {
                p += 8;
            }
            if flags & MORE == 0 {
                break;
            }
        }
        Some(out)
    }

    /// Build a complete `cmap` table holding a single format-4 `(3,1)` subtable
    /// mapping every BMP char in `keep` to its NEW gid (one 1-char segment each,
    /// plus the mandatory final `0xFFFF` segment).
    fn build_cmap4(
        &self,
        keep: &[char],
        new_of: &std::collections::BTreeMap<u16, u16>,
    ) -> Option<Vec<u8>> {
        // Unique, ascending code -> new gid (0xFFFF reserved for the final seg).
        let mut codes: std::collections::BTreeMap<u16, u16> = std::collections::BTreeMap::new();
        for &ch in keep {
            let cp = ch as u32;
            if cp >= 0xFFFF {
                continue;
            }
            let old = self.glyph_index(ch);
            let ng = *new_of.get(&old)?;
            codes.insert(cp as u16, ng);
        }
        let entries: Vec<(u16, u16)> = codes.into_iter().collect();
        let seg_count = entries.len() + 1; // + final 0xFFFF segment

        let mut pw: usize = 1;
        let mut es: u16 = 0;
        while pw * 2 <= seg_count {
            pw *= 2;
            es += 1;
        }
        let search_range = (pw as u16).wrapping_mul(2);
        let entry_selector = es;
        let range_shift = (seg_count as u16)
            .wrapping_mul(2)
            .wrapping_sub(search_range);

        let sub_len = 16 + seg_count * 8;
        let mut sub: Vec<u8> = Vec::with_capacity(sub_len);
        sub.extend_from_slice(&4u16.to_be_bytes()); // format
        sub.extend_from_slice(&(sub_len as u16).to_be_bytes()); // length
        sub.extend_from_slice(&0u16.to_be_bytes()); // language
        sub.extend_from_slice(&((seg_count * 2) as u16).to_be_bytes()); // segCountX2
        sub.extend_from_slice(&search_range.to_be_bytes());
        sub.extend_from_slice(&entry_selector.to_be_bytes());
        sub.extend_from_slice(&range_shift.to_be_bytes());
        // endCode[]
        for &(code, _) in &entries {
            sub.extend_from_slice(&code.to_be_bytes());
        }
        sub.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // reservedPad
        sub.extend_from_slice(&0u16.to_be_bytes());
        // startCode[]
        for &(code, _) in &entries {
            sub.extend_from_slice(&code.to_be_bytes());
        }
        sub.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // idDelta[]: (code + idDelta) & 0xFFFF == new gid.
        for &(code, ng) in &entries {
            sub.extend_from_slice(&ng.wrapping_sub(code).to_be_bytes());
        }
        // Final segment idDelta = 1.
        sub.extend_from_slice(&1u16.to_be_bytes());
        // idRangeOffset[] (all zero, glyphIdArray empty).
        for _ in &entries {
            sub.extend_from_slice(&0u16.to_be_bytes());
        }
        sub.extend_from_slice(&0u16.to_be_bytes());

        let mut cmap: Vec<u8> = Vec::with_capacity(12 + sub.len());
        cmap.extend_from_slice(&0u16.to_be_bytes()); // version
        cmap.extend_from_slice(&1u16.to_be_bytes()); // numTables
        cmap.extend_from_slice(&3u16.to_be_bytes()); // platformID (Windows)
        cmap.extend_from_slice(&1u16.to_be_bytes()); // encodingID (Unicode BMP)
        cmap.extend_from_slice(&12u32.to_be_bytes()); // subtable offset
        cmap.extend_from_slice(&sub);
        Some(cmap)
    }

    fn cmap12_lookup(&self, cp: u32) -> Option<u16> {
        let d = &self.data;
        let base = self.cmap_off;
        let num_groups = be_u32(d, base + 12)? as usize;
        for i in 0..num_groups {
            let g = base + 16 + i * 12;
            let start = be_u32(d, g)?;
            let end = be_u32(d, g + 4)?;
            if cp >= start && cp <= end {
                let start_gid = be_u32(d, g + 8)?;
                let gid = start_gid + (cp - start);
                return Some((gid & 0xFFFF) as u16);
            }
        }
        Some(0)
    }
}

/// Choose the best Unicode `cmap` subtable, returning its absolute offset and
/// format. Prefers a full-repertoire format-12 `(3,10)`/`(0,*)` table, then a
/// BMP format-4 `(3,1)`/`(0,*)` table.
fn select_cmap(d: &[u8], cmap: usize) -> Option<(usize, u16)> {
    let num = be_u16(d, cmap + 2)? as usize;
    let mut best: Option<(usize, u16, u8)> = None; // (offset, format, rank)
    for i in 0..num {
        let rec = cmap + 4 + i * 8;
        let platform = be_u16(d, rec)?;
        let encoding = be_u16(d, rec + 2)?;
        let sub = cmap + be_u32(d, rec + 4)? as usize;
        let format = be_u16(d, sub)?;
        let unicode = matches!((platform, encoding), (0, _) | (3, 1) | (3, 10));
        if !unicode {
            continue;
        }
        let rank = match format {
            12 => 3,
            4 => {
                if (platform, encoding) == (3, 1) || platform == 0 {
                    2
                } else {
                    1
                }
            }
            _ => continue,
        };
        if best.is_none_or(|(_, _, r)| rank > r) {
            best = Some((sub, format, rank));
        }
    }
    best.map(|(off, fmt, _)| (off, fmt))
}
