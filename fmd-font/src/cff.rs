//! Bounded CFF1 Type 2 decoding and deterministic, unhinted CFF subsetting.
//! CID-keyed CFF, CFF2, seac and computational charstring operators are
//! explicitly refused. Drawing, hints, flex and local/global subroutines are
//! supported. Coordinates and advances are font design units.
use crate::outline::Point;
use crate::{
    EmbeddingFormat, Font, MISSING_GLYPH_REMAP, Subset, SubsetError, SubsetErrorKind as Kind,
    be_u16, be_u32, find_table_full, table_checksum,
};
use std::collections::{BTreeMap, BTreeSet};

/// A CFF contour command. Cubics are retained without quadratic approximation.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CffOutline {
    pub commands: Vec<Command>,
    pub advance: u16,
    pub lsb: i16,
}

fn include_point(bounds: &mut Option<[f64; 4]>, p: Point) {
    *bounds = Some(match *bounds {
        None => [p.x, p.y, p.x, p.y],
        Some([x0, y0, x1, y1]) => [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
    });
}

/// Interior extrema of one cubic coordinate, with the constant derivative
/// factor removed. The stable quadratic formula avoids losing the smaller
/// root when the linear coefficient and discriminant have similar magnitude.
fn cubic_extrema(p0: f64, p1: f64, p2: f64, p3: f64) -> [Option<f64>; 2] {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    if a == 0.0 {
        return [if b == 0.0 { None } else { Some(-c / b) }, None];
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return [None, None];
    }
    let q = -0.5 * (b + discriminant.sqrt().copysign(b));
    if q == 0.0 {
        [Some(-b / (2.0 * a)), None]
    } else {
        [Some(q / a), Some(c / q)]
    }
}

fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let lerp = |a: Point, b: Point| Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    };
    let a = lerp(p0, p1);
    let b = lerp(p1, p2);
    let c = lerp(p2, p3);
    lerp(lerp(a, b), lerp(b, c))
}

/// Tight curve bounds, rather than the larger control-point envelope. Moves
/// without a drawing segment have no ink; a closing edge cannot extend the
/// bounds beyond its already included endpoints.
fn outline_bbox(outline: &CffOutline) -> Result<Option<[i16; 4]>, SubsetError> {
    let mut bounds = None;
    let mut current = None;
    for command in &outline.commands {
        let points: &[Point] = match command {
            Command::Move(p) | Command::Line(p) => std::slice::from_ref(p),
            Command::Curve(a, b, c) => &[*a, *b, *c],
            Command::Close => &[],
        };
        if points
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1e9 || p.y.abs() > 1e9)
        {
            return Err(malformed(0));
        }
        match *command {
            Command::Move(p) => current = Some(p),
            Command::Line(p) => {
                include_point(&mut bounds, current.ok_or(malformed(0))?);
                include_point(&mut bounds, p);
                current = Some(p);
            }
            Command::Curve(a, b, end) => {
                let start = current.ok_or(malformed(0))?;
                include_point(&mut bounds, start);
                include_point(&mut bounds, end);
                for t in cubic_extrema(start.x, a.x, b.x, end.x)
                    .into_iter()
                    .chain(cubic_extrema(start.y, a.y, b.y, end.y))
                    .flatten()
                    .filter(|&t| t > 0.0 && t < 1.0)
                {
                    include_point(&mut bounds, cubic_point(start, a, b, end, t));
                }
                current = Some(end);
            }
            Command::Close => current = None,
        }
    }
    let Some([x0, y0, x1, y1]) = bounds else {
        return Ok(None);
    };
    let rounded = [x0.floor(), y0.floor(), x1.ceil(), y1.ceil()];
    if rounded
        .iter()
        .any(|&v| v < f64::from(i16::MIN) || v > f64::from(i16::MAX))
    {
        return Err(err(Kind::Capacity, 0));
    }
    Ok(Some(rounded.map(|v| v as i16)))
}

