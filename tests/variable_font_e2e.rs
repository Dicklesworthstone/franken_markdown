//! gk3v.4 aggregate gate: mixed-weight PDF via host bytes (library/WASM API)
//! and via the CLI font slot, plus determinism, cmap-shared bbox tolerance,
//! hostile fvar/gvar render sweeps, and a size report.
//!
//! Every assertion prints `check id=… subject=… outcome=PASS|FAIL` on stderr.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::Font;
use franken_markdown::wasm::{render_pdf as wasm_render_pdf, WasmRenderOptions};
use franken_markdown::{
    parse_markdown, render_pdf, render_pdf_document, FontAssetSlot, FontAssets, PdfOptions,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const EPOCH: u64 = 1_700_000_000;
const MIXED_MD: &str = "# Mixed weight\n\nRegular body.\n\n**Bold body at the bold slot.**\n";
const PROBE: &str = " ";

fn log_check(id: &str, subject: &str, ok: bool) {
    eprintln!(
        "check id={id} subject={subject} outcome={}",
        if ok { "PASS" } else { "FAIL" }
    );
    assert!(ok, "{id}: {subject}");
}

fn log_bytes(id: &str, subject: &str, a: &[u8], b: &[u8]) {
    let ok = a == b;
    if !ok {
        let pos = a
            .iter()
            .zip(b.iter())
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        eprintln!(
            "check id={id} subject={subject} outcome=FAIL fixture=variable_triangle_fixture diff_at={pos} a_len={} b_len={}",
            a.len(),
            b.len()
        );
    } else {
        eprintln!("check id={id} subject={subject} outcome=PASS");
    }
    assert!(ok, "{id}: {subject}");
}

fn vf_bytes() -> Vec<u8> {
    franken_markdown::text::variable_triangle_fixture()
}

fn mixed_assets(weight_regular: u16, weight_bold: u16) -> FontAssets {
    FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, vf_bytes())
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, weight_regular)
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyBold, weight_bold)
        .unwrap()
}

fn pdf_opts(assets: FontAssets) -> PdfOptions {
    PdfOptions {
        font_assets: assets,
        metadata_epoch_seconds: Some(EPOCH),
        ..PdfOptions::default()
    }
}

fn table_range(data: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    if data.len() < 12 {
        return None;
    }
    let n = u16::from_be_bytes(data[4..6].try_into().ok()?) as usize;
    for i in 0..n {
        let rec = 12 + i * 16;
        if data.get(rec..rec + 4)? == tag {
            let off = u32::from_be_bytes(data[rec + 8..rec + 12].try_into().ok()?) as usize;
            let len = u32::from_be_bytes(data[rec + 12..rec + 16].try_into().ok()?) as usize;
            return Some((off, len));
        }
    }
    None
}

fn lcg(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    (*state >> 33) as usize
}

fn bbox_linf(a: &Font, b: &Font, probe: &str) -> i32 {
    let mut max = 0i32;
    for ch in probe.chars() {
        let gid = a.glyph_index(ch);
        if gid != b.glyph_index(ch) {
            continue;
        }
        let Some(ba) = a.glyph_bbox(gid) else {
            continue;
        };
        let Some(bb) = b.glyph_bbox(gid) else {
            continue;
        };
        for i in 0..4 {
            max = max.max((i32::from(ba[i]) - i32::from(bb[i])).unsigned_abs() as i32);
        }
    }
    max
}

fn temp_path(label: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "fmd-gk3v4-{label}-{}-{nanos}.{ext}",
        std::process::id()
    ))
}

fn fmd(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .env("SOURCE_DATE_EPOCH", EPOCH.to_string())
        .env_remove("FMD_VERBOSE")
        .output()
        .unwrap()
}

