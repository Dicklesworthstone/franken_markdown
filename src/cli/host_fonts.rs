//! Shared native font admission and explicit collection-face selection.
//!
//! Renderer options still contain standalone SFNT bytes. Collections are
//! resolved once at the filesystem boundary, before ordinary asset validation,
//! weight instancing, subsetting, or embedding in any publication format.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{FontAssetSlot, FontAssets};

#[derive(Debug)]
pub(crate) enum HostFontError {
    Usage(String),
    Input(String),
}

#[derive(Debug)]
struct FontInput {
    slot: FontAssetSlot,
    path: PathBuf,
    face: Option<u32>,
    effective: bool,
}

/// Parsed options contain no file bytes and perform no filesystem access.
#[derive(Debug)]
pub(crate) struct HostFonts {
    fonts: Vec<FontInput>,
    weights: Vec<(FontAssetSlot, u16)>,
}

impl HostFonts {
    /// Validate selectors before opening any mapped font. Repeated font
    /// mappings retain their existing order: later assignments win, and every
    /// file read remains an input protected against output overwrite.
    pub(crate) fn parse(
        fonts: &[String],
        weights: &[String],
        faces: &[String],
    ) -> Result<Self, String> {
        let mut fonts = fonts
            .iter()
            .map(|spec| {
                let (slot, path) = parse_font(spec)?;
                Ok(FontInput {
                    slot,
                    path,
                    face: None,
                    effective: false,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut seen = Vec::new();
        for font in fonts.iter_mut().rev() {
            if !seen.contains(&font.slot) {
                font.effective = true;
                seen.push(font.slot);
            }
        }
        let weights = weights
            .iter()
            .map(|spec| parse_weight(spec))
            .collect::<Result<Vec<_>, _>>()?;
        let mut selected = Vec::new();
        for spec in faces {
            let (slot, index) = parse_face(spec)?;
            if selected.contains(&slot) {
                return Err(format!(
                    "duplicate --pdf-font-face for {}; select one face per slot",
                    slot.as_str()
                ));
            }
            if !fonts.iter().any(|font| font.slot == slot) {
                return Err(format!(
                    "--pdf-font-face for {} requires a mapped --pdf-font {}=PATH",
                    slot.as_str(),
                    slot.as_str()
                ));
            }
            for font in fonts.iter_mut().filter(|font| font.slot == slot) {
                font.face = Some(index);
            }
            selected.push(slot);
        }
        Ok(Self { fonts, weights })
    }

    /// Preserve original paths, including missing files, for watch discovery.
    pub(crate) fn paths(&self) -> impl Iterator<Item = &Path> {
        self.fonts.iter().map(|font| font.path.as_path())
    }

    pub(crate) fn load(&self, input_paths: &mut Vec<PathBuf>) -> Result<FontAssets, HostFontError> {
        let mut assets = FontAssets::default();
        let limit = crate::MAX_FONT_ASSET_BYTES as u64;
        for font in &self.fonts {
            let label = format!("{} font from {}", font.slot.as_str(), font.path.display());
            let metadata = std::fs::metadata(&font.path)
                .map_err(|error| HostFontError::Input(format!("reading {label}: {error}")))?;
            if !metadata.is_file() || metadata.len() > limit {
                return Err(HostFontError::Input(format!(
                    "{label} must be a regular file of at most {limit} bytes"
                )));
            }
            let canonical = font
                .path
                .canonicalize()
                .map_err(|error| HostFontError::Input(format!("resolving {label}: {error}")))?;
            let mut bytes = Vec::new();
            std::fs::File::open(&font.path)
                .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
                .map_err(|error| HostFontError::Input(format!("reading {label}: {error}")))?;
            if bytes.len() as u64 > limit {
                return Err(HostFontError::Input(format!(
                    "{label} exceeds the {limit}-byte font-asset limit"
                )));
            }
            let bytes = select_face(bytes, font.face, font.effective, font.slot, &label)?;
            assets
                .set_slot(font.slot, bytes)
                .map_err(|error| HostFontError::Input(format!("{label}: {error}")))?;
            input_paths.push(canonical);
        }
        for &(slot, weight) in &self.weights {
            assets
                .set_slot_weight(slot, weight)
                .map_err(|error| HostFontError::Usage(error.to_string()))?;
        }
        Ok(assets)
    }
}

fn select_face(
    bytes: Vec<u8>,
    face: Option<u32>,
    effective: bool,
    slot: FontAssetSlot,
    label: &str,
) -> Result<Vec<u8>, HostFontError> {
    if bytes.starts_with(b"ttcf") {
        let index = face.ok_or_else(|| {
            HostFontError::Usage(format!(
                "{label} is a TTC/OTC collection; select a zero-based face with --pdf-font-face {}=INDEX",
                slot.as_str()
            ))
        })?;
        crate::text::extract_collection_face(&bytes, index).map_err(|error| {
            let message = format!("selecting face {index} from {label}: {error}");
            if error.kind == crate::text::CollectionErrorKind::InvalidFaceIndex {
                HostFontError::Usage(message)
            } else {
                HostFontError::Input(message)
            }
        })
    } else if face.is_some() && effective {
        Err(HostFontError::Usage(format!(
            "{label} is not a TTC/OTC collection; remove --pdf-font-face for {}",
            slot.as_str()
        )))
    } else {
        Ok(bytes)
    }
}

fn parse_font(spec: &str) -> Result<(FontAssetSlot, PathBuf), String> {
    let (slot, path) = spec.split_once('=').ok_or_else(|| {
        format!(
            "invalid --pdf-font {spec:?}; expected SLOT=PATH, for example --pdf-font body-regular=./Body.ttf"
        )
    })?;
    let slot = parse_slot(slot, "--pdf-font")?;
    let path = path.trim();
    if path.is_empty() {
        return Err("invalid --pdf-font: PATH must not be blank".to_owned());
    }
    Ok((slot, PathBuf::from(path)))
}

fn parse_slot(value: &str, flag: &str) -> Result<FontAssetSlot, String> {
    FontAssetSlot::parse(value).ok_or_else(|| {
        format!(
            "unknown {flag} slot {:?}; use body-regular, body-bold, body-italic, body-bold-italic, or mono-regular",
            value.trim()
        )
    })
}

fn slot_value<'a>(spec: &'a str, flag: &str) -> Result<(FontAssetSlot, &'a str), String> {
    match spec.trim().split_once('=') {
        Some((slot, value)) => Ok((parse_slot(slot, flag)?, value.trim())),
        None => Ok((FontAssetSlot::BodyRegular, spec.trim())),
    }
}

fn parse_weight(spec: &str) -> Result<(FontAssetSlot, u16), String> {
    let (slot, value) = slot_value(spec, "--pdf-font-weight")?;
    let weight = value
        .parse::<u16>()
        .ok()
        .filter(|weight| (1..=1000).contains(weight))
        .ok_or_else(|| format!("invalid --pdf-font-weight {value:?}; expected integer 1..=1000"))?;
    Ok((slot, weight))
}

fn parse_face(spec: &str) -> Result<(FontAssetSlot, u32), String> {
    let (slot, value) = slot_value(spec, "--pdf-font-face")?;
    let index = value.parse::<u32>().map_err(|_| {
        format!("invalid --pdf-font-face {value:?}; expected a zero-based face index")
    })?;
    Ok((slot, index))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn selectors_are_admitted_without_reading_mapped_files() {
        let fonts = vec!["body-regular=missing.ttc".into()];
        for faces in [
            vec!["1".into(), "regular=1".into()],
            vec!["mono=0".into()],
            vec!["-1".into()],
            vec!["4294967296".into()],
            vec!["0.5".into()],
        ] {
            assert!(HostFonts::parse(&fonts, &[], &faces).is_err());
        }
        let selected = HostFonts::parse(&fonts, &["650".into()], &["1".into()]).unwrap();
        assert_eq!(selected.fonts[0].face, Some(1));
        assert_eq!(selected.weights, [(FontAssetSlot::BodyRegular, 650)]);
        assert_eq!(
            selected.paths().collect::<Vec<_>>(),
            [Path::new("missing.ttc")]
        );
    }

    #[test]
    fn replacement_mappings_retain_every_input_and_one_slot_selector() {
        let selected = HostFonts::parse(
            &["regular=first.ttc".into(), "body-regular=second.ttc".into()],
            &[],
            &["body_regular=2".into()],
        )
        .unwrap();
        assert_eq!(
            selected.paths().collect::<Vec<_>>(),
            [Path::new("first.ttc"), Path::new("second.ttc")]
        );
        assert!(selected.fonts.iter().all(|font| font.face == Some(2)));
    }
}
