//! A complete native edition shares inputs and publishes every format together.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{HtmlOptions, PdfASettings, PdfImageAsset, PdfOptions};

const FONT: &[u8] = include_bytes!("../fmd-font/fonts/computer-modern/cmunrm.ttf");
// Independently encoded 2x2 RGB PNGs. A changing response catches repeated fetches.
const RED: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x02\x00\x00\x00\xfd\xd4\x9a\x73\x00\x00\x00\x10IDAT\x78\x9c\x63\x38\x21\xc2\x05\x44\x0c\x10\x0a\x00\x1c\x2e\x03\x99\x40\x9c\x9e\xf4\x00\x00\x00\x00IEND\xae\x42\x60\x82";
const BLUE: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x02\x00\x00\x00\xfd\xd4\x9a\x73\x00\x00\x00\x10IDAT\x78\x9c\x63\xe0\x32\xba\x03\x44\x0c\x10\x0a\x00\x1b\x66\x04\x61\x9e\x7f\xab\x2a\x00\x00\x00\x00IEND\xae\x42\x60\x82";
const SITE_FILES: &[&str] = &[
    "start.html",
    "guide__end.html",
    "search-index.json",
    "~fmd-search.html",
    "index.html",
];

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fmd-book-edition-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("book")).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.0.join(path)).unwrap()
    }

    fn text(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap()
    }

    fn command(&self, binary: &str) -> Command {
        let mut command = Command::new(binary);
        command
            .current_dir(&self.0)
            .args(["--no-config", "book", "book", "--json"])
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env("NO_PROXY", "127.0.0.1")
            .env("no_proxy", "127.0.0.1");
        command
    }

    fn publish(&self, target: &str, directory: &str, extra: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_fmd"))
            .args(["--to", target, "--out-dir", directory])
            .args(extra)
            .output()
            .unwrap()
    }

    fn two_chapters(&self) {
        self.write("book/start.md", "# Start\n\n[Next](guide/end.md#end).\n");
        self.write("book/guide/end.md", "# End\n\n[Back](../start.md#start).\n");
        self.write("book/book.toml", "order=['start.md','guide/end.md']\n");
    }

    fn seed_edition(&self) -> BTreeMap<String, Vec<u8>> {
        SITE_FILES
            .iter()
            .copied()
            .chain(["book.pdf", "book.epub"])
            .map(|name| {
                let path = format!("edition/{name}");
                let bytes = format!("previous publication: {name}").into_bytes();
                self.write(&path, &bytes);
                (path, bytes)
            })
            .collect()
    }

    fn assert_preserved(&self, previous: &BTreeMap<String, Vec<u8>>) {
        for (path, bytes) in previous {
            assert_eq!(&self.bytes(path), bytes, "changed {path}");
        }
        let mut names: Vec<_> = fs::read_dir(self.0.join("edition"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        let mut expected: Vec<_> = previous
            .keys()
            .map(|path| path.strip_prefix("edition/").unwrap().to_string())
            .collect();
        expected.sort();
        assert_eq!(names, expected, "no new or temporary publication files");
    }
}

fn succeeded(output: &Output) -> &str {
    assert!(
        output.status.success(),
        "status {:?}: stderr={} stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    std::str::from_utf8(&output.stdout).unwrap()
}

fn failed(output: &Output, expected: i32, tag: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "no partial success receipt");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"ok\":false") && stderr.contains(tag),
        "{stderr}"
    );
}

// Independent classic-ZIP and fixed/stored-DEFLATE reader. Inspect actual
// archive payloads, not the producer's resource inventory or a byte substring
// that could accidentally occur inside a compressed font or image.
fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}