fn err(kind: Kind, offset: usize) -> SubsetError {
    SubsetError::new(kind, *b"CFF ", None, Some(offset))
}
fn malformed(offset: usize) -> SubsetError {
    err(Kind::Malformed, offset)
}
fn slice(d: &[u8], start: usize, len: usize) -> Result<&[u8], SubsetError> {
    d.get(start..start.checked_add(len).ok_or(malformed(start))?)
        .ok_or(malformed(start))
}
fn index(d: &[u8], start: usize) -> Result<(Vec<&[u8]>, usize), SubsetError> {
    let count = usize::from(be_u16(d, start).ok_or(malformed(start))?);
    if count == 0 {
        return Ok((Vec::new(), start + 2));
    }
    let size = usize::from(*d.get(start + 2).ok_or(malformed(start + 2))?);
    if !(1..=4).contains(&size) {
        return Err(malformed(start + 2));
    }
    let base = start
        .checked_add(3 + (count + 1) * size)
        .ok_or(malformed(start))?;
    let offset = |i: usize| -> Result<usize, SubsetError> {
        let bytes = slice(d, start + 3 + i * size, size)?;
        let v = bytes.iter().fold(0usize, |a, &b| (a << 8) | usize::from(b));
        base.checked_add(v.checked_sub(1).ok_or(malformed(start))?)
            .ok_or(malformed(start))
    };
    if offset(0)? != base {
        return Err(malformed(start));
    }
    let mut objects = Vec::with_capacity(count);
    for i in 0..count {
        let a = offset(i)?;
        let b = offset(i + 1)?;
        objects.push(slice(d, a, b.checked_sub(a).ok_or(malformed(a))?)?);
    }
    Ok((objects, offset(count)?))
}
fn number(d: &[u8], p: &mut usize, dict: bool) -> Result<Option<f64>, SubsetError> {
    let at = *p;
    let b = *d.get(at).ok_or(malformed(at))?;
    let (v, n) = match b {
        32..=246 => (f64::from(b) - 139.0, 1),
        247..=250 => (
            f64::from(b - 247) * 256.0 + f64::from(*d.get(at + 1).ok_or(malformed(at))?) + 108.0,
            2,
        ),
        251..=254 => (
            -f64::from(b - 251) * 256.0 - f64::from(*d.get(at + 1).ok_or(malformed(at))?) - 108.0,
            2,
        ),
        28 => (f64::from(be_u16(d, at + 1).ok_or(malformed(at))? as i16), 3),
        29 if dict => (f64::from(be_u32(d, at + 1).ok_or(malformed(at))? as i32), 5),
        255 if !dict => (
            f64::from(be_u32(d, at + 1).ok_or(malformed(at))? as i32) / 65536.0,
            5,
        ),
        30 if dict => {
            let mut text = String::new();
            let mut pos = at + 1;
            'real: loop {
                let v = *d.get(pos).ok_or(malformed(pos))?;
                pos += 1;
                for digit in [v >> 4, v & 15] {
                    match digit {
                        0..=9 => text.push(char::from(b'0' + digit)),
                        10 => text.push('.'),
                        11 => text.push('E'),
                        12 => text.push_str("E-"),
                        14 => text.push('-'),
                        15 => break 'real,
                        _ => return Err(malformed(pos - 1)),
                    }
                    if text.len() > 64 {
                        return Err(err(Kind::BudgetExceeded, at));
                    }
                }
            }
            let v = text.parse::<f64>().map_err(|_| malformed(at))?;
            if !v.is_finite() {
                return Err(malformed(at));
            }
            (v, pos - at)
        }
        _ => return Ok(None),
    };
    *p += n;
    Ok(Some(v))
}
fn dictionary(d: &[u8]) -> Result<BTreeMap<u16, Vec<f64>>, SubsetError> {
    let mut out = BTreeMap::new();
    let mut stack = Vec::new();
    let mut p = 0;
    while p < d.len() {
        if let Some(v) = number(d, &mut p, true)? {
            stack.push(v);
            if stack.len() > 48 {
                return Err(err(Kind::BudgetExceeded, p));
            }
            continue;
        }
        let b = d[p];
        p += 1;
        let op = if b == 12 {
            let b = *d.get(p).ok_or(malformed(p))?;
            p += 1;
            0x0c00 + u16::from(b)
        } else {
            u16::from(b)
        };
        if out.insert(op, std::mem::take(&mut stack)).is_some() {
            return Err(malformed(p));
        }
    }
    if !stack.is_empty() {
        return Err(malformed(p));
    }
    Ok(out)
}
fn integer(v: f64, p: usize) -> Result<usize, SubsetError> {
    if v < 0.0 || v > u32::MAX as f64 || v.fract() != 0.0 {
        Err(malformed(p))
    } else {
        Ok(v as usize)
    }
}
struct Cff<'a> {
    chars: Vec<&'a [u8]>,
    local: Vec<&'a [u8]>,
    global: Vec<&'a [u8]>,
}
impl<'a> Cff<'a> {
    fn parse(font: &'a Font) -> Result<Self, SubsetError> {
        font.validate_font_metrics()?;
        let (o, n) = find_table_full(&font.data, b"CFF ").ok_or(SubsetError::new(
            Kind::UnsupportedFormat,
            *b"sfnt",
            None,
            None,
        ))?;
        let d = slice(&font.data, o, n)?;
        if d.first() != Some(&1) {
            return Err(err(Kind::UnsupportedFormat, 0));
        }
        let header = usize::from(*d.get(2).ok_or(malformed(2))?);
        if header < 4 || !matches!(d.get(3), Some(1..=4)) {
            return Err(malformed(2));
        }
        let (names, p) = index(d, header)?;
        if names.len() != 1 {
            return Err(err(Kind::UnsupportedFormat, header));
        }
        let (top, p) = index(d, p)?;
        if top.len() != 1 {
            return Err(malformed(p));
        }
        let (_, p) = index(d, p)?;
        let (global, _) = index(d, p)?;
        let dict = dictionary(top[0])?;
        if dict.get(&0x0c05).is_some_and(|v| v.as_slice() != [0.0]) {
            return Err(err(Kind::UnsupportedFormat, header));
        }
        if dict.contains_key(&0x0c1e) {
            return Err(err(Kind::UnsupportedFormat, header));
        }
        if dict.get(&0x0c06).is_some_and(|v| v.as_slice() != [2.0]) {
            return Err(err(Kind::UnsupportedFormat, header));
        }
        if let Some(matrix) = dict.get(&0x0c07) {
            let unit = 1.0 / f64::from(font.units_per_em);
            if matrix.len() != 6
                || matrix
                    .iter()
                    .zip([unit, 0.0, 0.0, unit, 0.0, 0.0])
                    .any(|(a, b)| (a - b).abs() > 1e-12)
            {
                return Err(err(Kind::UnsupportedFormat, header));
            }
        } else if font.units_per_em != 1000 {
            return Err(err(Kind::UnsupportedFormat, header));
        }
        let cs = dict
            .get(&17)
            .filter(|v| v.len() == 1)
            .ok_or(malformed(header))?;
        let (chars, _) = index(d, integer(cs[0], header)?)?;
        if chars.len() != usize::from(font.num_glyphs) {
            return Err(malformed(header));
        }
        let mut local = Vec::new();
        if let Some(private) = dict.get(&18) {
            if private.len() != 2 {
                return Err(malformed(header));
            }
            let size = integer(private[0], header)?;
            let base = integer(private[1], header)?;
            let pd = dictionary(slice(d, base, size)?)?;
            if let Some(subrs) = pd.get(&19) {
                if subrs.len() != 1 {
                    return Err(malformed(base));
                }
                let sub = base
                    .checked_add(integer(subrs[0], base)?)
                    .ok_or(malformed(base))?;
                local = index(d, sub)?.0;
            }
        }
        Ok(Self {
            chars,
            local,
            global,
        })
    }
    fn outline(&self, font: &Font, gid: u16) -> Result<CffOutline, SubsetError> {
        self.outline_budget(font, gid, &mut 100_000)
    }
    fn outline_budget(
        &self,
        font: &Font,
        gid: u16,
        budget: &mut usize,
    ) -> Result<CffOutline, SubsetError> {
        let initial = (*budget).min(100_000);
        let program = self.chars.get(usize::from(gid)).ok_or(SubsetError::new(
            Kind::InvalidGlyph,
            *b"CFF ",
            Some(gid),
            None,
        ))?;
        let mut vm = Vm {
            cff: self,
            stack: Vec::new(),
            out: Vec::new(),
            x: 0.0,
            y: 0.0,
            open: false,
            width: false,
            stems: 0,
            fuel: initial,
        };
        let ended = vm.run(program, 0).map_err(|mut e| {
            e.glyph = Some(gid);
            e
        })?;
        *budget -= initial - vm.fuel;
        if !ended {
            return Err(SubsetError::new(
                Kind::Malformed,
                *b"CFF ",
                Some(gid),
                Some(program.len()),
            ));
        }
        Ok(CffOutline {
            commands: vm.out,
            advance: font.advance_width(gid),
            lsb: font.left_side_bearing(gid),
        })
    }
}
struct Vm<'a, 'b> {
    cff: &'a Cff<'b>,
    stack: Vec<f64>,
    out: Vec<Command>,
    x: f64,
    y: f64,
    open: bool,
    width: bool,
    stems: usize,
    fuel: usize,
}
impl Vm<'_, '_> {
    fn point(&mut self, dx: f64, dy: f64) -> Result<Point, SubsetError> {
        self.x += dx;
        self.y += dy;
        if !self.x.is_finite() || !self.y.is_finite() || self.x.abs() > 1e9 || self.y.abs() > 1e9 {
            return Err(malformed(0));
        }
        Ok(Point {
            x: self.x,
            y: self.y,
        })
    }
    fn close(&mut self) {
        if self.open {
            self.out.push(Command::Close);
            self.open = false;
        }
    }
    fn line(&mut self, dx: f64, dy: f64) -> Result<(), SubsetError> {
        if !self.open {
            return Err(malformed(0));
        }
        let p = self.point(dx, dy)?;
        self.out.push(Command::Line(p));
        Ok(())
    }
    fn curve(&mut self, v: &[f64]) -> Result<(), SubsetError> {
        if !self.open || v.len() != 6 {
            return Err(malformed(0));
        }
        let a = self.point(v[0], v[1])?;
        let b = self.point(v[2], v[3])?;
        let c = self.point(v[4], v[5])?;
        self.out.push(Command::Curve(a, b, c));
        Ok(())
    }
    fn run(&mut self, d: &[u8], depth: usize) -> Result<bool, SubsetError> {
        if depth > 10 {
            return Err(err(Kind::BudgetExceeded, 0));
        }
        let mut p = 0;
        while p < d.len() {
            if self.fuel == 0 || self.out.len() > 65_536 {
                return Err(err(Kind::BudgetExceeded, p));
            }
            self.fuel -= 1;
            if let Some(v) = number(d, &mut p, false)? {
                self.stack.push(v);
                if self.stack.len() > 48 {
                    return Err(err(Kind::BudgetExceeded, p));
                }
                continue;
            }
            let at = p;
            let op = d[p];
            p += 1;
            if op == 10 || op == 29 {
                let v = self.stack.pop().ok_or(malformed(at))?;
                let subrs = if op == 10 {
                    &self.cff.local
                } else {
                    &self.cff.global
                };
                let bias = if subrs.len() < 1240 {
                    107.0
                } else if subrs.len() < 33900 {
                    1131.0
                } else {
                    32768.0
                };
                let i = integer(v + bias, at)?;
                let sub = subrs.get(i).ok_or(malformed(at))?;
                if self.run(sub, depth + 1)? {
                    return Ok(true);
                }
                continue;
            }
            if op == 11 {
                if depth == 0 {
                    return Err(malformed(at));
                }
                return Ok(false);
            }
            let mut s = std::mem::take(&mut self.stack);
            match op {
                1 | 3 | 18 | 23 | 19 | 20 => {
                    if !self.width && s.len() % 2 == 1 {
                        s.remove(0);
                    }
                    self.width = true;
                    if s.len() % 2 != 0 {
                        return Err(malformed(at));
                    }
                    self.stems += s.len() / 2;
                    if self.stems > 96 {
                        return Err(err(Kind::BudgetExceeded, at));
                    }
                    if op == 19 || op == 20 {
                        let n = self.stems.div_ceil(8);
                        slice(d, p, n)?;
                        p += n;
                    }
                }
                4 | 21 | 22 => {
                    let n = if op == 21 { 2 } else { 1 };
                    if !self.width && s.len() == n + 1 {
                        s.remove(0);
                    }
                    self.width = true;
                    if s.len() != n {
                        return Err(malformed(at));
                    }
                    self.close();
                    let (dx, dy) = match op {
                        4 => (0.0, s[0]),
                        22 => (s[0], 0.0),
                        _ => (s[0], s[1]),
                    };
                    let pt = self.point(dx, dy)?;
                    self.out.push(Command::Move(pt));
                    self.open = true;
                }
                5 => {
                    if s.is_empty() || s.len() % 2 != 0 {
                        return Err(malformed(at));
                    }
                    for v in s.chunks_exact(2) {
                        self.line(v[0], v[1])?;
                    }
                }
                6 | 7 => {
                    if s.is_empty() {
                        return Err(malformed(at));
                    }
                    let mut horizontal = op == 6;
                    for v in s {
                        self.line(
                            if horizontal { v } else { 0.0 },
                            if horizontal { 0.0 } else { v },
                        )?;
                        horizontal = !horizontal;
                    }
                }
                8 => {
                    if s.is_empty() || s.len() % 6 != 0 {
                        return Err(malformed(at));
                    }
                    for v in s.chunks_exact(6) {
                        self.curve(v)?;
                    }
                }
                24 => {
                    if s.len() < 8 || (s.len() - 2) % 6 != 0 {
                        return Err(malformed(at));
                    }
                    let n = s.len() - 2;
                    for v in s[..n].chunks_exact(6) {
                        self.curve(v)?;
                    }
                    self.line(s[n], s[n + 1])?;
                }
                25 => {
                    if s.len() < 8 || (s.len() - 6) % 2 != 0 {
                        return Err(malformed(at));
                    }
                    let n = s.len() - 6;
                    for v in s[..n].chunks_exact(2) {
                        self.line(v[0], v[1])?;
                    }
                    self.curve(&s[n..])?;
                }
                26 | 27 => {
                    if s.len() < 4 || !matches!(s.len() % 4, 0 | 1) {
                        return Err(malformed(at));
                    }
                    let mut extra = if s.len() % 4 == 1 { s.remove(0) } else { 0.0 };
                    for v in s.chunks_exact(4) {
                        if op == 26 {
                            self.curve(&[extra, v[0], v[1], v[2], 0.0, v[3]])?;
                        } else {
                            self.curve(&[v[0], extra, v[1], v[2], v[3], 0.0])?;
                        }
                        extra = 0.0;
                    }
                }
                30 | 31 => {
                    if s.len() < 4 || !matches!(s.len() % 4, 0 | 1) {
                        return Err(malformed(at));
                    }
                    let count = s.len() / 4;
                    let mut horizontal = op == 31;
                    for (i, v) in s[..count * 4].chunks_exact(4).enumerate() {
                        let extra = if i + 1 == count && s.len() % 4 == 1 {
                            s[s.len() - 1]
                        } else {
                            0.0
                        };
                        if horizontal {
                            self.curve(&[v[0], 0.0, v[1], v[2], extra, v[3]])?;
                        } else {
                            self.curve(&[0.0, v[0], v[1], v[2], v[3], extra])?;
                        }
                        horizontal = !horizontal;
                    }
                }
                12 => {
                    let escape = *d.get(p).ok_or(malformed(p))?;
                    p += 1;
                    match escape {
                        34 if s.len() == 7 => {
                            self.curve(&[s[0], 0.0, s[1], s[2], s[3], 0.0])?;
                            self.curve(&[s[4], 0.0, s[5], -s[2], s[6], 0.0])?;
                        }
                        35 if s.len() == 13 => {
                            self.curve(&s[..6])?;
                            self.curve(&s[6..12])?;
                        }
                        36 if s.len() == 9 => {
                            self.curve(&[s[0], s[1], s[2], s[3], s[4], 0.0])?;
                            self.curve(&[s[5], 0.0, s[6], s[7], s[8], -s[1] - s[3] - s[7]])?;
                        }
                        37 if s.len() == 11 => {
                            let dx = s[0] + s[2] + s[4] + s[6] + s[8];
                            let dy = s[1] + s[3] + s[5] + s[7] + s[9];
                            self.curve(&s[..6])?;
                            let (x, y) = if dx.abs() > dy.abs() {
                                (s[10], -dy)
                            } else {
                                (-dx, s[10])
                            };
                            self.curve(&[s[6], s[7], s[8], s[9], x, y])?;
                        }
                        34..=37 => return Err(malformed(at)),
                        _ => return Err(err(Kind::UnsupportedOperator, at)),
                    }
                }
                14 => {
                    if !self.width && s.len() % 2 == 1 {
                        s.remove(0);
                    }
                    if s.len() == 4 {
                        return Err(err(Kind::UnsupportedOperator, at));
                    }
                    if !s.is_empty() {
                        return Err(malformed(at));
                    }
                    self.close();
                    return Ok(true);
                }
                _ => return Err(err(Kind::UnsupportedOperator, at)),
            }
        }
        Err(malformed(p))
    }
}
impl Font {
    /// Decode a CFF1 glyph, preserving exact cubic control points. No host APIs.
    pub fn cff_outline(&self, gid: u16) -> Result<CffOutline, SubsetError> {
        Cff::parse(self)?.outline(self, gid)
    }

