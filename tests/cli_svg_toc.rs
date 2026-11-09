//! Native SVG navigation options and per-document defaults reach real output.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{SvgOptions, parse_markdown, render_svg};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn workspace() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "fmd-svg-toc-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}

fn run(root: &std::path::Path, args: &[&str]) {
    let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .current_dir(root)
        .env("FMD_CONFIG", root.join("config"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn svg_flags_frontmatter_and_explicit_markers_reach_the_same_renderer() {
    let root = workspace();
    let body = "# Main\n\n## Detail\n\n### Extra\n";
    std::fs::write(root.join("plain.md"), body).unwrap();
    std::fs::write(
        root.join("metadata.md"),
        format!("---\ntoc: true\ntoc_depth: 2\n---\n{body}"),
    )
    .unwrap();
    std::fs::write(root.join("marker.md"), format!("[TOC:2]\n\n{body}")).unwrap();
    let expected = render_svg(
        &parse_markdown(body),
        &SvgOptions {
            toc: true,
            toc_depth: Some(2),
            ..SvgOptions::default()
        },
    );
    run(
        &root,
        &[
            "plain.md",
            "--to",
            "svg",
            "--out",
            "flags.svg",
            "--toc",
            "--toc-depth",
            "2",
            "--no-config",
        ],
    );
    run(
        &root,
        &["metadata.md", "--to", "svg", "--out", "metadata.svg", "--no-config"],
    );
    run(
        &root,
        &["marker.md", "--to", "svg", "--out", "marker.svg", "--no-config"],
    );
    for name in ["flags.svg", "metadata.svg", "marker.svg"] {
        assert_eq!(std::fs::read(root.join(name)).unwrap(), expected, "{name}");
    }
    #[cfg(feature = "batch")]
    {
        run(
            &root,
            &["batch", "metadata.md", "--to", "svg", "--out-dir", "batch", "--no-config"],
        );
        assert_eq!(
            std::fs::read(root.join("batch/metadata.svg")).unwrap(),
            expected
        );
        run(
            &root,
            &[
                "batch",
                "plain.md",
                "--to",
                "svg",
                "--out-dir",
                "batch-flags",
                "--toc",
                "--toc-depth",
                "2",
                "--no-config",
            ],
        );
        assert_eq!(
            std::fs::read(root.join("batch-flags/plain.svg")).unwrap(),
            expected
        );
    }
}
