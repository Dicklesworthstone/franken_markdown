//! Bounded GIF87a/89a first-frame decoding for a static PDF image.
//!
//! The container reader also supplies intrinsic dimensions to the SVG backend,
//! which preserves the original animation. The decoder follows the GIF89a
//! specification, including deferred LZW clears and four-pass interlacing:
//! <https://www.w3.org/Graphics/GIF/spec-gif89a.txt>.

use super::{MAX_PDF_IMAGE_COMPRESSED_BYTES, MAX_PDF_IMAGE_PIXELS};

pub(super) struct Frame {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgb: Vec<u8>,
    pub(super) alpha: Option<Vec<u8>>,
}

struct Image<'a> {
    canvas_width: u16,
    canvas_height: u16,
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    interlaced: bool,
    palette: &'a [u8],
    background: Option<&'a [u8]>,
    transparent: Option<u8>,
    code_size: u8,
    blocks: &'a [u8],
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let bytes = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn palette(&mut self, packed: u8) -> Option<&'a [u8]> {
        if packed & 0x80 == 0 {
            Some(&[])
        } else {
            self.take(3 * (2usize << (packed & 7)))
        }
    }

    fn blocks(&mut self) -> Option<&'a [u8]> {
        let start = self.pos;
        loop {
            let len = usize::from(self.byte()?);
            if len == 0 {
                return self.bytes.get(start..self.pos);
            }
            self.take(len)?;
        }
    }
}

fn short(bytes: &[u8], index: usize) -> u16 {
    u16::from_le_bytes([bytes[index], bytes[index + 1]])
}

/// Validate block framing through the trailer without allocating animation
/// frames or trusting dimensions before the shared raster bounds are checked.
fn first_image(bytes: &[u8]) -> Option<Image<'_>> {
    if bytes.len() > MAX_PDF_IMAGE_COMPRESSED_BYTES
        || !matches!(bytes.get(..6)?, b"GIF87a" | b"GIF89a")
    {
        return None;
    }
    let mut reader = Reader { bytes, pos: 6 };
    let screen = reader.take(7)?;
    let canvas_width = short(screen, 0);
    let canvas_height = short(screen, 2);
    if canvas_width == 0
        || canvas_height == 0
        || u64::from(canvas_width) * u64::from(canvas_height) > MAX_PDF_IMAGE_PIXELS
    {
        return None;
    }
    let global = reader.palette(screen[4])?;
    let background = if global.is_empty() {
        None
    } else {
        let offset = usize::from(screen[5]) * 3;
        Some(global.get(offset..offset + 3)?)
    };
    let mut transparent = None;
    let mut first = None;
    loop {
        match reader.byte()? {
            0x21 => match reader.byte()? {
                0xf9 => {
                    if reader.byte()? != 4 {
                        return None;
                    }
                    let control = reader.take(4)?;
                    if reader.byte()? != 0 {
                        return None;
                    }
                    transparent = (control[0] & 1 != 0).then_some(control[3]);
                }
                // Plain-text graphics require a font and are outside this
                // raster decoder. Keep the usual visible alt-text fallback.
                0x01 => return None,
                _ => {
                    reader.blocks()?;
                }
            },
            0x2c => {
                let descriptor = reader.take(9)?;
                let left = short(descriptor, 0);
                let top = short(descriptor, 2);
                let width = short(descriptor, 4);
                let height = short(descriptor, 6);
                if width == 0
                    || height == 0
                    || u32::from(left) + u32::from(width) > u32::from(canvas_width)
                    || u32::from(top) + u32::from(height) > u32::from(canvas_height)
                {
                    return None;
                }
                let local = reader.palette(descriptor[8])?;
                let palette = if local.is_empty() { global } else { local };
                if palette.is_empty() {
                    return None;
                }
                let code_size = reader.byte()?;
                if !(2..=8).contains(&code_size) {
                    return None;
                }
                let blocks = reader.blocks()?;
                if blocks.len() < 3 {
                    return None;
                }
                first.get_or_insert(Image {
                    canvas_width,
                    canvas_height,
                    left,
                    top,
                    width,
                    height,
                    interlaced: descriptor[8] & 0x40 != 0,
                    palette,
                    background,
                    transparent,
                    code_size,
                    blocks,
                });
                transparent = None;
            }
            0x3b => return first,
            _ => return None,
        }
    }
}

pub(super) fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let image = first_image(bytes)?;
    Some((
        u32::from(image.canvas_width),
        u32::from(image.canvas_height),
    ))
}

struct Codes<'a> {
    reader: Reader<'a>,
    remaining: usize,
    bits: u32,
    available: u8,
}

impl Codes<'_> {
    fn read(&mut self, width: u8) -> Option<u16> {
        while self.available < width {
            if self.remaining == 0 {
                self.remaining = usize::from(self.reader.byte()?);
                if self.remaining == 0 {
                    return None;
                }
            }
            self.bits |= u32::from(self.reader.byte()?) << self.available;
            self.available += 8;
            self.remaining -= 1;
        }
        let code = (self.bits & ((1 << width) - 1)) as u16;
        self.bits >>= width;
        self.available -= width;
        Some(code)
    }
}

