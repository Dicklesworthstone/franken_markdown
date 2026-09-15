//! Deterministic shaping of caller-segmented runs. No bidi segmentation,
//! normalization, font fallback, or host discovery occurs here. Coverage is
//! Latin and basic Arabic (U+0620..U+064A plus declared combining marks).
//! GSUB single/ligature and GPOS single/pair/mark-to-base/mark-to-mark are supported;
//! selected unsupported lookups are refused rather than silently skipped.
use crate::{Font, be_i16, be_u16, be_u32, find_table_full};
use std::{collections::BTreeMap, ops::Range};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    pub tag: [u8; 4],
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeOptions<'a> {
    pub script: [u8; 4],
    /// OpenType language tag, or `dflt` for the script's default language.
    pub language: [u8; 4],
    pub direction: Direction,
    pub features: &'a [Feature],
}
impl Default for ShapeOptions<'_> {
    fn default() -> Self {
        Self {
            script: *b"latn",
            language: *b"dflt",
            direction: Direction::LeftToRight,
            features: &[],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    /// UTF-8 byte range in the original logical input. A ligature spans every
    /// contributing scalar; marks share their base's cluster. Visual order is
    /// increasing for LTR and decreasing for RTL; ranges may repeat.
    pub cluster: Range<usize>,
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub direction: Direction,
    source: String,
}
impl ShapedRun {
    /// Original logical text, retained exactly (including ligatures and marks).
    pub fn logical_text(&self) -> &str {
        &self.source
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeErrorKind {
    UnsupportedScript,
    UnsupportedLanguage,
    UnsupportedFeature,
    UnsupportedLookup,
    MissingGlyph,
    UnpositionedMark,
    MalformedFont,
    BudgetExceeded,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeError {
    pub kind: ShapeErrorKind,
    pub table: Option<[u8; 4]>,
    pub offset: Option<usize>,
    pub text_offset: Option<usize>,
}
impl core::fmt::Display for ShapeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "shaping {:?}, table {:?}, offset {:?}, text byte {:?}",
            self.kind, self.table, self.offset, self.text_offset
        )
    }
}
impl std::error::Error for ShapeError {}
fn error(kind: ShapeErrorKind) -> ShapeError {
    ShapeError {
        kind,
        table: None,
        offset: None,
        text_offset: None,
    }
}
#[derive(Clone, Copy)]
struct Table<'a> {
    d: &'a [u8],
    tag: [u8; 4],
    fuel: &'a std::cell::Cell<usize>,
}
impl Table<'_> {
    fn bad(&self, p: usize) -> ShapeError {
        ShapeError {
            kind: ShapeErrorKind::MalformedFont,
            table: Some(self.tag),
            offset: Some(p),
            text_offset: None,
        }
    }
    fn u16(&self, p: usize) -> Result<u16, ShapeError> {
        self.fuel.set(
            self.fuel
                .get()
                .checked_sub(1)
                .ok_or(error(ShapeErrorKind::BudgetExceeded))?,
        );
        be_u16(self.d, p).ok_or(self.bad(p))
    }
    fn i16(&self, p: usize) -> Result<i16, ShapeError> {
        be_i16(self.d, p).ok_or(self.bad(p))
    }
    fn offset(&self, base: usize, p: usize) -> Result<usize, ShapeError> {
        let n = self.u16(p)?;
        if n == 0 {
            return Err(self.bad(p));
        }
        let o = base + usize::from(n);
        if o >= self.d.len() {
            return Err(self.bad(p));
        }
        Ok(o)
    }
    fn tag(&self, p: usize) -> Result<[u8; 4], ShapeError> {
        self.d
            .get(p..p + 4)
            .and_then(|v| v.try_into().ok())
            .ok_or(self.bad(p))
    }
    fn coverage(&self, p: usize, g: u16) -> Result<Option<usize>, ShapeError> {
        let count = usize::from(self.u16(p + 2)?);
        match self.u16(p)? {
            1 => {
                let mut last = None;
                let mut result = None;
                for i in 0..count {
                    let id = self.u16(p + 4 + i * 2)?;
                    if last.is_some_and(|v| v >= id) {
                        return Err(self.bad(p));
                    }
                    if id == g {
                        result = Some(i);
                    }
                    last = Some(id);
                }
                Ok(result)
            }
            2 => {
                let mut previous = None;
                let mut expected = 0usize;
                let mut result = None;
                for i in 0..count {
                    let r = p + 4 + i * 6;
                    let a = self.u16(r)?;
                    let b = self.u16(r + 2)?;
                    let start = usize::from(self.u16(r + 4)?);
                    if a > b || previous.is_some_and(|v| v >= a) || start != expected {
                        return Err(self.bad(r));
                    }
                    if (a..=b).contains(&g) {
                        result = Some(start + usize::from(g - a));
                    }
                    expected += usize::from(b - a) + 1;
                    if expected > 65536 {
                        return Err(self.bad(r));
                    }
                    previous = Some(b);
                }
                Ok(result)
            }
            _ => Err(self.bad(p)),
        }
    }
    fn class(&self, p: usize, gid: u16) -> Result<usize, ShapeError> {
        match self.u16(p)? {
            1 => {
                let first = self.u16(p + 2)?;
                let count = usize::from(self.u16(p + 4)?);
                if gid < first || usize::from(gid - first) >= count {
                    return Ok(0);
                }
                Ok(usize::from(self.u16(p + 6 + usize::from(gid - first) * 2)?))
            }
            2 => {
                let mut previous = None;
                let mut result = 0;
                for i in 0..usize::from(self.u16(p + 2)?) {
                    let r = p + 4 + i * 6;
                    let a = self.u16(r)?;
                    let b = self.u16(r + 2)?;
                    if a > b || previous.is_some_and(|v| v >= a) {
                        return Err(self.bad(r));
                    }
                    if (a..=b).contains(&gid) {
                        result = usize::from(self.u16(r + 4)?);
                    }
                    previous = Some(b);
                }
                Ok(result)
            }
            _ => Err(self.bad(p)),
        }
    }
    fn anchor(&self, p: usize) -> Result<(i32, i32), ShapeError> {
        if self.u16(p)? != 1 {
            return Err(ShapeError {
                kind: ShapeErrorKind::UnsupportedLookup,
                ..self.bad(p)
            });
        }
        Ok((i32::from(self.i16(p + 2)?), i32::from(self.i16(p + 4)?)))
    }
}
#[derive(Clone)]
struct Item {
    g: ShapedGlyph,
    mark: bool,
    positioned: bool,
    form: [u8; 4],
}
fn is_mark(c: char) -> bool {
    matches!(c as u32,0x0300..=0x036f|0x064b..=0x065f|0x0670)
}
fn joining(c: char) -> u8 {
    // Unicode 17 ArabicShaping.txt: basic Arabic right/dual joining repertoire.
    match c as u32 {
        0x0620
        | 0x0626
        | 0x0628
        | 0x062a..=0x062e
        | 0x0633..=0x063f
        | 0x0640..=0x0647
        | 0x0649..=0x064a => 2,
        0x0622..=0x0625 | 0x0627 | 0x0629 | 0x062f..=0x0632 | 0x0648 => 1,
        _ => 0,
    }
}
fn features(
    t: Table<'_>,
    opts: &ShapeOptions<'_>,
) -> Result<BTreeMap<[u8; 4], Vec<u16>>, ShapeError> {
    if t.u16(0)? != 1 || t.u16(2)? != 0 {
        return Err(ShapeError {
            kind: ShapeErrorKind::UnsupportedLookup,
            ..t.bad(0)
        });
    }
    let scripts = t.offset(0, 4)?;
    let list = t.offset(0, 6)?;
    let mut script = None;
    for i in 0..usize::from(t.u16(scripts)?) {
        let r = scripts + 2 + i * 6;
        if t.tag(r)? == opts.script {
            script = Some(t.offset(scripts, r + 4)?);
        }
    }
    let Some(script) = script else {
        return Ok(BTreeMap::new());
    };
    let mut lang = if opts.language == *b"dflt" && t.u16(script)? != 0 {
        Some(t.offset(script, script)?)
    } else {
        None
    };
    for i in 0..usize::from(t.u16(script + 2)?) {
        let r = script + 4 + i * 6;
        if t.tag(r)? == opts.language {
            lang = Some(t.offset(script, r + 4)?);
        }
    }
    let lang = lang.ok_or(error(ShapeErrorKind::UnsupportedLanguage))?;
    if t.u16(lang)? != 0 {
        return Err(t.bad(lang));
    }
    let mut ids = Vec::new();
    let required = t.u16(lang + 2)?;
    if required != u16::MAX {
        ids.push(required);
    }
    for i in 0..usize::from(t.u16(lang + 4)?) {
        ids.push(t.u16(lang + 6 + i * 2)?);
    }
    let n = t.u16(list)?;
    let mut out: BTreeMap<[u8; 4], Vec<u16>> = BTreeMap::new();
    for id in ids {
        if id >= n {
            return Err(t.bad(list));
        }
        let r = list + 2 + usize::from(id) * 6;
        let tag = t.tag(r)?;
        if id == required
            && (![
                *b"ccmp", *b"locl", *b"rlig", *b"liga", *b"isol", *b"init", *b"medi", *b"fina",
                *b"kern", *b"mark", *b"mkmk",
            ]
            .contains(&tag)
                || !enabled(opts, tag))
        {
            return Err(ShapeError {
                kind: ShapeErrorKind::UnsupportedFeature,
                ..t.bad(r)
            });
        }
        let f = t.offset(list, r + 4)?;
        if t.u16(f)? != 0 {
            return Err(ShapeError {
                kind: ShapeErrorKind::UnsupportedFeature,
                ..t.bad(f)
            });
        }
        let dest = out.entry(tag).or_default();
        for i in 0..usize::from(t.u16(f + 2)?) {
            dest.push(t.u16(f + 4 + i * 2)?);
        }
    }
    Ok(out)
}
fn table<'a>(
    font: &'a Font,
    tag: [u8; 4],
    fuel: &'a std::cell::Cell<usize>,
) -> Result<Option<Table<'a>>, ShapeError> {
    let Some((o, n)) = find_table_full(&font.data, &tag) else {
        return Ok(None);
    };
    let d = font
        .data
        .get(
            o..o.checked_add(n)
                .ok_or(error(ShapeErrorKind::MalformedFont))?,
        )
        .ok_or(error(ShapeErrorKind::MalformedFont))?;
    Ok(Some(Table { d, tag, fuel }))
}
fn enabled(opts: &ShapeOptions<'_>, tag: [u8; 4]) -> bool {
    opts.features
        .iter()
        .find(|f| f.tag == tag)
        .is_none_or(|f| f.enabled)
}
fn spend(fuel: &mut usize) -> Result<(), ShapeError> {
    *fuel = fuel
        .checked_sub(1)
        .ok_or(error(ShapeErrorKind::BudgetExceeded))?;
    Ok(())
}
fn lookup(t: Table<'_>, id: u16) -> Result<(u16, u16, Vec<usize>), ShapeError> {
    let list = t.offset(0, 8)?;
    if id >= t.u16(list)? {
        return Err(t.bad(list));
    }
    let p = t.offset(list, list + 2 + usize::from(id) * 2)?;
    let kind = t.u16(p)?;
    let flags = t.u16(p + 2)?;
    if flags & !8 != 0 {
        return Err(ShapeError {
            kind: ShapeErrorKind::UnsupportedLookup,
            ..t.bad(p + 2)
        });
    }
    let mut subtables = Vec::new();
    for i in 0..usize::from(t.u16(p + 4)?) {
        subtables.push(t.offset(p, p + 6 + i * 2)?);
    }
    Ok((kind, flags, subtables))
}
fn extension(t: Table<'_>, kind: u16, p: usize) -> Result<(u16, usize), ShapeError> {
    let wrapper = if t.tag == *b"GSUB" { 7 } else { 9 };
    if kind != wrapper {
        return Ok((kind, p));
    }
    if t.u16(p)? != 1 {
        return Err(t.bad(p));
    }
    let actual = t.u16(p + 2)?;
    if actual == wrapper {
        return Err(t.bad(p));
    }
    let offset = be_u32(t.d, p + 4).ok_or(t.bad(p + 4))? as usize;
    let next = p
        .checked_add(offset)
        .filter(|&x| x < t.d.len())
        .ok_or(t.bad(p + 4))?;
    Ok((actual, next))
}
fn substitute(
    t: Table<'_>,
    ids: &[u16],
    items: &mut Vec<Item>,
    form: Option<[u8; 4]>,
    fuel: &mut usize,
) -> Result<(), ShapeError> {
    for &id in ids {
        let (kind, flags, subs) = lookup(t, id)?;
        for item_index in 0..items.len() {
            // Index may become out of range after ligature contraction.
            if item_index >= items.len() {
                break;
            }
            spend(fuel)?;
            if form.is_some_and(|f| items[item_index].form != f)
                || flags & 8 != 0 && items[item_index].mark
            {
                continue;
            }
            for &sub in &subs {
                let (kind, p) = extension(t, kind, sub)?;
                if !matches!(kind, 1 | 4) {
                    return Err(ShapeError {
                        kind: ShapeErrorKind::UnsupportedLookup,
                        ..t.bad(p)
                    });
                }
                let coverage = t.offset(p, p + 2)?;
                let Some(ci) = t.coverage(coverage, items[item_index].g.glyph_id)? else {
                    continue;
                };
                if kind == 1 {
                    let gid = match t.u16(p)? {
                        1 => items[item_index].g.glyph_id.wrapping_add(t.u16(p + 4)?),
                        2 => {
                            if ci >= usize::from(t.u16(p + 4)?) {
                                return Err(t.bad(p));
                            }
                            t.u16(p + 6 + ci * 2)?
                        }
                        _ => return Err(t.bad(p)),
                    };
                    items[item_index].g.glyph_id = gid;
                    break;
                }
                if t.u16(p)? != 1 || ci >= usize::from(t.u16(p + 4)?) {
                    return Err(t.bad(p));
                }
                let set = t.offset(p, p + 6 + ci * 2)?;
                let mut matched = false;
                for n in 0..usize::from(t.u16(set)?) {
                    spend(fuel)?;
                    let lig = t.offset(set, set + 2 + n * 2)?;
                    let count = usize::from(t.u16(lig + 2)?);
                    if count < 2 {
                        return Err(t.bad(lig));
                    }
                    let mut indices = vec![item_index];
                    let mut next = item_index + 1;
                    for j in 1..count {
                        while next < items.len() && flags & 8 != 0 && items[next].mark {
                            next += 1;
                        }
                        if next >= items.len()
                            || items[next].g.glyph_id != t.u16(lig + 2 + j * 2)?
                        {
                            break;
                        }
                        indices.push(next);
                        next += 1;
                    }
                    if indices.len() == count {
                        let end = items[*indices.last().ok_or(t.bad(lig))?].g.cluster.end;
                        items[item_index].g.cluster.end = end;
                        items[item_index].g.glyph_id = t.u16(lig)?;
                        // Skipped marks retain the merged source cluster; positioning remains mandatory.
                        let cluster = items[item_index].g.cluster.clone();
                        for item in &mut items[item_index..next] {
                            item.g.cluster = cluster.clone();
                        }
                        for &i in indices[1..].iter().rev() {
                            items.remove(i);
                        }
                        matched = true;
                        break;
                    }
                }
                if matched {
                    break;
                }
            }
        }
    }
    Ok(())
}
fn value(t: Table<'_>, p: usize, format: u16) -> Result<([i32; 4], usize), ShapeError> {
    if format & !15 != 0 {
        return Err(ShapeError {
            kind: ShapeErrorKind::UnsupportedLookup,
            ..t.bad(p)
        });
    }
    let mut values = [0; 4];
    let mut q = p;
    for (i, v) in values.iter_mut().enumerate() {
        if format & (1 << i) != 0 {
            *v = i32::from(t.i16(q)?);
            q += 2;
        }
    }
    Ok((values, q))
}
fn apply(g: &mut ShapedGlyph, v: [i32; 4]) -> Result<(), ShapeError> {
    for (dst, delta) in [
        (&mut g.x_offset, v[0]),
        (&mut g.y_offset, v[1]),
        (&mut g.x_advance, v[2]),
        (&mut g.y_advance, v[3]),
    ] {
        *dst = dst
            .checked_add(delta)
            .ok_or(error(ShapeErrorKind::BudgetExceeded))?;
    }
    Ok(())
}

