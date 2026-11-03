//! Bounded colour admission for four-component JPEG streams.
//!
//! PDF's DCTDecode consumes the original JPEG bytes, including Adobe APP14.
//! ISO 32000-1, 7.4.8/Table 13 gives that marker precedence over ColorTransform.
//! Bare Adobe CMYK (transform 0) and YCCK (transform 2) store inverted samples;
//! the image Decode array restores ordinary DeviceCMYK ink concentrations.
//! Missing or contradictory colour metadata is not enough to infer polarity.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Encoding {
    AdobeCmyk,
    AdobeYcck,
}

impl Encoding {
    /// APP14's YCCK value is 2, but PDF's transform switch is 1. The retained
    /// marker remains authoritative; spell out the matching PDF interpretation.
    pub(super) const fn pdf_parameters(self) -> &'static [u8] {
        match self {
            Self::AdobeCmyk => b" /DecodeParms << /ColorTransform 0 >> /Decode [1 0 1 0 1 0 1 0]",
            Self::AdobeYcck => b" /DecodeParms << /ColorTransform 1 >> /Decode [1 0 1 0 1 0 1 0]",
        }
    }
}

#[derive(Default)]
pub(super) struct Metadata {
    adobe: Option<u8>,
    invalid: bool,
}

impl Metadata {
    /// Consume only metadata whose containing marker length was checked by the
    /// JPEG walker. No allocation, pixel decoding, or scan-data traversal.
    pub(super) fn observe(&mut self, marker: u8, payload: &[u8]) {
        if marker != 0xee || !payload.starts_with(b"Adobe") {
            return;
        }
        let Some(&transform) = payload.get(11) else {
            self.invalid = true;
            return;
        };
        if self.adobe.is_some_and(|previous| previous != transform) {
            self.invalid = true;
        }
        self.adobe = Some(transform);
    }

    pub(super) fn encoding(&self) -> Option<Encoding> {
        // Some real YCCK encoders also leave JFIF metadata in the file. The
        // explicit Adobe transform governs these four components, just as it
        // does in the PDF reader; contradictory Adobe markers remain invalid.
        if self.invalid {
            return None;
        }
        match self.adobe? {
            0 => Some(Encoding::AdobeCmyk),
            2 => Some(Encoding::AdobeYcck),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Frame {
    ids: [u8; 4],
    progressive: bool,
}

impl Frame {
    /// Validate the newly admitted four-component frame, including component
    /// descriptors. Existing grayscale/RGB admission stays on its old path.
    pub(super) fn parse(marker: u8, payload: &[u8]) -> Option<Self> {
        if payload.len() != 18 || payload[5] != 4 {
            return None;
        }
        let mut ids = [0; 4];
        for (index, component) in payload[6..].as_chunks::<3>().0.iter().enumerate() {
            let [id, sampling, table] = *component;
            if ids[..index].contains(&id)
                || !(1..=4).contains(&(sampling >> 4))
                || !(1..=4).contains(&(sampling & 15))
                || table > 3
            {
                return None;
            }
            ids[index] = id;
        }
        Some(Self {
            ids,
            progressive: marker == 0xc2,
        })
    }

    /// Check the first scan's container before trusting its four channels.
    /// JPEG entropy and subsequent progressive scans remain the PDF reader's
    /// responsibility, as for the existing RGB and grayscale passthrough.
    pub(super) fn scan_is_valid(self, bytes: &[u8], length_offset: usize) -> bool {
        self.checked_scan(bytes, length_offset).is_some()
    }

    fn checked_scan(self, bytes: &[u8], length_offset: usize) -> Option<()> {
        let size_bytes = bytes.get(length_offset..length_offset.checked_add(2)?)?;
        let length = usize::from(u16::from_be_bytes(size_bytes.try_into().ok()?));
        let end = length_offset.checked_add(length)?;
        let scan = bytes.get(length_offset.checked_add(2)?..end)?;
        let count = usize::from(*scan.first()?);
        if !(1..=4).contains(&count)
            || scan.len() != 4 + 2 * count
            || !bytes.ends_with(&[0xff, 0xd9])
            || end.checked_add(2)? >= bytes.len()
        {
            return None;
        }
        let mut seen = [false; 4];
        for component in scan[1..1 + 2 * count].as_chunks::<2>().0 {
            let index = self.ids.iter().position(|&id| id == component[0])?;
            let tables = component[1];
            if seen[index] || tables >> 4 > 3 || tables & 15 > 3 {
                return None;
            }
            seen[index] = true;
        }
        let tail = &scan[1 + 2 * count..];
        if self.progressive {
            if tail[0] > tail[1]
                || tail[1] > 63
                || tail[2] >> 4 > 13
                || tail[2] & 15 > 13
                || (tail[0] == 0 && tail[1] != 0)
                || (tail[0] != 0 && count != 1)
            {
                return None;
            }
        } else if tail != [0, 63, 0] {
            return None;
        }
        Some(())
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::parse_jpeg_header;

    #[test]
    fn every_truncated_real_cmyk_container_is_refused_without_panicking() {
        for fixture in [
            include_str!("../../tests/fixtures/jpeg-cmyk-swatches.hex"),
            include_str!("../../tests/fixtures/jpeg-ycck-swatches.hex"),
            include_str!("../../tests/fixtures/jpeg-cmyk-progressive-swatches.hex"),
            include_str!("../../tests/fixtures/jpeg-ycck-progressive-swatches.hex"),
        ] {
            let hex: String = fixture.split_whitespace().collect();
            let bytes: Vec<_> = hex
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            assert!(parse_jpeg_header(&bytes).is_some());
            for end in 0..bytes.len() {
                assert!(parse_jpeg_header(&bytes[..end]).is_none(), "prefix {end}");
            }
        }
    }
}