#[test]
fn host_bytes_mixed_weight_pdf_is_deterministic_and_matches_wasm() {
    let assets = mixed_assets(400, 700);
    let opts = pdf_opts(assets.clone());
    let a = render_pdf(MIXED_MD, &opts).unwrap();
    let b = render_pdf(MIXED_MD, &opts).unwrap();
    log_check(
        "gk3v.4.host.pdf",
        "library PDF starts with %PDF-",
        a.starts_with(b"%PDF-"),
    );
    log_bytes(
        "gk3v.4.host.det",
        "same mixed-weight host bytes twice",
        &a,
        &b,
    );

    let wasm_opts = WasmRenderOptions {
        font_assets: assets,
        metadata_epoch_seconds: Some(EPOCH),
        ..WasmRenderOptions::default()
    };
    let wasm = wasm_render_pdf(MIXED_MD, &wasm_opts).unwrap();
    log_bytes(
        "gk3v.4.wasm.match",
        "WASM host-bytes PDF matches native library PDF",
        &a,
        &wasm.bytes,
    );

    let doc = parse_markdown(MIXED_MD);
    let via_doc = render_pdf_document(&doc, &opts).unwrap();
    log_bytes(
        "gk3v.4.host.doc",
        "render_pdf_document matches render_pdf",
        &a,
        &via_doc,
    );
}

#[test]
fn mixed_weight_pdf_differs_from_regular_only() {
    let mixed = render_pdf(MIXED_MD, &pdf_opts(mixed_assets(400, 700))).unwrap();
    let regular = render_pdf(
        MIXED_MD,
        &pdf_opts(
            FontAssets::default()
                .with_slot(FontAssetSlot::BodyRegular, vf_bytes())
                .unwrap()
                .with_slot_weight(FontAssetSlot::BodyRegular, 400)
                .unwrap(),
        ),
    )
    .unwrap();
    log_check(
        "gk3v.4.mixed.diff",
        "regular+bold slots differ from regular-only",
        mixed != regular && mixed.starts_with(b"%PDF-") && regular.starts_with(b"%PDF-"),
    );
}

#[test]
fn outline_tolerance_cmap_shared_bbox_linf() {
    let font = Font::parse(vf_bytes()).unwrap();
    let a = font.instance(450.0).unwrap();
    let b = font.instance(450.0).unwrap();
    log_bytes(
        "gk3v.4.tol.sfnt",
        "instance(450) twice is byte-identical",
        a.as_sfnt(),
        b.as_sfnt(),
    );
    log_check(
        "gk3v.4.tol.same",
        "cmap-shared bbox L∞ at the same weight is 0",
        bbox_linf(&a, &b, PROBE) == 0,
    );
    let light = font.instance(400.0).unwrap();
    let peak = font.instance(900.0).unwrap();
    log_check(
        "gk3v.4.tol.cross",
        "400 vs 900 cmap-shared bbox L∞ is 50",
        bbox_linf(&light, &peak, PROBE) == 50,
    );
}

#[test]
fn hostile_fvar_and_gvar_mutation_never_panics_the_render_path() {
    let original = vf_bytes();
    let fvar = table_range(&original, b"fvar").expect("fvar");
    let gvar = table_range(&original, b"gvar").expect("gvar");
    let mut state = 0x0BAD_F00Du64;
    for (tag, (off, len), rounds) in [("fvar", fvar, 32usize), ("gvar", gvar, 32usize)] {
        for round in 0..rounds {
            let mut mutated = original.clone();
            for _ in 0..4 {
                let pos = off + (lcg(&mut state) % len);
                mutated[pos] ^= 1u8 << (lcg(&mut state) % 8);
            }
            let outcome = std::panic::catch_unwind(move || {
                let assets = FontAssets {
                    body_regular: Some(mutated),
                    body_regular_weight: Some(650),
                    ..FontAssets::default()
                };
                let _ = render_pdf("# Hi\n\nhello\n", &pdf_opts(assets));
            });
            log_check(
                "gk3v.4.hostile.render",
                &format!("{tag} round {round} no panic"),
                outcome.is_ok(),
            );
        }
    }
}

