//! Font-wide bounds and horizontal extrema from the final static instance.
//!
//! `head` bounds and the three signed `hhea` extrema exclude contourless
//! glyphs; `advanceWidthMax` includes every horizontal metric, including space.
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/head>
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/hhea>

use crate::{be_i16, write_u16};

pub(super) struct Summary {
    bounds: [i16; 4],
    advance_max: u16,
    min_lsb: i16,
    min_rsb: i16,
    max_extent: i16,
}

impl Summary {
    /// A linear pass after composite bounds and horizontal metrics are final.
    /// All arithmetic is wider than the stored FWORDs; an unrepresentable
    /// static summary rejects the instance instead of wrapping or clamping.
    pub(super) fn new(glyf: &[u8], loca: &[u32], metrics: &[(u16, i16)]) -> Option<Self> {
        if loca.len() != metrics.len().checked_add(1)? {
            return None;
        }
        let mut bounds = None::<[i16; 4]>;
        let mut advance_max = 0;
        let mut min_lsb = i32::MAX;
        let mut min_rsb = i32::MAX;
        let mut max_extent = i32::MIN;
        for (range, &(advance, lsb)) in loca.windows(2).zip(metrics) {
            advance_max = advance_max.max(advance);
            let start = usize::try_from(range[0]).ok()?;
            let end = usize::try_from(range[1]).ok()?;
            let glyph = glyf.get(start..end)?;
            if glyph.is_empty() || be_i16(glyph, 0)? == 0 {
                continue;
            }
            let rect = [
                be_i16(glyph, 2)?,
                be_i16(glyph, 4)?,
                be_i16(glyph, 6)?,
                be_i16(glyph, 8)?,
            ];
            if rect[0] > rect[2] || rect[1] > rect[3] {
                return None;
            }
            bounds = Some(match bounds {
                None => rect,
                Some(old) => [
                    old[0].min(rect[0]),
                    old[1].min(rect[1]),
                    old[2].max(rect[2]),
                    old[3].max(rect[3]),
                ],
            });
            let extent = i32::from(lsb) + i32::from(rect[2]) - i32::from(rect[0]);
            min_lsb = min_lsb.min(i32::from(lsb));
            min_rsb = min_rsb.min(i32::from(advance) - extent);
            max_extent = max_extent.max(extent);
        }
        // A font with no contours has no signed extrema or ink bounds.
        if bounds.is_none() {
            min_lsb = 0;
            min_rsb = 0;
            max_extent = 0;
        }
        Some(Self {
            bounds: bounds.unwrap_or([0; 4]),
            advance_max,
            min_lsb: i16::try_from(min_lsb).ok()?,
            min_rsb: i16::try_from(min_rsb).ok()?,
            max_extent: i16::try_from(max_extent).ok()?,
        })
    }

    pub(super) fn apply_head(&self, bytes: &mut [u8]) -> Option<()> {
        for (index, value) in self.bounds.iter().enumerate() {
            write_u16(bytes, 36 + index * 2, *value as u16)?;
        }
        Some(())
    }

    pub(super) fn apply_hhea(&self, bytes: &mut [u8]) -> Option<()> {
        write_u16(bytes, 10, self.advance_max)?;
        write_u16(bytes, 12, self.min_lsb as u16)?;
        write_u16(bytes, 14, self.min_rsb as u16)?;
        write_u16(bytes, 16, self.max_extent as u16)
    }
}
