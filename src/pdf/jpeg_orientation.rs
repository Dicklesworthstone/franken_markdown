//! Bounded EXIF IFD0 orientation, shared by PDF painting and SVG measurement.
//!
//! Metadata never authorizes an external read or changes the JPEG samples.
//! Invalid, ambiguous or oversized IFDs are ignored. Only the first IFD is
//! inspected: following thumbnail or maker-note offsets would not help orient
//! the primary image and would introduce unnecessary graph traversal.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Orientation {
    #[default]
    Normal,
    FlipHorizontal,
    Rotate180,
    FlipVertical,
    Transpose,
    Rotate90,
    Transverse,
    Rotate270,
}

impl Orientation {
    /// An EXIF APP1 payload (including its six-byte signature).
    pub(super) fn from_exif(bytes: &[u8]) -> Option<Self> {
        let tiff = bytes.strip_prefix(b"Exif\0\0")?;
        let little = match tiff.get(..2)? {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let short = |offset: usize| -> Option<u16> {
            let pair: [u8; 2] = tiff.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
            Some(if little {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            })
        };
        let long = |offset: usize| -> Option<u32> {
            let word: [u8; 4] = tiff.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
            Some(if little {
                u32::from_le_bytes(word)
            } else {
                u32::from_be_bytes(word)
            })
        };
        if short(2)? != 42 {
            return None;
        }
        let offset = usize::try_from(long(4)?).ok()?;
        if offset < 8 {
            return None;
        }
        let count = usize::from(short(offset)?);
        // A normal camera IFD has tens of entries. This fixed bound also
        // applies to direct callers, independent of JPEG's APP1 length limit.
        if count > 4096 {
            return None;
        }
        let start = offset.checked_add(2)?;
        let end = start.checked_add(count.checked_mul(12)?)?;
        // Require the complete table and its next-IFD pointer, but do not
        // follow it (it describes a thumbnail, not the primary image).
        tiff.get(start..end.checked_add(4)?)?;
        let mut orientation = None;
        for index in 0..count {
            let entry = start + index * 12;
            if short(entry)? != 0x0112 {
                continue;
            }
            if orientation.is_some() || short(entry + 2)? != 3 || long(entry + 4)? != 1 {
                return None;
            }
            orientation = Some(match short(entry + 8)? {
                1 => Self::Normal,
                2 => Self::FlipHorizontal,
                3 => Self::Rotate180,
                4 => Self::FlipVertical,
                5 => Self::Transpose,
                6 => Self::Rotate90,
                7 => Self::Transverse,
                8 => Self::Rotate270,
                _ => return None,
            });
        }
        orientation
    }

    /// Scan only JPEG marker metadata, never entropy-coded data. The first
    /// valid EXIF orientation wins; malformed metadata is inert.
    pub(super) fn from_jpeg(bytes: &[u8]) -> Self {
        if !bytes.starts_with(&[0xff, 0xd8]) {
            return Self::Normal;
        }
        let mut offset = 2usize;
        while bytes.get(offset) == Some(&0xff) {
            while bytes.get(offset) == Some(&0xff) {
                offset += 1;
            }
            let Some(&marker) = bytes.get(offset) else {
                break;
            };
            offset += 1;
            if matches!(marker, 0x00 | 0xd8 | 0xd9 | 0xda) {
                break;
            }
            if matches!(marker, 0x01 | 0xd0..=0xd7) {
                continue;
            }
            let Some(length) = bytes.get(offset..offset + 2) else {
                break;
            };
            let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
            if length < 2 {
                break;
            }
            let Some(end) = offset.checked_add(length) else {
                break;
            };
            let Some(segment) = bytes.get(offset + 2..end) else {
                break;
            };
            if marker == 0xe1
                && let Some(orientation) = Self::from_exif(segment)
            {
                return orientation;
            }
            offset = end;
        }
        Self::Normal
    }

    pub(super) fn display_dimensions(self, width: u32, height: u32) -> (u32, u32) {
        if matches!(
            self,
            Self::Transpose | Self::Rotate90 | Self::Transverse | Self::Rotate270
        ) {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// Affine transform inside a unit square whose origin is bottom left,
    /// matching PDF image space. Layout has already exchanged displayed axes
    /// for orientations 5 through 8; the encoded sample dimensions stay intact.
    #[allow(dead_code)] // The SVG measurement copy does not paint PDF images.
    pub(super) const fn matrix(self) -> [f32; 6] {
        match self {
            Self::Normal => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            Self::FlipHorizontal => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0],
            Self::Rotate180 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],
            Self::FlipVertical => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],
            Self::Transpose => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0],
            Self::Rotate90 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],
            Self::Transverse => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
            Self::Rotate270 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::Orientation;

