//! GIF publication retains the original animation, including frame metadata.

use franken_markdown::{
    BookInput, HtmlOptions, PdfImageAsset, build_book, parse_markdown, render_epub,
    render_html_document,
};

const ANIMATION: &str = include_str!("../fixtures/gif/animated-colors.base64");

// Independently encoded with Pillow: two 2x2 frames, 80/120 ms delays, a
// looping extension, transparency, and a local palette on the second frame.
// Retaining every byte catches accidental first-frame conversion in formats
// whose readers can display the complete animation.
fn animation() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut value = 0u32;
    let mut bits = 0;
    for byte in ANIMATION.trim().bytes().take_while(|&byte| byte != b'=') {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("invalid GIF fixture encoding"),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((value >> bits) as u8);
        }
    }
    bytes
}

fn assert_animation(epub: &[u8], chapters: usize, expected: &[u8]) {
    let entries = super::parse_central_directory(epub);
    let images: Vec<_> = entries
        .iter()
        .filter(|entry| entry.name.starts_with("OEBPS/assets/"))
        .collect();
    assert_eq!(images.len(), 1, "identical GIFs share one resource");
    assert!(images[0].name.ends_with(".gif"));
    assert_eq!(super::recover_entry(epub, images[0]), expected);
    let href = images[0].name.strip_prefix("OEBPS/").unwrap();
    let opf = super::entry_text(epub, "OEBPS/content.opf");
    assert!(opf.contains(&format!("href=\"{href}\" media-type=\"image/gif\"")));
    for chapter in 1..=chapters {
        let xhtml = super::entry_text(epub, &format!("OEBPS/chapter-{chapter}.xhtml"));
        super::assert_well_formed_xml(&xhtml);
        assert!(xhtml.contains(&format!("src=\"{href}\"")));
        assert!(xhtml.contains("alt=\"Animation\""));
        assert!(!xhtml.contains("Image unavailable"));
    }
}

#[test]
fn html_and_epub_retain_complete_gif_assets_under_opaque_destination_keys() {
    let bytes = animation();
    let doc = parse_markdown("# Animation\n\n![Animation](opaque-asset)\n");
    let opts = HtmlOptions {
        image_assets: vec![PdfImageAsset::new("opaque-asset", bytes.clone())],
        ..HtmlOptions::default()
    };
    let html = render_html_document(&doc, &opts).unwrap();
    assert!(html.contains(&format!(
        "src=\"data:image/gif;base64,{}\"",
        ANIMATION.trim()
    )));
    let epub = render_epub(&doc, &opts).unwrap();
    assert_animation(&epub, 1, &bytes);
    assert_eq!(epub, render_epub(&doc, &opts).unwrap());
}

#[test]
fn book_gifs_resolve_chapter_relative_assets_and_keep_one_complete_animation() {
    let bytes = animation();
    let book = build_book(&[
        BookInput {
            path: "guide/first.md".into(),
            source: "# First\n\n![Animation](motion.gif)\n".into(),
        },
        BookInput {
            path: "second.md".into(),
            source: "# Second\n\n![Animation](other.gif)\n".into(),
        },
    ])
    .unwrap();
    let opts = HtmlOptions {
        image_assets: vec![
            PdfImageAsset::new("guide/motion.gif", bytes.clone()),
            PdfImageAsset::new("other.gif", bytes.clone()),
        ],
        ..HtmlOptions::default()
    };
    let epub = franken_markdown::epub::render_book_epub(&book, &opts).unwrap();
    assert_animation(&epub, 2, &bytes);
    assert_eq!(
        epub,
        franken_markdown::epub::render_book_epub(&book, &opts).unwrap()
    );
}

#[cfg(feature = "cli")]
mod native {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "fmd-gif-publication-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
        fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
            let path = self.path(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn invoke(dir: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .arg("--no-config")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
    }

    fn success(output: Output) {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("image_unsupported"));
    }

