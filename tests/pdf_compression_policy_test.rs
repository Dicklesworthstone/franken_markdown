//! Explicit compression scheduling must preserve actual PDF bytes and paint.
//! The oracle follows emitted xref/page/font references rather than layout helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::{
    PdfCompression, PdfEmitOptions, PdfOptions, PdfPageEmission, RenderError, render_pdf,
    render_pdf_emitted,
};

const SMALL: &str = "SMALL CONCURRENT COMPRESSION";

fn options() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

fn emit(compression: PdfCompression, emission: PdfPageEmission) -> PdfEmitOptions {
    PdfEmitOptions {
        compression,
        emission,
        ..PdfEmitOptions::default()
    }
}

struct Objects<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
    xref: usize,
}

impl<'a> Objects<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|part| part == marker)
            .unwrap();
        let xref = std::str::from_utf8(&bytes[at + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(section[0], "0");
        let count: usize = section[1].parse().unwrap();
        let offsets = lines
            .take(count)
            .map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields[2] == "n" {
                    fields[0].parse().unwrap()
                } else {
                    0
                }
            })
            .collect();
        Self {
            bytes,
            offsets,
            xref,
        }
    }

    fn body(&self, id: usize) -> &'a [u8] {
        let start = self.offsets[id];
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&at| at > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        let object = &self.bytes[start..end];
        assert!(
            object.starts_with(header.as_bytes()),
            "xref must point to the actual object"
        );
        &object[header.len()..]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let object = self.body(id);
        let end = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap_or(object.len());
        std::str::from_utf8(&object[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let object = self.body(id);
        let at = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap();
        let dictionary = std::str::from_utf8(&object[..at]).unwrap();
        let length = integer_after(dictionary, "/Length ");
        let payload = &object[at + 8..at + 8 + length];
        let bytes = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(
                !dictionary.contains("/Filter"),
                "extend the decoder for a new stream filter"
            );
            payload.to_vec()
        };
        String::from_utf8(bytes).unwrap()
    }
}

fn integer_after(dictionary: &str, key: &str) -> usize {
    dictionary
        .split_once(key)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn unicode_map(objects: &Objects<'_>, font: usize) -> BTreeMap<u16, String> {
    let cmap = objects.stream(integer_after(objects.dictionary(font), "/ToUnicode "));
    let mut chars = false;
    let mut map = BTreeMap::new();
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            chars = true;
            continue;
        }
        if line == "endbfchar" {
            chars = false;
            continue;
        }
        if !chars {
            continue;
        }
        let entries: Vec<_> = line.split_whitespace().collect();
        let id = u16::from_str_radix(entries[0].trim_matches(['<', '>']), 16).unwrap();
        let encoded = entries[1].trim_matches(['<', '>']);
        let units = encoded
            .as_bytes()
            .chunks_exact(4)
            .map(|part| u16::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        map.insert(id, String::from_utf16(&units).unwrap());
    }
    assert!(
        !map.is_empty(),
        "painted font must have a selectable glyph map"
    );
    map
}

struct Inspection {
    page_text: Vec<String>,
    decoded_bytes: usize,
}

