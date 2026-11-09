//! Browser font preparation uses the native bounded extractor, then the
//! ordinary host-font renderer. These fixtures contain shared TrueType/CFF
//! tables; the expected standalone programs were generated independently.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::CollectionErrorKind;
use franken_markdown::wasm::{self, WasmRenderOptions};
use franken_markdown::{FontAssetSlot, text};

const V1: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdMixedFacesV1.ttc");
const V2: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdMixedFacesV2.otc");
const TTF: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdCollectionTrueType.ttf");
const CFF: &[u8] = include_bytes!("../fmd-font/fonts/test-cid-cff/FmdCidFormat3.otf");

#[test]
fn browser_preparation_recovers_both_real_shared_table_faces_deterministically() {
    for collection in [V1, V2] {
        let original = collection.to_vec();
        assert_eq!(wasm::font_collection_face_count(collection).unwrap(), 2);
        for (index, expected) in [TTF, CFF].into_iter().enumerate() {
            let mut face = wasm::extract_font_collection_face(collection, index as u32).unwrap();
            assert_eq!(face, expected);
            let parsed = text::Font::parse(face.clone()).unwrap();
            assert_eq!(parsed.glyph_index('A'), 1);
            assert_eq!(parsed.glyph_index('日'), 3);
            assert_eq!(
                parsed.glyph_bbox(1),
                Some([100, 0, 300, if index == 0 { 450 } else { 300 }])
            );
            face.fill(0);
            assert_eq!(collection, original);
            assert_eq!(
                wasm::extract_font_collection_face(collection, index as u32).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn extracted_faces_use_existing_browser_font_slots_and_output_paths() {
    for (index, expected) in [TTF, CFF].into_iter().enumerate() {
        let face = wasm::extract_font_collection_face(V2, index as u32).unwrap();
        let selected = WasmRenderOptions::default()
            .with_font_asset_bytes(FontAssetSlot::BodyRegular, face)
            .unwrap();
        let standalone = WasmRenderOptions::default()
            .with_font_asset_bytes(FontAssetSlot::BodyRegular, expected)
            .unwrap();
        let source = "A B 日";
        assert_eq!(
            wasm::render_html(source, &selected).unwrap().bytes,
            wasm::render_html(source, &standalone).unwrap().bytes
        );
        assert_eq!(
            wasm::render_pdf(source, &selected).unwrap().bytes,
            wasm::render_pdf(source, &standalone).unwrap().bytes
        );
    }
}

#[test]
fn browser_preparation_retains_native_typed_failures_without_fallback() {
    for source in [TTF, CFF, &[][..], &b"ttcf"[..]] {
        let expected = text::collection_face_count(source).unwrap_err();
        assert_eq!(
            wasm::font_collection_face_count(source).unwrap_err(),
            expected
        );
    }
    let invalid = wasm::extract_font_collection_face(V1, 2).unwrap_err();
    assert_eq!(invalid.kind, CollectionErrorKind::InvalidFaceIndex);
    assert_eq!(invalid.face_index, Some(2));
    let mut broken = V2.to_vec();
    broken[12..16].copy_from_slice(&0u32.to_be_bytes());
    assert_eq!(
        wasm::font_collection_face_count(&broken).unwrap_err().kind,
        CollectionErrorKind::Malformed
    );
    assert_eq!(
        wasm::extract_font_collection_face(&broken, 1)
            .unwrap_err()
            .kind,
        CollectionErrorKind::Malformed
    );
}

#[cfg(feature = "wasm-bindgen")]
#[test]
fn javascript_abi_exports_execute_the_same_collection_extractor() {
    use franken_markdown::wasm_abi;
    for collection in [V1, V2] {
        assert_eq!(wasm_abi::font_collection_face_count(collection).unwrap(), 2);
        assert_eq!(
            wasm_abi::extract_font_collection_face(collection, None).unwrap(),
            TTF
        );
        assert_eq!(
            wasm_abi::extract_font_collection_face(collection, Some(1.0)).unwrap(),
            CFF
        );
    }
}