/// Decode directly into the canvas: no concatenated compressed-data buffer or
/// frame-sized index buffer. Dictionary storage is fixed at 4096 entries, and
/// every expanded string is checked against the remaining pixel count.
fn decode_lzw(
    blocks: &[u8],
    minimum: u8,
    pixel_count: usize,
    mut pixel: impl FnMut(u8) -> Option<()>,
) -> Option<()> {
    let mut codes = Codes {
        reader: Reader {
            bytes: blocks,
            pos: 0,
        },
        remaining: 0,
        bits: 0,
        available: 0,
    };
    let clear = 1u16 << minimum;
    let end = clear + 1;
    let mut next = clear + 2;
    let mut width = minimum + 1;
    let mut prefixes = [0u16; 4096];
    let mut suffixes = [0u8; 4096];
    let mut stack = [0u8; 4096];
    let mut previous = None;
    let mut previous_first = 0;
    let mut written = 0usize;
    loop {
        let code = codes.read(width)?;
        if code == clear {
            next = clear + 2;
            width = minimum + 1;
            previous = None;
            continue;
        }
        if code == end {
            return (written == pixel_count).then_some(());
        }
        if code > next {
            return None;
        }
        let mut current = code;
        let mut count = 0usize;
        if code == next {
            current = previous?;
            stack[count] = previous_first;
            count += 1;
        }
        while current >= clear {
            if current <= end || current >= next || count >= stack.len() - 1 {
                return None;
            }
            stack[count] = suffixes[usize::from(current)];
            count += 1;
            current = prefixes[usize::from(current)];
        }
        let first = current as u8;
        stack[count] = first;
        count += 1;
        if count > pixel_count.saturating_sub(written) {
            return None;
        }
        for &index in stack[..count].iter().rev() {
            pixel(index)?;
        }
        written += count;
        if let Some(previous) = previous
            && next < 4096
        {
            prefixes[usize::from(next)] = previous;
            suffixes[usize::from(next)] = first;
            next += 1;
            if next == 1 << width && width < 12 {
                width += 1;
            }
        }
        previous = Some(code);
        previous_first = first;
    }
}

fn plane(len: usize, fill: u8) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).ok()?;
    bytes.resize(len, fill);
    Some(bytes)
}

pub(super) fn decode_first_frame(bytes: &[u8]) -> Option<Frame> {
    let image = first_image(bytes)?;
    let width = usize::from(image.canvas_width);
    let height = usize::from(image.canvas_height);
    let count = width.checked_mul(height)?;
    let mut rgb = plane(count.checked_mul(3)?, 0)?;
    // As in browser GIF compositing, a transparent first frame starts on a
    // transparent canvas. Without a transparent index the global background
    // fills the uncovered logical screen; a local-only palette has no such
    // background, so uncovered pixels stay transparent.
    let background = image.background.filter(|_| image.transparent.is_none());
    if let Some(background) = background {
        for pixel in rgb.chunks_exact_mut(3) {
            pixel.copy_from_slice(background);
        }
    }
    let covers_canvas = image.width == image.canvas_width && image.height == image.canvas_height;
    let mut alpha = if image.transparent.is_some() || (!covers_canvas && background.is_none()) {
        Some(plane(count, 0)?)
    } else {
        None
    };
    let mut row = 0usize;
    let mut column = 0usize;
    let mut pass = 0usize;
    let starts = [0, 4, 2, 1];
    let steps = [8, 8, 4, 2];
    decode_lzw(
        image.blocks,
        image.code_size,
        usize::from(image.width) * usize::from(image.height),
        |index| {
            let at = (usize::from(image.top) + row) * width + usize::from(image.left) + column;
            if image.transparent != Some(index) {
                let offset = usize::from(index) * 3;
                rgb.get_mut(at * 3..at * 3 + 3)?
                    .copy_from_slice(image.palette.get(offset..offset + 3)?);
                if let Some(alpha) = &mut alpha {
                    *alpha.get_mut(at)? = 255;
                }
            }
            column += 1;
            if column == usize::from(image.width) {
                column = 0;
                if image.interlaced {
                    row += steps[pass];
                    while row >= usize::from(image.height) && pass < 3 {
                        pass += 1;
                        row = starts[pass];
                    }
                } else {
                    row += 1;
                }
            }
            Some(())
        },
    )?;
    if alpha
        .as_ref()
        .is_some_and(|plane| plane.iter().all(|&value| value == 255))
    {
        alpha = None;
    }
    Some(Frame {
        width: u32::from(image.canvas_width),
        height: u32::from(image.canvas_height),
        rgb,
        alpha,
    })
}

#[cfg(test)]
#[path = "gif_tests.rs"]
mod tests;
