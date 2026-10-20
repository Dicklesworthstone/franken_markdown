#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::book::{BookInput, BookRenderer};
use crate::{PdfImageAsset, PdfRunningImage, PdfRunningImagePosition};

const TOKENS: &str = "homogeneous,antiriver,pareto,optimal-pagination,expansion";
const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;

fn sources() -> (Vec<String>, Vec<String>) {
    let prose = "A carefully prepared publication keeps the original words while making each line \
        comfortable to read. Decimal quantities, figures, links and repeated page furniture must \
        survive the same publication path, even when the author selects a different typesetting algorithm. ";
    (
        vec!["guide/one.md".into(), "two.md".into()],
        vec![
            format!(
                "# One\n\n[Next](../two.md#two)\n\n![Chart](chart.svg)\n\n{}",
                prose.repeat(12)
            ),
            "# Two\n\n[Previous](guide/one.md#one)\n\nSecond chapter.".into(),
        ],
    )
}

fn geometry() -> Vec<f64> {
    vec![512.0, 420.0, 60.0, 36.0, 60.0, 36.0]
}

fn render(typography: bool, running: bool, skip: bool) -> FmdRenderResult {
    let (paths, sources) = sources();
    let slots = if running {
        ["{title}", "", "{author}", "", "{page} / {pages}", "{date}"]
            .map(String::from)
            .to_vec()
    } else {
        vec![]
    };
    render_book_pdf_configured_advanced(
        paths,
        sources,
        Some("serif".into()),
        None,
        Some("Manual".into()),
        Some("Writer".into()),
        Some(0.0),
        false,
        false,
        vec!["guide/chart.svg".into(), "brand.svg".into()],
        [SVG, SVG].concat(),
        vec![SVG.len() as u32; 2],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        None,
        None,
        None,
        true,
        None,
        Some("en".into()),
        true,
        Some(2),
        None,
        true,
        geometry(),
        slots,
        running,
        running,
        skip,
        if running {
            vec!["brand.svg".into(), String::new()]
        } else {
            vec![]
        },
        if running { vec![1, 0] } else { vec![] },
        if running { vec![20, 0] } else { vec![] },
        typography.then(|| TOKENS.into()),
    )
    .unwrap()
}

fn native(typography: bool, running: bool, skip: bool) -> BookRenderer {
    let (paths, sources) = sources();
    let inputs: Vec<_> = paths
        .into_iter()
        .zip(sources)
        .map(|(path, source)| BookInput { path, source })
        .collect();
    let mut renderer = BookRenderer::from_sources(&inputs, &[]).unwrap();
    let opts = renderer.options_mut();
    opts.theme = crate::Theme::serif();
    opts.theme.page = crate::PageStyle::from_browser_geometry(&geometry())
        .unwrap()
        .unwrap();
    opts.title = Some("Manual".into());
    opts.author = Some("Writer".into());
    opts.metadata_epoch_seconds = Some(0);
    opts.page_numbers = true;
    opts.lang = Some("en".into());
    opts.toc = true;
    opts.toc_depth = Some(2);
    opts.microtype = crate::layout::MicrotypeOptions::CONSERVATIVE;
    opts.pdf_image_assets = ["guide/chart.svg", "brand.svg"]
        .map(|destination| PdfImageAsset {
            destination: destination.into(),
            bytes: SVG.to_vec(),
        })
        .to_vec();
    if typography {
        // Independent public options are the oracle, not the token helper.
        opts.gradual_demerits = true;
        opts.river_penalty = true;
        opts.pareto_line_breaking = true;
        opts.optimal_pagination = true;
        opts.microtype = crate::layout::MicrotypeOptions {
            protrusion: false,
            max_expansion_per_mille: 15,
        };
    }
    if running {
        opts.running.header.left = Some("{title}".into());
        opts.running.header.right = Some("{author}".into());
        opts.running.header.rule = true;
        opts.running.header.image = Some(PdfRunningImage {
            dest: "brand.svg".into(),
            position: PdfRunningImagePosition::Right,
            height_pt: Some(20),
        });
        opts.running.footer.center = Some("{page} / {pages}".into());
        opts.running.footer.right = Some("{date}".into());
        opts.running.footer.rule = true;
    }
    opts.running.skip_first_page = skip;
    renderer
}

#[test]
fn advanced_book_abi_matches_native_typography_chapter_links_and_host_only_logo() {
    for (typography, running, skip) in [
        (true, false, false),
        (true, true, false),
        (true, true, true),
    ] {
        let actual = render(typography, running, skip);
        let expected = native(typography, running, skip);
        assert_eq!(actual.bytes(), expected.render_pdf().unwrap());
        assert_eq!(actual.format(), "book-pdf");
        assert_eq!(actual.source_length(), expected.source_length());
        assert!(String::from_utf8_lossy(&actual.bytes()).contains("/MediaBox [0 0 512 420]"));
    }
    let plain = render(false, false, false).bytes();
    let typeset = render(true, false, false).bytes();
    assert_ne!(
        typeset, plain,
        "the requested algorithms must affect this long-prose witness"
    );
    let branded = render(true, true, false).bytes();
    assert_ne!(
        branded, typeset,
        "host-only logo and running bands must be drawn"
    );
    assert_ne!(
        branded,
        render(true, true, true).bytes(),
        "skip-first must affect the actual book"
    );
}

#[test]
fn advanced_book_empty_presentation_preserves_configured_page_bytes() {
    let (paths, sources) = sources();
    let old = super::super::render_book_pdf_configured_page(
        paths,
        sources,
        Some("serif".into()),
        None,
        Some("Manual".into()),
        Some("Writer".into()),
        Some(0.0),
        false,
        false,
        vec!["guide/chart.svg".into(), "brand.svg".into()],
        [SVG, SVG].concat(),
        vec![SVG.len() as u32; 2],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        None,
        None,
        None,
        true,
        None,
        Some("en".into()),
        true,
        Some(2),
        None,
        true,
        geometry(),
    )
    .unwrap();
    assert_eq!(render(false, false, false).bytes(), old.bytes());
}

#[test]
fn advanced_book_presentation_rejects_invalid_tokens_slots_and_images_before_rendering() {
    let empty = || (vec![], vec![], vec![]);
    for typography in ["unknown", "pareto,unknown", &" ".repeat(257)] {
        let (dest, pos, height) = empty();
        assert!(
            book_presentation(
                Some(typography),
                vec![],
                false,
                false,
                false,
                dest,
                pos,
                height
            )
            .is_err()
        );
    }
    for slots in [vec![String::new(); 5], vec!["é".repeat(2049); 6]] {
        let (dest, pos, height) = empty();
        assert!(book_presentation(None, slots, false, false, false, dest, pos, height).is_err());
    }
    for (dest, pos, height) in [
        (vec!["logo".into()], vec![0], vec![20]),
        (vec!["logo".into(), String::new()], vec![2, 0], vec![20, 0]),
        (
            vec!["logo".into(), String::new()],
            vec![0, 0],
            vec![65536, 0],
        ),
        (vec![String::new(), String::new()], vec![1, 0], vec![0, 0]),
    ] {
        assert!(book_presentation(None, vec![], false, false, false, dest, pos, height).is_err());
    }
    let running = book_presentation(
        Some(TOKENS),
        vec![String::new(); 6],
        false,
        true,
        true,
        vec!["logo".into(), String::new()],
        vec![1, 0],
        vec![20, 0],
    )
    .unwrap();
    assert!(running.footer.rule && running.skip_first_page);
    assert_eq!(running.header.image.unwrap().dest, "logo");
}