fn dword(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn crc(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for &byte in bytes {
        value ^= u32::from(byte);
        for _ in 0..8 {
            value = (value >> 1) ^ if value & 1 == 1 { 0xedb8_8320 } else { 0 };
        }
    }
    !value
}

struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn read(&mut self, count: usize) -> usize {
        let mut value = 0;
        for bit in 0..count {
            value |= usize::from((self.bytes[self.at / 8] >> (self.at % 8)) & 1) << bit;
            self.at += 1;
        }
        value
    }

    fn huffman(&mut self, count: usize) -> usize {
        (0..count).fold(0, |value, _| (value << 1) | self.read(1))
    }

    fn symbol(&mut self) -> usize {
        let mut code = self.huffman(7);
        if code < 24 {
            return 256 + code;
        }
        code = (code << 1) | self.read(1);
        match code {
            48..=191 => code - 48,
            192..=199 => 280 + code - 192,
            _ => {
                code = (code << 1) | self.read(1);
                assert!((400..=511).contains(&code));
                144 + code - 400
            }
        }
    }
}

fn inflate(bytes: &[u8], size: usize) -> Vec<u8> {
    const LENGTHS: [usize; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LENGTH_BITS: [usize; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DISTANCES: [usize; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    let mut bits = Bits { bytes, at: 0 };
    let mut out = Vec::with_capacity(size);
    loop {
        let last = bits.read(1);
        match bits.read(2) {
            0 => {
                bits.at = bits.at.div_ceil(8) * 8;
                let length = bits.read(16);
                assert_eq!(length ^ bits.read(16), 0xffff);
                for _ in 0..length {
                    out.push(bits.read(8) as u8);
                }
            }
            1 => loop {
                match bits.symbol() {
                    literal @ 0..=255 => out.push(literal as u8),
                    256 => break,
                    symbol @ 257..=285 => {
                        let index = symbol - 257;
                        let length = LENGTHS[index] + bits.read(LENGTH_BITS[index]);
                        let distance = bits.huffman(5);
                        assert!(distance < DISTANCES.len());
                        let distance =
                            DISTANCES[distance] + bits.read(distance.saturating_sub(2) / 2);
                        assert!(distance <= out.len());
                        for _ in 0..length {
                            out.push(out[out.len() - distance]);
                        }
                    }
                    symbol => panic!("invalid fixed symbol {symbol}"),
                }
                assert!(out.len() <= size);
            },
            kind => panic!("unexpected DEFLATE block type {kind}"),
        }
        assert!(out.len() <= size);
        if last == 1 {
            assert_eq!(out.len(), size);
            return out;
        }
    }
}

fn archive(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let end = bytes.len() - 22;
    assert_eq!(dword(bytes, end), 0x0605_4b50);
    assert_eq!(word(bytes, end + 20), 0);
    let count = usize::from(word(bytes, end + 10));
    let mut at = dword(bytes, end + 16) as usize;
    assert_eq!(at + dword(bytes, end + 12) as usize, end);
    let mut entries = BTreeMap::new();
    for index in 0..count {
        assert_eq!(dword(bytes, at), 0x0201_4b50);
        let method = word(bytes, at + 10);
        let checksum = dword(bytes, at + 16);
        let compressed = dword(bytes, at + 20) as usize;
        let size = dword(bytes, at + 24) as usize;
        let name_len = usize::from(word(bytes, at + 28));
        let name = std::str::from_utf8(&bytes[at + 46..at + 46 + name_len]).unwrap();
        let local = dword(bytes, at + 42) as usize;
        assert_eq!(dword(bytes, local), 0x0403_4b50);
        assert_eq!(word(bytes, local + 8), method);
        assert_eq!(word(bytes, local + 6) & 8, 0);
        assert_eq!(dword(bytes, local + 14), checksum);
        assert_eq!(dword(bytes, local + 18) as usize, compressed);
        assert_eq!(dword(bytes, local + 22) as usize, size);
        assert_eq!(&bytes[local + 30..local + 30 + name_len], name.as_bytes());
        let start = local + 30 + name_len + usize::from(word(bytes, local + 28));
        let data = &bytes[start..start + compressed];
        let data = match method {
            0 => data.to_vec(),
            8 => inflate(data, size),
            _ => panic!("unsupported ZIP method {method}"),
        };
        assert_eq!(data.len(), size);
        assert_eq!(crc(&data), checksum, "CRC for {name}");
        if index == 0 {
            assert_eq!((name, method, local), ("mimetype", 0, 0));
            assert_eq!(data, b"application/epub+zip");
        }
        assert!(entries.insert(name.to_owned(), data).is_none());
        at += 46 + name_len + usize::from(word(bytes, at + 30)) + usize::from(word(bytes, at + 32));
    }
    assert_eq!(at, end);
    entries
}

fn text<'a>(entries: &'a BTreeMap<String, Vec<u8>>, name: &str) -> &'a str {
    std::str::from_utf8(entries.get(name).unwrap()).unwrap()
}

#[test]
fn all_preserves_each_publication_with_shared_fonts_images_and_chapter_settings() {
    let workspace = Workspace::new();
    workspace.write("book/book.toml", "title='Shared edition'\nauthor='Author & Proof'\nlang='en'\norder=['start.md','guide/end.md']\ninclude_only=['parts/shared.md']\n");
    workspace.write("book/start.md", "---\nlang: de\ntoc: true\ntoc_depth: 1\n---\n# Start\n\n{{#include parts/shared.md}}\n\n[Next](guide/end.md#end). Evidence[^n]; [read the note](#fn-n).\n\n## Body detail\n\n### Deeper detail\n\n[^n]: Retained note content.\n");
    workspace.write("book/guide/end.md", "---\nlang: fr\ntoc: false\ntoc_depth: 3\n---\n# End\n\n[Back](../start.md#start).\n\n![Repeated figure](../parts/chart.png)\n\n## Detail\n\n### Deep section\n");
    workspace.write("book/parts/shared.md", "Included source.\n\n![Shared figure](chart.png)\n\nTerm\n: Literal definition under the selected profile.\n");
    workspace.write("book/parts/chart.png", RED);
    workspace.write("body.ttf", FONT);
    let common = [
        "--deny-broken-links",
        "--profile",
        "commonmark-gfm",
        "--pdf-font",
        "body-regular=body.ttf",
        "--font-scale",
        "110%",
        "--font-embedding",
        "full",
    ];
    let mut combined = common.to_vec();
    combined.extend([
        "--html-font-format",
        "ttf",
        "--pdf-a",
        "2b",
        "--pdf-a-strict",
        "--pdf-footnote-backlinks",
        "--pdf-line-numbers",
    ]);
    let output = workspace.publish("all", "edition", &combined);
    let receipt = succeeded(&output);
    assert!(receipt.contains("\"target\":\"all\"") && receipt.contains("\"chapters\":2"));
    assert!(receipt.contains("\"epub_name\":\"chapter-1.xhtml\""));
    assert!(receipt.contains("\"epub_name\":\"chapter-2.xhtml\""));
    assert!(receipt.contains("\"out_name\":\"guide__end.html\""));
    assert!(receipt.contains("\"link_check\":") && receipt.contains("\"unresolved_links\":0"));
    assert!(receipt.contains("\"pdf_a\":{\"profile\":\"2b\",\"strict\":true}"));

    let pair_output = workspace.publish("both", "pair", &combined);
    let pair_receipt = succeeded(&pair_output);
    assert!(!pair_receipt.contains("epub_name"));
    assert!(!workspace.0.join("pair/book.epub").exists());
    for name in SITE_FILES.iter().copied().chain(["book.pdf"]) {
        assert_eq!(
            workspace.bytes(&format!("edition/{name}")),
            workspace.bytes(&format!("pair/{name}")),
            "complete-edition bytes differ for {name}"
        );
    }
    let epub_output = workspace.publish("epub", "epub-only", &common);
    assert!(!succeeded(&epub_output).contains("epub_name"));
    let epub = workspace.bytes("edition/book.epub");
    assert_eq!(epub, workspace.bytes("epub-only/book.epub"));
    let entries = archive(&epub);
    let metadata = text(&entries, "OEBPS/content.opf");
    assert!(metadata.contains("<dc:title>Shared edition</dc:title>"));
    assert!(metadata.contains("<dc:creator>Author &amp; Proof</dc:creator>"));
    let first = text(&entries, "OEBPS/chapter-1.xhtml");
    let last = text(&entries, "OEBPS/chapter-2.xhtml");
    assert!(first.contains("xml:lang=\"de\"") && last.contains("xml:lang=\"fr\""));
    assert!(first.contains("href=\"chapter-2.xhtml#end\""));
    assert!(last.contains("href=\"chapter-1.xhtml#start\""));
    assert!(first.contains("Included source.") && first.contains("Retained note content."));
    assert!(first.contains("epub:type=\"noteref\""));
    assert!(!entries.contains_key("OEBPS/chapter-3.xhtml"));
    assert!(
        entries
            .iter()
            .any(|(name, bytes)| name.starts_with("OEBPS/fonts/") && bytes.as_slice() == FONT)
    );
    let pngs: Vec<_> = entries
        .iter()
        .filter(|(name, _)| name.ends_with(".png"))
        .collect();
    assert_eq!(pngs.len(), 1, "the same figure is packaged once");
    assert_eq!(pngs[0].1.as_slice(), RED);

    let image = format!(
        "data:image/png;base64,{}",
        franken_markdown::html::base64_encode(RED)
    );
    assert!(workspace.text("edition/start.html").contains(&image));
    assert!(workspace.text("edition/guide__end.html").contains(&image));
    assert!(!workspace.0.join("edition/parts__shared.html").exists());
    let previous: Vec<_> = SITE_FILES
        .iter()
        .copied()
        .chain(["book.pdf", "book.epub"])
        .map(|name| (name, workspace.bytes(&format!("edition/{name}"))))
        .collect();
    assert_eq!(
        succeeded(&workspace.publish("all", "edition", &combined)),
        receipt
    );
    for (name, bytes) in previous {
        assert_eq!(workspace.bytes(&format!("edition/{name}")), bytes);
    }
}

#[test]
fn all_has_normal_default_destinations_in_both_executables_and_keeps_default_both() {
    let workspace = Workspace::new();
    workspace.write("book/start.md", "# A complete edition\n\nBody.\n");
    let mut original = None;
    for binary in [
        env!("CARGO_BIN_EXE_fmd"),
        env!("CARGO_BIN_EXE_franken_markdown"),
    ] {
        let output = workspace
            .command(binary)
            .args(["--to", "all"])
            .output()
            .unwrap();
        let receipt = succeeded(&output);
        let expected_outputs = [
            "book-site/start.html",
            "book-site/search-index.json",
            "book-site/~fmd-search.html",
            "book-site/index.html",
            "book.pdf",
            "book.epub",
        ]
        .iter()
        .map(|path| format!("{:?}", PathBuf::from(".").join(path).to_string_lossy()))
        .collect::<Vec<_>>()
        .join(",");
        assert!(
            receipt.contains(&format!("\"outputs\":[{expected_outputs}]")),
            "{receipt}"
        );
        let bytes = [
            workspace.bytes("book-site/start.html"),
            workspace.bytes("book.pdf"),
            workspace.bytes("book.epub"),
        ];
        if let Some(previous) = &original {
            assert_eq!(&bytes, previous);
        } else {
            original = Some(bytes);
        }
    }
    let output = workspace
        .command(env!("CARGO_BIN_EXE_fmd"))
        .args(["--out-dir", "default-target"])
        .output()
        .unwrap();
    assert!(succeeded(&output).contains("\"target\":\"both\""));
    assert!(workspace.0.join("default-target/start.html").is_file());
    assert!(workspace.0.join("default-target/book.pdf").is_file());
    assert!(!workspace.0.join("default-target/book.epub").exists());
}

#[test]
fn any_format_failure_preserves_every_existing_edition_output() {
    for (extra, forbidden_note, expected) in [
        (
            vec!["--toc=false", "--fit-to-pages", "1"],
            false,
            "page_budget_unmet",
        ),
        (
            vec!["--pdf-a", "2b", "--pdf-a-strict"],
            true,
            "pdf_a_file_uri",
        ),
        (
            vec!["--author", "Author\u{b}Name"],
            false,
            "epub_render_error",
        ),
    ] {
        let workspace = Workspace::new();
        workspace.two_chapters();
        if forbidden_note {
            workspace.write(
                "book/start.md",
                "# Start\n\n[^unused]: [Forbidden](file:///private/source).\n",
            );
        }
        // The last case passes both earlier formats and fails during EPUB.
        if expected == "epub_render_error" {
            succeeded(&workspace.publish("both", "control", &extra));
        }
        let previous = workspace.seed_edition();
        failed(&workspace.publish("all", "edition", &extra), 70, expected);
        workspace.assert_preserved(&previous);
    }
}

#[test]
fn epub_destination_is_protected_when_it_is_a_loaded_resource_or_directory() {
    for directory in [false, true] {
        let workspace = Workspace::new();
        workspace.two_chapters();
        let mut previous = workspace.seed_edition();
        let extra = if directory {
            // A distinct directory is selected without deleting a fixture file.
            let path = workspace.0.join("blocked");
            fs::create_dir_all(path.join("book.epub")).unwrap();
            for (source, bytes) in &previous {
                let name = source.strip_prefix("edition/").unwrap();
                if name != "book.epub" {
                    workspace.write(&format!("blocked/{name}"), bytes);
                }
            }
            let output = workspace.publish("all", "blocked", &[]);
            failed(&output, 73, "output is a directory");
            for (source, bytes) in &previous {
                let name = source.strip_prefix("edition/").unwrap();
                if name != "book.epub" {
                    assert_eq!(&workspace.bytes(&format!("blocked/{name}")), bytes);
                }
            }
            assert!(
                fs::read_dir(path.join("book.epub"))
                    .unwrap()
                    .next()
                    .is_none()
            );
            continue;
        } else {
            workspace.write("edition/book.epub", RED);
            previous.insert("edition/book.epub".into(), RED.to_vec());
            ["--pdf-image", "brand=edition/book.epub"]
        };
        failed(
            &workspace.publish("all", "edition", &extra),
            73,
            "overwrite a book input",
        );
        workspace.assert_preserved(&previous);
    }
}

#[cfg(unix)]
#[test]
fn pdf_and_epub_output_aliases_cannot_split_the_edition_transaction() {
    let workspace = Workspace::new();
    workspace.two_chapters();
    let mut previous = BTreeMap::new();
    for name in SITE_FILES.iter().copied().chain(["book.pdf"]) {
        let path = format!("edition/{name}");
        let bytes = format!("retained {name}").into_bytes();
        workspace.write(&path, &bytes);
        previous.insert(path, bytes);
    }
    fs::hard_link(
        workspace.0.join("edition/book.pdf"),
        workspace.0.join("edition/book.epub"),
    )
    .unwrap();
    previous.insert(
        "edition/book.epub".into(),
        workspace.bytes("edition/book.pdf"),
    );
    failed(
        &workspace.publish("all", "edition", &[]),
        73,
        "duplicate output path",
    );
    workspace.assert_preserved(&previous);
}

#[test]
fn all_rejects_incompatible_output_and_invalid_options_before_source_io() {
    let workspace = Workspace::new();
    // Empty book deliberately cannot publish: admission errors must win first.
    for extra in [
        vec!["--out", "edition.pdf"],
        vec!["--out-dir", "-"],
        vec!["--check-links"],
        vec!["--pdf-a-strict"],
        vec!["--fit-to-pages", "0"],
        vec!["--page-size", "not-paper"],
        vec!["--pdf-baseline-grid-pt", "12", "--fit-to-pages", "1"],
    ] {
        let output = workspace
            .command(env!("CARGO_BIN_EXE_fmd"))
            .args(["--to", "all"])
            .args(extra)
            .output()
            .unwrap();
        failed(&output, 64, "\"ok\":false");
    }
    assert!(!workspace.0.join("book-site").exists());
    assert!(!workspace.0.join("book.pdf").exists());
    assert!(!workspace.0.join("book.epub").exists());
}

struct ChangingImage {
    url: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ChangingImage {
    fn start(source: PathBuf) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!("http://{}/changing.png", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&requests);
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("loopback listener: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while request.len() < 8192 && !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => request.extend_from_slice(&buffer[..count]),
                    }
                }
                let occurrence = seen.fetch_add(1, Ordering::AcqRel);
                if occurrence == 0 {
                    // Parsing has already captured this source. Reopening it
                    // for a later format would publish a different edition.
                    fs::write(
                        &source,
                        "# Changed after capture\n\nDifferent source bytes.\n",
                    )
                    .unwrap();
                }
                let body = if occurrence == 0 { RED } else { BLUE };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if stream.write_all(header.as_bytes()).is_ok() {
                    let _ = stream.write_all(body);
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for ChangingImage {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[test]
fn all_uses_one_remote_image_and_source_snapshot_across_every_format() {
    let workspace = Workspace::new();
    let server = ChangingImage::start(workspace.0.join("book/start.md"));
    let first = format!(
        "# Snapshot\n\nOriginal captured source.\n\n![Figure]({})\n",
        server.url
    );
    let last = format!("# End\n\n![Same figure]({})\n", server.url);
    workspace.write("book/start.md", &first);
    workspace.write("book/guide/end.md", &last);
    workspace.write("book/book.toml", "order=['start.md','guide/end.md']\n");
    let output = workspace.publish("all", "edition", &["--embed-remote-images"]);
    succeeded(&output);
    assert_eq!(server.requests.load(Ordering::Acquire), 1);
    assert!(
        workspace
            .text("book/start.md")
            .contains("Changed after capture")
    );
    let image = format!(
        "data:image/png;base64,{}",
        franken_markdown::html::base64_encode(RED)
    );
    let html = workspace.text("edition/start.html");
    assert!(html.contains("Original captured source.") && html.contains(&image));
    assert!(!html.contains("Different source bytes."));
    assert!(workspace.text("edition/guide__end.html").contains(&image));
    let renderer = BookRenderer::new(&[
        BookInput {
            path: "start.md".into(),
            source: first,
        },
        BookInput {
            path: "guide/end.md".into(),
            source: last,
        },
    ])
    .unwrap();
    let assets = vec![PdfImageAsset::new(server.url.clone(), RED.to_vec())];
    let pdf = renderer
        .render_pdf_with_options(
            &PdfOptions {
                title: Some("Snapshot".into()),
                image_assets: assets.clone(),
                toc: true,
                page_numbers: true,
                metadata_epoch_seconds: Some(1_700_000_000),
                ..PdfOptions::default()
            },
            PdfASettings::OFF,
        )
        .unwrap();
    assert_eq!(workspace.bytes("edition/book.pdf"), pdf);
    let epub = workspace.bytes("edition/book.epub");
    assert_eq!(
        epub,
        franken_markdown::epub::render_book_epub(
            renderer.book(),
            &HtmlOptions {
                title: Some("Snapshot".into()),
                image_assets: assets,
                ..HtmlOptions::default()
            }
        )
        .unwrap()
    );
    let entries = archive(&epub);
    let pngs: Vec<_> = entries
        .iter()
        .filter(|(name, _)| name.ends_with(".png"))
        .collect();
    assert_eq!(pngs.len(), 1);
    assert_eq!(pngs[0].1.as_slice(), RED);
    assert!(text(&entries, "OEBPS/chapter-1.xhtml").contains("Original captured source."));
}