    fn exif(value: u16, little: bool) -> Vec<u8> {
        let short = |n: u16| {
            if little {
                n.to_le_bytes()
            } else {
                n.to_be_bytes()
            }
        };
        let long = |n: u32| {
            if little {
                n.to_le_bytes()
            } else {
                n.to_be_bytes()
            }
        };
        let mut bytes = b"Exif\0\0".to_vec();
        bytes.extend_from_slice(if little { b"II" } else { b"MM" });
        bytes.extend_from_slice(&short(42));
        bytes.extend_from_slice(&long(8));
        bytes.extend_from_slice(&short(1));
        bytes.extend_from_slice(&short(0x0112));
        bytes.extend_from_slice(&short(3));
        bytes.extend_from_slice(&long(1));
        bytes.extend_from_slice(&short(value));
        bytes.extend_from_slice(&short(0));
        bytes.extend_from_slice(&long(0));
        bytes
    }

    #[test]
    fn both_tiff_orders_cover_all_orientations() {
        let expected = [
            Orientation::Normal,
            Orientation::FlipHorizontal,
            Orientation::Rotate180,
            Orientation::FlipVertical,
            Orientation::Transpose,
            Orientation::Rotate90,
            Orientation::Transverse,
            Orientation::Rotate270,
        ];
        for little in [false, true] {
            for (index, expected) in expected.into_iter().enumerate() {
                assert_eq!(
                    Orientation::from_exif(&exif(index as u16 + 1, little)),
                    Some(expected)
                );
                assert_eq!(
                    expected.display_dimensions(80, 40),
                    if index < 4 { (80, 40) } else { (40, 80) }
                );
            }
        }
    }

    #[test]
    fn malformed_metadata_is_ignored_without_offset_or_count_traversal() {
        let good = exif(6, true);
        for length in 0..good.len() {
            assert_eq!(Orientation::from_exif(&good[..length]), None);
        }
        for value in [0, 9, u16::MAX] {
            assert_eq!(Orientation::from_exif(&exif(value, true)), None);
        }
        for (at, data) in [
            (6, vec![b'X', b'X']),
            (8, vec![43, 0]),
            (10, vec![0xff; 4]),
            (10, vec![0; 4]),
            (14, vec![0xff; 2]),
            (18, vec![4, 0]),
            (20, vec![2, 0, 0, 0]),
        ] {
            let mut invalid = good.clone();
            invalid[at..at + data.len()].copy_from_slice(&data);
            assert_eq!(Orientation::from_exif(&invalid), None, "offset {at}");
        }
        let mut duplicate = good[..28].to_vec();
        duplicate[14..16].copy_from_slice(&2u16.to_le_bytes());
        duplicate.extend_from_slice(&good[16..28]);
        duplicate.extend_from_slice(&[0; 4]);
        assert_eq!(Orientation::from_exif(&duplicate), None);
        // A complete, oversized table is refused as well as a truncated one.
        let mut excessive = good[..16].to_vec();
        excessive[14..16].copy_from_slice(&4097u16.to_le_bytes());
        excessive.resize(16 + 4097 * 12 + 4, 0);
        excessive[16..28].copy_from_slice(&good[16..28]);
        assert_eq!(Orientation::from_exif(&excessive), None);
    }

    #[test]
    fn jpeg_scan_skips_other_metadata_and_never_reads_past_sos() {
        let exif = exif(6, false);
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xe1];
        jpeg.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(&exif);
        jpeg.extend_from_slice(&[0xff, 0xda]);
        assert_eq!(Orientation::from_jpeg(&jpeg), Orientation::Rotate90);
        for length in 0..jpeg.len() - 2 {
            assert_eq!(Orientation::from_jpeg(&jpeg[..length]), Orientation::Normal);
        }
        jpeg.splice(2..2, [0xff, 0xda]);
        assert_eq!(Orientation::from_jpeg(&jpeg), Orientation::Normal);
    }
}