    #[test]
    fn local_gifs_reach_html_epub_and_multi_chapter_publications() {
        let dir = Directory::new();
        let bytes = animation();
        dir.write("book/guide/assets/motion demo.GIF", &bytes);
        dir.write(
            "book/guide/first.md",
            "# First\n\n![Animation](assets/motion%20demo.GIF)\n",
        );
        dir.write(
            "book/second.md",
            "# Second\n\n![Animation](guide/assets/motion%20demo.GIF)\n",
        );
        success(invoke(
            &dir.0,
            &["book/guide/first.md", "--out", "single.html"],
        ));
        let html = std::fs::read_to_string(dir.path("single.html")).unwrap();
        assert!(html.contains(&format!("data:image/gif;base64,{}", ANIMATION.trim())));
        success(invoke(
            &dir.0,
            &[
                "book/guide/first.md",
                "--to",
                "epub",
                "--out",
                "single.epub",
            ],
        ));
        assert_animation(&std::fs::read(dir.path("single.epub")).unwrap(), 1, &bytes);
        success(invoke(
            &dir.0,
            &["book", "book", "--to", "epub", "--out", "book.epub"],
        ));
        assert_animation(&std::fs::read(dir.path("book.epub")).unwrap(), 2, &bytes);
        success(invoke(
            &dir.0,
            &["book", "book", "--to", "html", "--out-dir", "site"],
        ));
        for page in ["guide__first.html", "second.html"] {
            let html = std::fs::read_to_string(dir.path(&format!("site/{page}"))).unwrap();
            assert!(html.contains(&format!("data:image/gif;base64,{}", ANIMATION.trim())));
        }
    }

    #[cfg(feature = "batch")]
    #[test]
    fn batch_combined_output_preserves_gif_animation_and_paints_a_pdf_image() {
        let dir = Directory::new();
        dir.write("input/media/motion demo.GIF", animation());
        dir.write(
            "input/chapter.md",
            "# Animation\n\n![Animation](media/motion%20demo.GIF)\n",
        );
        success(invoke(
            &dir.0,
            &[
                "batch",
                "input",
                "--to",
                "both",
                "--out-dir",
                "exported",
                "--workers",
                "1",
            ],
        ));
        let html = std::fs::read_to_string(dir.path("exported/chapter.html")).unwrap();
        assert!(html.contains(&format!(
            "src=\"data:image/gif;base64,{}\"",
            ANIMATION.trim()
        )));
        let pdf = std::fs::read(dir.path("exported/chapter.pdf")).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let image = b"/Subtype /Image";
        assert!(
            pdf.windows(image.len()).any(|part| part == image),
            "the batch PDF must paint the local GIF instead of its fallback"
        );
    }

    #[test]
    fn explicit_gif_bindings_preserve_animation_and_obey_the_image_byte_limit() {
        let dir = Directory::new();
        let bytes = animation();
        dir.write("animation.gif", &bytes);
        let args = [
            "--text",
            "![Animation](opaque-asset)",
            "--to",
            "epub",
            "--pdf-image",
            "opaque-asset=animation.gif",
            "--out",
            "bound.epub",
        ];
        success(invoke(&dir.0, &args));
        assert_animation(&std::fs::read(dir.path("bound.epub")).unwrap(), 1, &bytes);
        dir.write("local.md", "![Animation](animation.gif)\n");
        dir.write("limited.epub", b"previous complete publication");
        let failure = invoke(
            &dir.0,
            &[
                "local.md",
                "--to",
                "epub",
                "--out",
                "limited.epub",
                "--max-pdf-image-bytes",
                "112",
            ],
        );
        assert_eq!(failure.status.code(), Some(66));
        assert!(failure.stdout.is_empty());
        assert_eq!(
            std::fs::read(dir.path("limited.epub")).unwrap(),
            b"previous complete publication"
        );
    }
}