fn position(
    t: Table<'_>,
    ids: &[u16],
    items: &mut [Item],
    direction: Direction,
    fuel: &mut usize,
) -> Result<(), ShapeError> {
    for &id in ids {
        let (kind, flags, subs) = lookup(t, id)?;
        for step in 0..items.len() {
            let i = if direction == Direction::RightToLeft {
                items.len() - 1 - step
            } else {
                step
            };
            spend(fuel)?;
            if flags & 8 != 0 && items[i].mark {
                continue;
            }
            for &sub in &subs {
                let (kind, p) = extension(t, kind, sub)?;
                if !matches!(kind, 1 | 2 | 4 | 6) {
                    return Err(ShapeError {
                        kind: ShapeErrorKind::UnsupportedLookup,
                        ..t.bad(p)
                    });
                }
                let Some(ci) = t.coverage(t.offset(p, p + 2)?, items[i].g.glyph_id)? else {
                    continue;
                };
                if kind == 1 {
                    let format = t.u16(p + 4)?;
                    let q = match t.u16(p)? {
                        1 => p + 6,
                        2 => {
                            if ci >= usize::from(t.u16(p + 6)?) {
                                return Err(t.bad(p));
                            }
                            p + 8 + ci * format.count_ones() as usize * 2
                        }
                        _ => return Err(t.bad(p)),
                    };
                    let (v, _) = value(t, q, format)?;
                    apply(&mut items[i].g, v)?;
                    break;
                }
                if kind == 2 {
                    let pair_format = t.u16(p)?;
                    if !matches!(pair_format, 1 | 2) {
                        return Err(t.bad(p));
                    }
                    let candidate = match direction {
                        Direction::LeftToRight => {
                            (i + 1..items.len()).find(|&j| flags & 8 == 0 || !items[j].mark)
                        }
                        Direction::RightToLeft => {
                            (0..i).rev().find(|&j| flags & 8 == 0 || !items[j].mark)
                        }
                    };
                    let Some(j) = candidate else {
                        continue;
                    };
                    let f1 = t.u16(p + 4)?;
                    let f2 = t.u16(p + 6)?;
                    if pair_format == 2 {
                        let a = t.class(t.offset(p, p + 8)?, items[i].g.glyph_id)?;
                        let b = t.class(t.offset(p, p + 10)?, items[j].g.glyph_id)?;
                        let n1 = usize::from(t.u16(p + 12)?);
                        let n2 = usize::from(t.u16(p + 14)?);
                        if n1.checked_mul(n2).is_none_or(|n| n > 1_000_000) {
                            return Err(error(ShapeErrorKind::BudgetExceeded));
                        }
                        if a >= n1 || b >= n2 {
                            return Err(t.bad(p));
                        }
                        let size = (f1.count_ones() + f2.count_ones()) as usize * 2;
                        let q = p + 16 + (a * n2 + b) * size;
                        let (v, next) = value(t, q, f1)?;
                        let (w, _) = value(t, next, f2)?;
                        apply(&mut items[i].g, v)?;
                        apply(&mut items[j].g, w)?;
                        break;
                    }
                    if ci >= usize::from(t.u16(p + 8)?) {
                        return Err(t.bad(p));
                    }
                    let set = t.offset(p, p + 10 + ci * 2)?;
                    let mut q = set + 2;
                    let mut found = false;
                    for _ in 0..t.u16(set)? {
                        spend(fuel)?;
                        let gid = t.u16(q)?;
                        let (a, next) = value(t, q + 2, f1)?;
                        let (b, next) = value(t, next, f2)?;
                        q = next;
                        if gid == items[j].g.glyph_id {
                            apply(&mut items[i].g, a)?;
                            apply(&mut items[j].g, b)?;
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                    continue;
                }
                if t.u16(p)? != 1 {
                    return Err(t.bad(p));
                }
                if !items[i].mark {
                    return Err(t.bad(p));
                }
                let candidate = match direction {
                    Direction::LeftToRight => (0..i).rev().find(|&j| items[j].mark == (kind == 6)),
                    Direction::RightToLeft => {
                        (i + 1..items.len()).find(|&j| items[j].mark == (kind == 6))
                    }
                };
                let Some(j) = candidate else {
                    continue;
                };
                if kind == 6 && items[j.min(i) + 1..j.max(i)].iter().any(|v| !v.mark) {
                    continue;
                }
                let Some(bi) = t.coverage(t.offset(p, p + 4)?, items[j].g.glyph_id)? else {
                    continue;
                };
                let classes = usize::from(t.u16(p + 6)?);
                if classes > 256 {
                    return Err(error(ShapeErrorKind::BudgetExceeded));
                }
                let marks = t.offset(p, p + 8)?;
                let bases = t.offset(p, p + 10)?;
                if ci >= usize::from(t.u16(marks)?) || bi >= usize::from(t.u16(bases)?) {
                    return Err(t.bad(p));
                }
                let class = usize::from(t.u16(marks + 2 + ci * 4)?);
                if class >= classes {
                    return Err(t.bad(marks));
                }
                let ma = t.anchor(t.offset(marks, marks + 4 + ci * 4)?)?;
                let anchor_slot = bases + 2 + (bi * classes + class) * 2;
                if t.u16(anchor_slot)? == 0 {
                    continue;
                }
                let ba = t.anchor(t.offset(bases, anchor_slot)?)?;
                let delta = if j < i {
                    -items[j..i]
                        .iter()
                        .map(|v| i64::from(v.g.x_advance))
                        .sum::<i64>()
                } else {
                    items[i..j]
                        .iter()
                        .map(|v| i64::from(v.g.x_advance))
                        .sum::<i64>()
                };
                items[i].g.x_offset = i32::try_from(
                    delta + i64::from(items[j].g.x_offset) + i64::from(ba.0) - i64::from(ma.0),
                )
                .map_err(|_| error(ShapeErrorKind::BudgetExceeded))?;
                items[i].g.y_offset = i32::try_from(
                    i64::from(items[j].g.y_offset) + i64::from(ba.1) - i64::from(ma.1),
                )
                .map_err(|_| error(ShapeErrorKind::BudgetExceeded))?;
                items[i].positioned = true;
                break;
            }
        }
    }
    Ok(())
}
impl Font {
    /// Shape a single explicitly segmented run. See module docs for exact coverage.
    pub fn shape(&self, text: &str, opts: &ShapeOptions<'_>) -> Result<ShapedRun, ShapeError> {
        self.validate_font_metrics().map_err(|e| ShapeError {
            kind: ShapeErrorKind::MalformedFont,
            table: Some(e.table),
            offset: e.offset,
            text_offset: None,
        })?;
        let arabic = opts.script == *b"arab";
        if opts.script != *b"latn" && !arabic {
            return Err(error(ShapeErrorKind::UnsupportedScript));
        }
        if (arabic && opts.direction != Direction::RightToLeft)
            || (!arabic && opts.direction != Direction::LeftToRight)
        {
            return Err(error(ShapeErrorKind::UnsupportedScript));
        }
        if text.len() > 65536 {
            return Err(error(ShapeErrorKind::BudgetExceeded));
        }
        let allowed = [
            *b"ccmp", *b"locl", *b"rlig", *b"liga", *b"kern", *b"mark", *b"mkmk", *b"isol",
            *b"init", *b"medi", *b"fina",
        ];
        for (i, f) in opts.features.iter().enumerate() {
            if !allowed.contains(&f.tag) || opts.features[..i].iter().any(|v| v.tag == f.tag) {
                return Err(error(ShapeErrorKind::UnsupportedFeature));
            }
        }
        let chars: Vec<_> = text.char_indices().collect();
        if chars.len() > 4096 {
            return Err(error(ShapeErrorKind::BudgetExceeded));
        }
        let mut items: Vec<Item> = Vec::new();
        for (i, &(offset, ch)) in chars.iter().enumerate() {
            let cp = ch as u32;
            let mark = is_mark(ch);
            let covered = if arabic {
                matches!(cp,0x20..=0x40|0x5b..=0x60|0x7b..=0x7e|0x0620..=0x064a|0x0660..=0x0669)
                    || mark
            } else {
                matches!(cp,0x20..=0x024f|0x0300..=0x036f)
            };
            if !covered {
                return Err(ShapeError {
                    text_offset: Some(offset),
                    ..error(ShapeErrorKind::UnsupportedScript)
                });
            }
            let gid = self.glyph_index(ch);
            if gid == 0 {
                return Err(ShapeError {
                    text_offset: Some(offset),
                    ..error(ShapeErrorKind::MissingGlyph)
                });
            }
            if gid >= self.num_glyphs {
                return Err(error(ShapeErrorKind::MalformedFont));
            }
            let end = offset + ch.len_utf8();
            let start = if mark {
                items.last().map_or(offset, |v| v.g.cluster.start)
            } else {
                offset
            };
            if mark && i > 64 && chars[i - 64..i].iter().all(|(_, c)| is_mark(*c)) {
                return Err(error(ShapeErrorKind::BudgetExceeded));
            }
            if mark {
                for prev in items
                    .iter_mut()
                    .rev()
                    .take_while(|v| v.g.cluster.start == start)
                {
                    prev.g.cluster.end = end;
                }
            }
            let mut form = *b"isol";
            if arabic && !mark {
                let current = joining(ch);
                let prev = chars[..i]
                    .iter()
                    .rev()
                    .find(|(_, c)| !is_mark(*c))
                    .map_or(0, |(_, c)| joining(*c));
                let next = chars[i + 1..]
                    .iter()
                    .find(|(_, c)| !is_mark(*c))
                    .map_or(0, |(_, c)| joining(*c));
                let before = prev == 2 && current != 0;
                let after = current == 2 && next != 0;
                form = match (before, after) {
                    (true, true) => *b"medi",
                    (true, false) => *b"fina",
                    (false, true) => *b"init",
                    _ => *b"isol",
                };
            }
            items.push(Item {
                g: ShapedGlyph {
                    glyph_id: gid,
                    cluster: start..end,
                    x_advance: 0,
                    y_advance: 0,
                    x_offset: 0,
                    y_offset: 0,
                },
                mark,
                positioned: false,
                form,
            });
        }
        let mut fuel = 1_000_000;
        let read_budget = std::cell::Cell::new(2_000_000);
        let gsub = table(self, *b"GSUB", &read_budget)?;
        let gpos = table(self, *b"GPOS", &read_budget)?;
        let gs = if let Some(t) = gsub {
            features(t, opts)?
        } else {
            BTreeMap::new()
        };
        let gp = if let Some(t) = gpos {
            features(t, opts)?
        } else {
            BTreeMap::new()
        };
        if opts.language != *b"dflt" && gs.is_empty() && gp.is_empty() {
            return Err(error(ShapeErrorKind::UnsupportedLanguage));
        }
        if arabic && gp.contains_key(b"curs") {
            return Err(error(ShapeErrorKind::UnsupportedLookup));
        }
        if arabic {
            for item in &items {
                if item.form != *b"isol"
                    && !item.mark
                    && enabled(opts, item.form)
                    && !gs.contains_key(&item.form)
                {
                    return Err(error(ShapeErrorKind::UnsupportedFeature));
                }
            }
        }
        for f in opts.features {
            if f.enabled && !gs.contains_key(&f.tag) && !gp.contains_key(&f.tag) {
                return Err(error(ShapeErrorKind::UnsupportedFeature));
            }
        }
        if let Some(t) = gsub {
            for tag in [
                *b"ccmp", *b"locl", *b"isol", *b"fina", *b"medi", *b"init", *b"rlig", *b"liga",
            ] {
                if enabled(opts, tag)
                    && let Some(ids) = gs.get(&tag)
                {
                    let form = if [*b"isol", *b"fina", *b"medi", *b"init"].contains(&tag) {
                        if !arabic {
                            continue;
                        }
                        Some(tag)
                    } else {
                        None
                    };
                    substitute(t, ids, &mut items, form, &mut fuel)?;
                }
            }
        }
        for item in &mut items {
            if item.g.glyph_id >= self.num_glyphs {
                return Err(error(ShapeErrorKind::MalformedFont));
            }
            item.g.x_advance = if item.mark {
                0
            } else {
                i32::from(self.advance_width(item.g.glyph_id))
            };
        }
        if opts.direction == Direction::RightToLeft {
            items.reverse();
        }
        if let Some(t) = gpos {
            for tag in [*b"kern", *b"mark", *b"mkmk"] {
                if enabled(opts, tag)
                    && let Some(ids) = gp.get(&tag)
                {
                    position(t, ids, &mut items, opts.direction, &mut fuel)?;
                }
            }
        }
        for item in &items {
            if item.mark && !item.positioned {
                return Err(ShapeError {
                    text_offset: Some(item.g.cluster.start),
                    ..error(ShapeErrorKind::UnpositionedMark)
                });
            }
        }
        Ok(ShapedRun {
            glyphs: items.into_iter().map(|i| i.g).collect(),
            direction: opts.direction,
            source: text.to_owned(),
        })
    }
}