    /// Ink bounds `[xMin, yMin, xMax, yMax]` of a CFF1 glyph in design units,
    /// including interior cubic extrema and rounded outward. Empty glyphs
    /// return `Ok(None)`. Decode errors remain typed; bounds outside the `i16`
    /// range return [`SubsetErrorKind::Capacity`](crate::SubsetErrorKind::Capacity)
    /// instead of silently clamping away ink.
    pub fn cff_glyph_bbox(&self, gid: u16) -> Result<Option<[i16; 4]>, SubsetError> {
        outline_bbox(&self.cff_outline(gid)?).map_err(|mut e| {
            e.glyph = Some(gid);
            e
        })
    }
}
fn dict_int(out: &mut Vec<u8>, v: i32) {
    out.push(29);
    out.extend_from_slice(&v.to_be_bytes());
}
fn encode_index(objects: &[Vec<u8>]) -> Result<Vec<u8>, SubsetError> {
    let count = u16::try_from(objects.len()).map_err(|_| err(Kind::Capacity, 0))?;
    let mut out = count.to_be_bytes().to_vec();
    if count == 0 {
        return Ok(out);
    }
    out.push(4);
    let mut offset = 1u32;
    out.extend_from_slice(&offset.to_be_bytes());
    for v in objects {
        offset = offset
            .checked_add(u32::try_from(v.len()).map_err(|_| err(Kind::Capacity, 0))?)
            .ok_or(err(Kind::Capacity, 0))?;
        out.extend_from_slice(&offset.to_be_bytes());
    }
    for v in objects {
        out.extend_from_slice(v);
    }
    Ok(out)
}
fn cs_num(out: &mut Vec<u8>, v: f64) -> Result<(), SubsetError> {
    if v.fract() == 0.0 && (-32768.0..=32767.0).contains(&v) {
        out.push(28);
        out.extend_from_slice(&(v as i16).to_be_bytes());
    } else {
        let fixed = (v * 65536.0).round();
        if fixed < i32::MIN as f64 || fixed > i32::MAX as f64 {
            return Err(err(Kind::Capacity, 0));
        }
        out.push(255);
        out.extend_from_slice(&(fixed as i32).to_be_bytes());
    }
    Ok(())
}
fn encode_outline(o: &CffOutline) -> Result<Vec<u8>, SubsetError> {
    let mut out = Vec::new();
    cs_num(&mut out, f64::from(o.advance))?;
    let mut current = Point { x: 0.0, y: 0.0 };
    for c in &o.commands {
        match c {
            Command::Move(p) | Command::Line(p) => {
                cs_num(&mut out, p.x - current.x)?;
                cs_num(&mut out, p.y - current.y)?;
                out.push(if matches!(c, Command::Move(_)) { 21 } else { 5 });
                current = *p;
            }
            Command::Curve(a, b, c) => {
                for p in [a, b, c] {
                    cs_num(&mut out, p.x - current.x)?;
                    cs_num(&mut out, p.y - current.y)?;
                    current = *p;
                }
                out.push(8);
            }
            Command::Close => {}
        }
    }
    out.push(14);
    Ok(out)
}
fn cff_program(
    outlines: &[CffOutline],
    upem: u16,
    bounds: [i16; 4],
) -> Result<Vec<u8>, SubsetError> {
    let names = encode_index(&[b"FmdSubset".to_vec()])?;
    let strings: Vec<_> = (1..outlines.len())
        .map(|i| format!("g{i}").into_bytes())
        .collect();
    let strings = encode_index(&strings)?;
    let chars = encode_index(
        &outlines
            .iter()
            .map(encode_outline)
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let mut charset = vec![0];
    for i in 1..outlines.len() {
        let sid = u16::try_from(390 + i).map_err(|_| err(Kind::Capacity, 0))?;
        charset.extend_from_slice(&sid.to_be_bytes());
    }
    // Fixed-width DICT offsets make layout independent of offset magnitude.
    let make_top = |charset_off: i32, chars_off: i32| -> Vec<u8> {
        let mut d = Vec::new();
        for &v in &bounds {
            dict_int(&mut d, i32::from(v));
        }
        d.push(5);
        dict_int(&mut d, 0);
        dict_int(&mut d, chars_off + chars.len() as i32);
        d.push(18);
        dict_int(&mut d, charset_off);
        d.push(15);
        dict_int(&mut d, chars_off);
        d.push(17);
        // Real FontMatrix = 1/upem. Decimal round-trip is deterministic.
        let value = format!("{:.16}", 1.0 / f64::from(upem));
        for val in [value.as_str(), "0", "0", value.as_str(), "0", "0"] {
            d.push(30);
            let mut digits: Vec<u8> = val
                .bytes()
                .map(|b| if b == b'.' { 10 } else { b - b'0' })
                .collect();
            digits.push(15);
            if digits.len() % 2 != 0 {
                digits.push(15);
            }
            for pair in digits.chunks_exact(2) {
                d.push(pair[0] * 16 + pair[1]);
            }
        }
        d.extend_from_slice(&[12, 7]);
        d
    };
    let top_size = encode_index(&[make_top(0, 0)])?.len();
    let charset_off = 4 + names.len() + top_size + strings.len() + 2;
    let mut out = vec![1, 0, 4, 4];
    out.extend(names);
    out.extend(encode_index(&[make_top(
        charset_off as i32,
        (charset_off + charset.len()) as i32,
    )])?);
    out.extend(strings);
    out.extend([0, 0]);
    out.extend(charset);
    out.extend(chars);
    Ok(out)
}
pub(crate) fn subset(font: &Font, glyphs: &[u16], chars: &[char]) -> Result<Subset, SubsetError> {
    let cff = Cff::parse(font)?;
    let mut keep = BTreeSet::from([0]);
    for &g in glyphs {
        if g >= font.num_glyphs {
            return Err(SubsetError::new(
                Kind::InvalidGlyph,
                *b"maxp",
                Some(g),
                None,
            ));
        }
        keep.insert(g);
    }
    let mut map = vec![MISSING_GLYPH_REMAP; usize::from(font.num_glyphs)];
    let mut outlines = Vec::new();
    let mut bounds: Option<[i16; 4]> = None;
    let mut command_count = 0usize;
    let mut instruction_budget = 5_000_000usize;
    for (i, &g) in keep.iter().enumerate() {
        map[usize::from(g)] = i as u16;
        let outline = cff.outline_budget(font, g, &mut instruction_budget)?;
        if let Some([x0, y0, x1, y1]) = outline_bbox(&outline).map_err(|mut e| {
            e.glyph = Some(g);
            e
        })? {
            bounds = Some(match bounds {
                None => [x0, y0, x1, y1],
                Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1), d.max(y1)],
            });
        }
        command_count += outline.commands.len();
        if command_count > 1_000_000 {
            return Err(err(Kind::BudgetExceeded, 0));
        }
        outlines.push(outline);
    }
    let bounds = bounds.unwrap_or([0; 4]);
    let cff = cff_program(&outlines, font.units_per_em, bounds)?;
    let mut tables = BTreeMap::new();
    for tag in [*b"head", *b"hhea", *b"OS/2", *b"name"] {
        if let Some((o, n)) = find_table_full(&font.data, &tag) {
            tables.insert(tag, slice(&font.data, o, n)?.to_vec());
        }
    }
    // Avoid carrying reserved source font names into a derivative program.
    tables.insert(*b"name", vec![0, 0, 0, 0, 0, 6]);
    let n = keep.len() as u16;
    let head = tables.get_mut(b"head").ok_or(malformed(0))?;
    crate::write_u32(head, 8, 0).ok_or(malformed(8))?;
    crate::write_u16(head, 50, 0).ok_or(malformed(50))?;
    for (i, value) in bounds.into_iter().enumerate() {
        crate::write_u16(head, 36 + i * 2, value as u16).ok_or(malformed(36 + i * 2))?;
    }
    crate::write_u16(tables.get_mut(b"hhea").ok_or(malformed(0))?, 34, n).ok_or(malformed(34))?;
    let mut maxp = 0x0000_5000u32.to_be_bytes().to_vec();
    maxp.extend_from_slice(&n.to_be_bytes());
    tables.insert(*b"maxp", maxp);
    let mut metrics = Vec::new();
    for o in &outlines {
        metrics.extend_from_slice(&o.advance.to_be_bytes());
        metrics.extend_from_slice(&o.lsb.to_be_bytes());
    }
    tables.insert(*b"hmtx", metrics);
    let cmap = font.build_cmap12(chars, &map).ok_or(SubsetError::new(
        Kind::Capacity,
        *b"cmap",
        None,
        None,
    ))?;
    tables.insert(*b"cmap", cmap);
    let mut post = vec![0; 32];
    post[..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    tables.insert(*b"post", post);
    tables.insert(*b"CFF ", cff);
    Ok(Subset {
        bytes: assemble(&tables)?,
        glyph_map: map,
        format: EmbeddingFormat::OpenTypeCff,
    })
}
fn assemble(tables: &BTreeMap<[u8; 4], Vec<u8>>) -> Result<Vec<u8>, SubsetError> {
    let n = tables.len();
    let pow = 1usize << n.ilog2();
    let mut out = vec![0; 12 + n * 16];
    out[..4].copy_from_slice(b"OTTO");
    for (off, v) in [
        (4, n as u16),
        (6, (pow * 16) as u16),
        (8, n.ilog2() as u16),
        (10, ((n - pow) * 16) as u16),
    ] {
        out[off..off + 2].copy_from_slice(&v.to_be_bytes());
    }
    let mut head = 0;
    for (i, (tag, data)) in tables.iter().enumerate() {
        while out.len() % 4 != 0 {
            out.push(0);
        }
        let offset = out.len();
        let p = 12 + i * 16;
        out[p..p + 4].copy_from_slice(tag);
        for (p, v) in [
            (p + 4, table_checksum(data)),
            (
                p + 8,
                u32::try_from(offset).map_err(|_| err(Kind::Capacity, 0))?,
            ),
            (p + 12, data.len() as u32),
        ] {
            out[p..p + 4].copy_from_slice(&v.to_be_bytes());
        }
        if tag == b"head" {
            head = offset;
        }
        out.extend_from_slice(data);
    }
    while out.len() % 4 != 0 {
        out.push(0);
    }
    let sum = 0xB1B0_AFBAu32.wrapping_sub(table_checksum(&out));
    crate::write_u32(&mut out, head + 8, sum).ok_or(malformed(head))?;
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    const FONT: &[u8] = include_bytes!("../fonts/test-cff/Bravura.otf");

    fn outline(commands: Vec<Command>) -> CffOutline {
        CffOutline {
            commands,
            advance: 500,
            lsb: 0,
        }
    }

    fn point(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn cubic_bounds_include_extrema_without_including_control_points() {
        let arch = outline(vec![
            Command::Move(point(0.0, 0.0)),
            Command::Curve(point(0.0, 100.0), point(100.0, 100.0), point(100.0, 0.0)),
            Command::Close,
        ]);
        assert_eq!(outline_bbox(&arch).unwrap(), Some([0, 0, 100, 75]));

        let two_extrema = outline(vec![
            Command::Move(point(0.0, 0.0)),
            Command::Curve(point(100.0, 100.0), point(-100.0, -100.0), point(0.0, 0.0)),
            Command::Close,
        ]);
        // Interior extrema are +/- 100 * sqrt(3) / 6, about 28.868.
        assert_eq!(
            outline_bbox(&two_extrema).unwrap(),
            Some([-29, -29, 29, 29])
        );

        let controls_outside_i16 = outline(vec![
            Command::Move(point(0.0, 10_000.0)),
            Command::Curve(
                point(0.0, 40_000.0),
                point(100.0, 40_000.0),
                point(100.0, 10_000.0),
            ),
            Command::Close,
        ]);
        assert_eq!(
            outline_bbox(&controls_outside_i16).unwrap(),
            Some([0, 10_000, 100, 32_500])
        );
    }

    #[test]
    fn bounds_handle_degenerate_derivatives_and_outward_rounding() {
        let linear = outline(vec![
            Command::Move(point(0.0, 0.0)),
            Command::Curve(point(10.0, 10.0), point(20.0, 20.0), point(30.0, 30.0)),
        ]);
        assert_eq!(outline_bbox(&linear).unwrap(), Some([0, 0, 30, 30]));

        let repeated_root = outline(vec![
            Command::Move(point(-0.125, -0.125)),
            Command::Curve(
                point(0.125, 0.125),
                point(-0.125, -0.125),
                point(0.125, 0.125),
            ),
        ]);
        assert_eq!(outline_bbox(&repeated_root).unwrap(), Some([-1, -1, 1, 1]));

        let fractional = outline(vec![
            Command::Move(point(30_000.0, 30_000.0)),
            Command::Close,
            Command::Move(point(0.5, -1.25)),
            Command::Line(point(0.75, -0.75)),
            Command::Close,
        ]);
        assert_eq!(outline_bbox(&fractional).unwrap(), Some([0, -2, 1, 0]));
        assert_eq!(outline_bbox(&outline(vec![])).unwrap(), None);
        assert_eq!(
            outline_bbox(&outline(vec![
                Command::Move(point(1.0, 2.0)),
                Command::Close
            ]))
            .unwrap(),
            None
        );
    }

    fn single_glyph_font(glyph: &CffOutline) -> Font {
        let mut tables = BTreeMap::new();
        for tag in [*b"head", *b"hhea", *b"cmap"] {
            let (o, n) = find_table_full(FONT, &tag).unwrap();
            tables.insert(tag, FONT[o..o + n].to_vec());
        }
        crate::write_u32(tables.get_mut(b"head").unwrap(), 8, 0).unwrap();
        crate::write_u16(tables.get_mut(b"hhea").unwrap(), 34, 1).unwrap();
        tables.insert(*b"maxp", vec![0, 0, 0x50, 0, 0, 1]);
        let mut metrics = glyph.advance.to_be_bytes().to_vec();
        metrics.extend_from_slice(&glyph.lsb.to_be_bytes());
        tables.insert(*b"hmtx", metrics);
        // Deliberately false declared bounds ensure callers measure the ink.
        tables.insert(
            *b"CFF ",
            cff_program(std::slice::from_ref(glyph), 1000, [0; 4]).unwrap(),
        );
        Font::parse(assemble(&tables).unwrap()).unwrap()
    }

    #[test]
    fn unrepresentable_ink_is_a_typed_error_not_a_clamped_box() {
        let glyph = outline(vec![
            Command::Move(point(0.0, 0.0)),
            Command::Line(point(20_000.0, 0.0)),
            Command::Line(point(40_000.0, 0.0)),
            Command::Close,
        ]);
        let font = single_glyph_font(&glyph);
        for error in [
            font.cff_glyph_bbox(0).unwrap_err(),
            font.try_subset_glyphs(&[0], &[]).unwrap_err(),
        ] {
            assert_eq!(error.kind, Kind::Capacity);
            assert_eq!(error.table, *b"CFF ");
            assert_eq!(error.glyph, Some(0));
        }
        assert_eq!(font.glyph_bbox(0), None);

        let fractional_overflow = outline(vec![
            Command::Move(point(0.0, 0.0)),
            Command::Line(point(32767.25, -32768.25)),
        ]);
        assert_eq!(
            outline_bbox(&fractional_overflow).unwrap_err().kind,
            Kind::Capacity
        );
        for commands in [
            vec![Command::Line(point(0.0, 0.0))],
            vec![Command::Move(point(f64::NAN, 0.0))],
        ] {
            assert_eq!(
                outline_bbox(&outline(commands)).unwrap_err().kind,
                Kind::Malformed
            );
        }
    }

    #[test]
    fn charstring_failures_keep_distinct_typed_causes() {
        let f = Font::parse(FONT.to_vec()).unwrap();
        for (program, kind) in [
            (vec![12, 23, 14], Kind::UnsupportedOperator),
            (vec![28, 0], Kind::Malformed),
            (vec![5, 14], Kind::Malformed),
            (vec![11], Kind::Malformed),
        ] {
            let c = Cff {
                chars: vec![&program],
                local: vec![],
                global: vec![],
            };
            let e = c.outline(&f, 0).unwrap_err();
            assert_eq!(e.kind, kind);
            assert_eq!(e.glyph, Some(0));
            assert!(e.offset.is_some());
        }
        let recursive = [32, 10, 11];
        let main = [32, 10, 14];
        let c = Cff {
            chars: vec![&main],
            local: vec![&recursive],
            global: vec![],
        };
        assert_eq!(c.outline(&f, 0).unwrap_err().kind, Kind::BudgetExceeded);
    }
    #[test]
    fn zero_glyph_font_and_cff2_refuse_without_panics() {
        let mut d = FONT.to_vec();
        let (o, _) = find_table_full(&d, b"maxp").unwrap();
        crate::write_u16(&mut d, o + 4, 0).unwrap();
        let f = Font::parse(d).unwrap();
        assert_eq!(
            f.try_subset_glyphs(&[], &[]).unwrap_err().kind,
            Kind::Malformed
        );
        let mut d = FONT.to_vec();
        let (o, _) = find_table_full(&d, b"CFF ").unwrap();
        d[o] = 2;
        let f = Font::parse(d).unwrap();
        assert_eq!(f.cff_outline(0).unwrap_err().kind, Kind::UnsupportedFormat);
        assert_eq!(
            f.cff_glyph_bbox(0).unwrap_err().kind,
            Kind::UnsupportedFormat
        );
    }
    #[test]
    fn cff_subset_has_valid_sfnt_checksum() {
        let f = Font::parse(FONT.to_vec()).unwrap();
        let s = f.try_subset_glyphs(&[5, 1000], &[]).unwrap();
        assert_eq!(table_checksum(&s.bytes), 0xB1B0_AFBA);
        let subset = Font::parse(s.bytes).unwrap();
        let (start, len) = find_table_full(&subset.data, b"CFF ").unwrap();
        let data = &subset.data[start..start + len];
        let (_, pos) = index(data, usize::from(data[2])).unwrap();
        let (top, _) = index(data, pos).unwrap();
        let dict = dictionary(top[0]).unwrap();
        assert_eq!(dict[&5], subset.font_bbox().unwrap().map(f64::from));
    }
}