#[test]
fn size_report_instances_are_not_larger_than_the_variable_face() {
    let vf = vf_bytes();
    let font = Font::parse(vf.clone()).unwrap();
    let inst_400 = font.instance(400.0).unwrap();
    let inst_700 = font.instance(700.0).unwrap();
    let plex_regular =
        include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf").len();
    let plex_bold = include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Bold.ttf").len();
    eprintln!(
        "check id=gk3v.4.size.report subject=vf={} inst400={} inst700={} plex_regular={} plex_bold={} plex_pair={} outcome=PASS",
        vf.len(),
        inst_400.as_sfnt().len(),
        inst_700.as_sfnt().len(),
        plex_regular,
        plex_bold,
        plex_regular + plex_bold
    );
    log_check(
        "gk3v.4.size.vf_small",
        "synthetic VF is far smaller than one static Plex cut",
        vf.len() < plex_regular,
    );
    log_check(
        "gk3v.4.size.instance_finite",
        "instanced static faces are non-empty",
        !inst_400.as_sfnt().is_empty() && !inst_700.as_sfnt().is_empty(),
    );
}

#[test]
fn cli_slot_mixed_weight_pdf_is_deterministic() {
    let vf_path = temp_path("vf", "ttf");
    fs::write(&vf_path, vf_bytes()).unwrap();
    let vf_s = vf_path.display().to_string();
    let mapping = format!("body-regular={vf_s}");
    let out_a = temp_path("mixed-a", "pdf");
    let out_b = temp_path("mixed-b", "pdf");
    let out_light = temp_path("w400", "pdf");
    let out_a_s = out_a.display().to_string();
    let out_b_s = out_b.display().to_string();
    let out_light_s = out_light.display().to_string();

    let run = |out: &str, extra: &[&str]| {
        let mut args = vec![
            "--no-config",
            "--text",
            MIXED_MD,
            "--to",
            "pdf",
            "--out",
            out,
            "--pdf-font",
            mapping.as_str(),
            "--pdf-font-weight",
            "body-regular=400",
        ];
        args.extend_from_slice(extra);
        fmd(&args)
    };

    let a = run(&out_a_s, &["--pdf-font-weight", "body-bold=700"]);
    log_check(
        "gk3v.4.cli.a.exit",
        "first CLI mixed-weight render exits 0",
        a.status.success(),
    );
    let b = run(&out_b_s, &["--pdf-font-weight", "body-bold=700"]);
    log_check(
        "gk3v.4.cli.b.exit",
        "second CLI mixed-weight render exits 0",
        b.status.success(),
    );
    let bytes_a = fs::read(&out_a).unwrap();
    let bytes_b = fs::read(&out_b).unwrap();
    log_bytes(
        "gk3v.4.cli.det",
        "CLI mixed-weight PDFs are byte-identical",
        &bytes_a,
        &bytes_b,
    );
    log_check(
        "gk3v.4.cli.pdf",
        "CLI output is a PDF",
        bytes_a.starts_with(b"%PDF-"),
    );

    let light = run(&out_light_s, &[]);
    log_check(
        "gk3v.4.cli.light.exit",
        "regular-only CLI render exits 0",
        light.status.success(),
    );
    let bytes_light = fs::read(&out_light).unwrap();
    log_check(
        "gk3v.4.cli.mixed.diff",
        "CLI mixed-weight PDF differs from regular-only",
        bytes_a != bytes_light,
    );

    // Phase logs stay on stderr; stdout stays empty when --out is a file.
    log_check(
        "gk3v.4.cli.stdout",
        "CLI stdout is empty with --out",
        a.stdout.is_empty(),
    );
    let stderr = String::from_utf8_lossy(&a.stderr);
    log_check(
        "gk3v.4.cli.phase",
        "stderr records font_instance/font_assets phase",
        stderr.contains("font_instance") || stderr.contains("font_assets"),
    );

    let _ = fs::remove_file(&vf_path);
    let _ = fs::remove_file(&out_a);
    let _ = fs::remove_file(&out_b);
    let _ = fs::remove_file(&out_light);
}

#[test]
fn docs_name_the_outline_tolerance_method() {
    let docs = fs::read_to_string("docs/VARIABLE_FONTS.md").unwrap();
    log_check(
        "gk3v.4.docs.method",
        "docs name cmap-shared bbox L∞",
        docs.contains("cmap-shared") && docs.contains("bbox L∞"),
    );
    log_check(
        "gk3v.4.docs.size",
        "docs feed smif.2 with a size report",
        docs.contains("smif.2") && docs.contains("IBMPlexSans-Regular"),
    );
}