fn inspect(pdf: &[u8]) -> Inspection {
    let objects = Objects::new(pdf);
    let page_tree = (1..objects.offsets.len())
        .find(|&id| objects.dictionary(id).contains("/Type /Pages "))
        .unwrap();
    let kids = objects
        .dictionary(page_tree)
        .split_once("/Kids [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(kids.len() % 3, 0);
    let mut decoded_bytes = 0;
    let mut page_text = Vec::new();
    for kid in kids.chunks_exact(3) {
        assert_eq!(&kid[1..], &["0", "R"]);
        let page = objects.dictionary(kid[0].parse().unwrap());
        let resources = page
            .split_once("/Font <<")
            .unwrap()
            .1
            .split_once(">>")
            .unwrap()
            .0
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(resources.len() % 4, 0);
        let fonts = resources
            .chunks_exact(4)
            .map(|entry| (entry[0], unicode_map(&objects, entry[1].parse().unwrap())))
            .collect::<BTreeMap<_, _>>();
        let content = objects.stream(integer_after(page, "/Contents "));
        decoded_bytes += content.len();
        let mut painted = String::new();
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let separated = line
                .replace('[', " [ ")
                .replace(']', " ] ")
                .replace('<', " <")
                .replace('>', "> ");
            let words: Vec<_> = separated.split_whitespace().collect();
            let mut font = None;
            let mut mode = 0;
            let mut text = String::new();
            for (index, word) in words.iter().enumerate() {
                match *word {
                    "Tf" => font = Some(&fonts[words[index - 2]]),
                    "Tr" => mode = words[index - 1].parse::<u8>().unwrap(),
                    "Tj" | "TJ" => {
                        if mode != 3 {
                            painted.push_str(&text);
                        }
                        text.clear();
                    }
                    hex if hex.starts_with('<') && hex.ends_with('>') => {
                        let font = font.expect("painted glyph needs an active font");
                        let hex = hex.trim_matches(['<', '>']);
                        assert_eq!(hex.len() % 4, 0);
                        for glyph in hex.as_bytes().chunks_exact(4) {
                            let id = u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16)
                                .unwrap();
                            text.push_str(
                                font.get(&id)
                                    .expect("painted glyph must have a Unicode mapping"),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        page_text.push(painted.chars().filter(|c| !c.is_whitespace()).collect());
    }
    Inspection {
        page_text,
        decoded_bytes,
    }
}

#[test]
fn default_policy_is_sequential_and_preserves_the_public_render_output() {
    assert_eq!(
        PdfEmitOptions::default().compression,
        PdfCompression::Sequential
    );
    let opts = options();
    let bytes = render_pdf(SMALL, &opts).unwrap();
    for emission in [PdfPageEmission::Chunked, PdfPageEmission::Monolithic] {
        assert_eq!(
            bytes,
            render_pdf_emitted(SMALL, &opts, emit(PdfCompression::Sequential, emission)).unwrap()
        );
    }
    let actual = inspect(&bytes);
    assert_eq!(actual.page_text, ["SMALLCONCURRENTCOMPRESSION"]);
    assert!(actual.decoded_bytes < 256 * 1024);
}

#[test]
fn invalid_native_worker_budgets_are_explicit_input_errors() {
    for max_workers in [0, 5, usize::MAX] {
        let error = render_pdf_emitted(
            SMALL,
            &options(),
            emit(
                PdfCompression::NativeWorkers { max_workers },
                PdfPageEmission::Chunked,
            ),
        )
        .unwrap_err();
        assert!(
            matches!(error, RenderError::InvalidInput(ref text)
            if text.starts_with("pdf_compression:") && text.contains("1 through 4")),
            "{error}"
        );
    }
}

#[cfg(any(not(feature = "native-compression"), target_arch = "wasm32"))]
#[test]
fn explicit_native_workers_require_the_feature_and_a_native_target() {
    for max_workers in [1, 2, 4] {
        let error = render_pdf_emitted(
            SMALL,
            &options(),
            emit(
                PdfCompression::NativeWorkers { max_workers },
                PdfPageEmission::Chunked,
            ),
        )
        .unwrap_err();
        assert!(
            matches!(error, RenderError::InvalidInput(ref text)
            if text.starts_with("pdf_compression_unavailable:")),
            "{error}"
        );
    }
}

#[cfg(all(feature = "native-compression", not(target_arch = "wasm32")))]
mod native {
    use super::*;

    fn large_fixture(prefix: &str) -> (String, String) {
        let mut source = String::from("```text\n");
        let mut expected = String::new();
        for index in 0..1400 {
            let line = format!(
                "{prefix}{index:04} ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 abcdefghijklmnopqrstuvwxyz\n"
            );
            source.push_str(&line);
            expected.extend(line.chars().filter(|c| !c.is_whitespace()));
        }
        source.push_str("```\n");
        assert!(
            source.len() < 200_000,
            "fixture must stay small enough for ordinary test runs"
        );
        (source, expected)
    }

    fn render(source: &str, policy: PdfCompression, emission: PdfPageEmission) -> Vec<u8> {
        render_pdf_emitted(source, &options(), emit(policy, emission)).unwrap()
    }

    #[test]
    fn small_renders_match_for_every_worker_budget_and_emitter() {
        let baseline = render(SMALL, PdfCompression::Sequential, PdfPageEmission::Chunked);
        for max_workers in [1, 2, 4] {
            for emission in [PdfPageEmission::Chunked, PdfPageEmission::Monolithic] {
                let actual = render(
                    SMALL,
                    PdfCompression::NativeWorkers { max_workers },
                    emission,
                );
                assert_eq!(
                    actual, baseline,
                    "small document: {max_workers} workers, {emission:?}"
                );
            }
        }
    }

    #[test]
    fn large_real_page_streams_preserve_bytes_glyph_order_and_determinism() {
        let (source, expected) = large_fixture("ROW");
        let baseline = render(
            &source,
            PdfCompression::Sequential,
            PdfPageEmission::Chunked,
        );
        let decoded = inspect(&baseline);
        assert!(
            decoded.page_text.len() >= 4,
            "exercise several independently compressed pages"
        );
        assert!(
            decoded.decoded_bytes > 256 * 1024,
            "real emitted page streams must cross the parallel admission threshold: {}",
            decoded.decoded_bytes
        );
        assert_eq!(
            decoded.page_text.concat(),
            expected,
            "every painted source row stays in order"
        );
        assert_eq!(
            baseline,
            render(
                &source,
                PdfCompression::Sequential,
                PdfPageEmission::Monolithic
            )
        );
        for max_workers in [1, 2, 4] {
            for emission in [PdfPageEmission::Chunked, PdfPageEmission::Monolithic] {
                let actual = render(
                    &source,
                    PdfCompression::NativeWorkers { max_workers },
                    emission,
                );
                assert_eq!(
                    actual, baseline,
                    "large document: {max_workers} workers, {emission:?}"
                );
                assert_eq!(inspect(&actual).page_text.concat(), expected);
            }
        }
        assert_eq!(
            baseline,
            render(
                &source,
                PdfCompression::NativeWorkers { max_workers: 4 },
                PdfPageEmission::Chunked
            )
        );
    }

    #[test]
    fn retained_byte_admission_boundary_is_unchanged_by_requested_workers() {
        for emission in [PdfPageEmission::Chunked, PdfPageEmission::Monolithic] {
            let mut budget = emit(PdfCompression::Sequential, emission);
            budget.max_retained_bytes = Some(1);
            let error = render_pdf_emitted(SMALL, &options(), budget)
                .unwrap_err()
                .to_string();
            let required = error
                .split_once("pdf_heap_ceiling: retained ")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert!(required > 1);
            budget.max_retained_bytes = Some(required);
            let accepted = render_pdf_emitted(SMALL, &options(), budget).unwrap();
            assert_eq!(accepted, render_pdf(SMALL, &options()).unwrap());
            budget.max_retained_bytes = Some(required - 1);
            let rejected = render_pdf_emitted(SMALL, &options(), budget)
                .unwrap_err()
                .to_string();
            assert!(rejected.contains("pdf_heap_ceiling:"));
            for max_workers in [1, 2, 4] {
                budget.compression = PdfCompression::NativeWorkers { max_workers };
                budget.max_retained_bytes = Some(required);
                assert_eq!(
                    accepted,
                    render_pdf_emitted(SMALL, &options(), budget).unwrap()
                );
                budget.max_retained_bytes = Some(required - 1);
                assert_eq!(
                    rejected,
                    render_pdf_emitted(SMALL, &options(), budget)
                        .unwrap_err()
                        .to_string()
                );
            }
        }
    }

    #[test]
    fn ceilings_preserve_large_document_success_and_failure_results() {
        let (source, expected) = large_fixture("LIMIT");
        for emission in [PdfPageEmission::Chunked, PdfPageEmission::Monolithic] {
            for ceiling in [1, 64 * 1024 * 1024] {
                let mut sequential = emit(PdfCompression::Sequential, emission);
                sequential.max_retained_bytes = Some(ceiling);
                let baseline = render_pdf_emitted(&source, &options(), sequential)
                    .map_err(|error| error.to_string());
                let native = PdfEmitOptions {
                    compression: PdfCompression::NativeWorkers { max_workers: 4 },
                    ..sequential
                };
                let actual = render_pdf_emitted(&source, &options(), native)
                    .map_err(|error| error.to_string());
                assert_eq!(actual, baseline);
                if ceiling == 1 {
                    assert!(actual.unwrap_err().contains("pdf_heap_ceiling:"));
                } else {
                    let decoded = inspect(&actual.unwrap());
                    assert!(decoded.decoded_bytes > 256 * 1024);
                    assert_eq!(decoded.page_text.concat(), expected);
                }
            }
        }
    }

    #[test]
    fn independent_host_jobs_keep_their_own_outputs_and_worker_budgets() {
        let (left, left_text) = large_fixture("LEFT");
        let (right, right_text) = large_fixture("RIGHT");
        let left_expected = render(&left, PdfCompression::Sequential, PdfPageEmission::Chunked);
        let right_expected = render(
            &right,
            PdfCompression::Sequential,
            PdfPageEmission::Monolithic,
        );
        let start = std::sync::Barrier::new(2);
        let (left_actual, right_actual) = std::thread::scope(|scope| {
            let left = scope.spawn(|| {
                start.wait();
                render(
                    &left,
                    PdfCompression::NativeWorkers { max_workers: 2 },
                    PdfPageEmission::Chunked,
                )
            });
            let right = scope.spawn(|| {
                start.wait();
                render(
                    &right,
                    PdfCompression::NativeWorkers { max_workers: 2 },
                    PdfPageEmission::Monolithic,
                )
            });
            (left.join().unwrap(), right.join().unwrap())
        });
        assert_eq!(left_actual, left_expected);
        assert_eq!(right_actual, right_expected);
        assert_eq!(inspect(&left_actual).page_text.concat(), left_text);
        assert_eq!(inspect(&right_actual).page_text.concat(), right_text);
        // This exercises nested callers without a flaky process-wide thread
        // counter. Scheduler unit tests assert each pool's in-flight bound and
        // joined lifetime, including partial-spawn and panic failures.
    }
}
