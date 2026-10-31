//! Native execution of the browser adapter, including the actual font payload.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

const FONT: &[u8] =
    include_bytes!("../../tests/fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");
const SOURCE: &str = "# Typography\n\noffice AVATAR z\u{301}\u{307}.\n";

fn html(policy: Option<&str>) -> FmdRenderResult {
    render_html_configured_advanced(
        SOURCE,
        None,
        None,
        None,
        None,
        false,
        None,
        FONT.to_vec(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        false,
        None,
        Some("ttf".into()),
        policy.map(str::to_owned),
    )
    .unwrap()
}

fn epub(policy: Option<&str>, supplied: bool) -> FmdRenderResult {
    render_epub_configured_advanced(
        SOURCE,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        if supplied { FONT.to_vec() } else { Vec::new() },
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        policy.map(str::to_owned),
    )
    .unwrap()
}

#[test]
fn html_binding_embeds_complete_cff_font_and_preserves_subset_default() {
    let compact = html(None).bytes();
    assert_eq!(compact, html(Some("subset")).bytes());
    let full = html(Some("full")).bytes();
    let encoded = crate::html::base64_encode(FONT);
    let payload = format!("data:font/otf;base64,{encoded}");
    assert!(std::str::from_utf8(&full).unwrap().contains(&payload));
    assert!(!std::str::from_utf8(&compact).unwrap().contains(&payload));
    assert_eq!(full, html(Some(" FULL ")).bytes());
}

#[test]
fn epub_binding_keeps_exact_host_font_and_full_can_embed_bundled_faces() {
    assert_eq!(epub(None, true).bytes(), epub(Some("subset"), true).bytes());
    for supplied in [false, true] {
        let full = epub(Some("full"), supplied).bytes();
        let mut options = WasmRenderOptions::default();
        options.font_embedding = crate::FontEmbedding::Full;
        if supplied {
            options
                .font_assets
                .set_slot(FontAssetSlot::BodyRegular, FONT.to_vec())
                .unwrap();
        }
        assert_eq!(
            full,
            crate::render_epub(&crate::parse_markdown(SOURCE), &options.html_options()).unwrap()
        );
        assert!(
            full.windows(b"OEBPS/fonts/".len())
                .any(|window| window == b"OEBPS/fonts/")
        );
        assert_ne!(full, epub(None, supplied).bytes());
        if supplied {
            assert_epub_font(&full, FONT);
        }
    }
}

fn assert_epub_font(archive: &[u8], expected: &[u8]) {
    let u16_at = |at| u16::from_le_bytes(archive[at..at + 2].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(archive[at..at + 4].try_into().unwrap());
    let mut at = 0;
    while archive.get(at..at + 4) == Some(b"PK\x03\x04") {
        let name_end = at + 30 + usize::from(u16_at(at + 26));
        let start = name_end + usize::from(u16_at(at + 28));
        let end = start + u32_at(at + 18) as usize;
        if &archive[at + 30..name_end] == b"OEBPS/fonts/font-1.otf" {
            assert_eq!(u32_at(at + 22) as usize, expected.len());
            assert_eq!(u32_at(at + 14), crate::crc32(expected));
            if u16_at(at + 8) == 0 {
                assert_eq!(&archive[start..end], expected);
            } else {
                assert_eq!(u16_at(at + 8), 8);
                let (mut a, mut b) = (1u32, 0u32);
                for byte in expected {
                    a = (a + u32::from(*byte)) % 65521;
                    b = (b + a) % 65521;
                }
                let mut zlib = vec![0x78, 0x9c];
                zlib.extend_from_slice(&archive[start..end]);
                zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
                assert_eq!(
                    crate::zlib_decompress(&zlib, expected.len()).unwrap(),
                    expected
                );
            }
            return;
        }
        at = end;
    }
    panic!("EPUB did not contain the supplied complete OpenType font");
}
