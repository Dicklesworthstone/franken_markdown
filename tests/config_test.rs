//! Native config contract tests. These use `FMD_CONFIG` temp paths so the real
//! user config is never read or modified.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn fmd_with_config(args: &[&str], config_path: &PathBuf) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(args)
        .env("FMD_CONFIG", config_path)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn temp_file(name: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "fmd-config-test-{}-{}-{}.{}",
        std::process::id(),
        nanos,
        name,
        ext
    ))
}

#[test]
fn config_show_json_uses_defaults_and_env_path_when_missing() {
    let config = temp_file("missing", "conf");
    let _ = fs::remove_file(&config);
    let out = fmd_with_config(&["config", "show", "--json"], &config);

    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let stdout = text(&out.stdout);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"font\":\"sans\""));
    assert!(stdout.contains("\"theme\""));
    assert!(stdout.contains(&config.display().to_string()));
}

#[test]
fn config_set_get_and_render_use_persistent_default_font() {
    let config = temp_file("font", "conf");
    let html_path = temp_file("serif", "html");
    let html_path_s = html_path.display().to_string();

    let set = fmd_with_config(&["config", "set", "font", "serif", "--json"], &config);
    assert!(set.status.success());
    assert!(text(&set.stdout).contains("\"event\":\"config_set\""));

    let get = fmd_with_config(&["config", "get", "font", "--json"], &config);
    assert!(get.status.success());
    assert!(text(&get.stdout).contains("\"value\":\"serif\""));

    let render = fmd_with_config(&["--text", "# Configured", "--out", &html_path_s], &config);
    assert!(render.status.success());
    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("Source Serif 4"));

    let _ = fs::remove_file(config);
    let _ = fs::remove_file(html_path);
}

#[test]
fn no_config_ignores_persistent_default_font() {
    let config = temp_file("no-config", "conf");
    let html_path = temp_file("sans", "html");
    let html_path_s = html_path.display().to_string();

    let set = fmd_with_config(&["config", "set", "font", "serif"], &config);
    assert!(set.status.success());

    let render = fmd_with_config(
        &["--no-config", "--text", "# Repro", "--out", &html_path_s],
        &config,
    );
    assert!(render.status.success());
    let html = fs::read_to_string(&html_path).unwrap();
    assert!(!html.contains("Source Serif 4"));
    assert!(html.contains("Inter"));

    let _ = fs::remove_file(config);
    let _ = fs::remove_file(html_path);
}

#[test]
fn config_custom_css_is_used_when_render_flag_does_not_override_it() {
    let config = temp_file("css", "conf");
    let css = temp_file("style", "css");
    let html_path = temp_file("custom-css", "html");
    let html_path_s = html_path.display().to_string();

    let mut file = fs::File::create(&css).unwrap();
    file.write_all(b"body{color:#b00020}").unwrap();

    let set = fmd_with_config(
        &[
            "config",
            "set",
            "custom_css",
            &css.display().to_string(),
            "--json",
        ],
        &config,
    );
    assert!(set.status.success());

    let render = fmd_with_config(&["--text", "# CSS", "--out", &html_path_s], &config);
    assert!(render.status.success());
    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("body{color:#b00020}"));
    assert!(!html.contains("--fmd-accent"));

    let _ = fs::remove_file(config);
    let _ = fs::remove_file(css);
    let _ = fs::remove_file(html_path);
}
