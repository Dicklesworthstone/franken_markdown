//! Native config contract tests. These use `FMD_CONFIG` temp paths so the real
//! user config is never read or modified.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use franken_markdown::config::{CONFIG_KEYS, ConfigError, FmdConfig};
use franken_markdown::{DarkModePolicy, FontFamily, PageMargins};

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

#[test]
fn config_rejects_multiline_custom_css_values_before_serializing() {
    let config = temp_file("css-newline", "conf");
    let injected_value = "style.css\nfont=serif";

    let set = fmd_with_config(
        &["config", "set", "custom_css", injected_value, "--json"],
        &config,
    );

    assert_eq!(set.status.code(), Some(64));
    assert!(set.stdout.is_empty());
    let stderr = text(&set.stderr);
    assert!(stderr.contains("\"code\":\"usage_error\""));
    assert!(stderr.contains("custom_css path must be a single line"));
    assert!(
        !config.exists(),
        "rejected multiline values must not create a config file with injected keys"
    );
}

#[test]
fn programmatic_config_save_rejects_multiline_custom_css_values() {
    let config_path = temp_file("programmatic-css-newline", "conf");
    let cfg = FmdConfig {
        custom_css: Some(PathBuf::from("style.css\nfont=serif")),
        ..FmdConfig::default()
    };

    let err = cfg
        .save_to_path(&config_path)
        .expect_err("multiline custom_css paths must not be serializable");

    assert!(
        err.to_string()
            .contains("custom_css path must be a single line")
    );
    assert!(
        !config_path.exists(),
        "rejected programmatic config values must not create an injected config file"
    );
    assert!(
        !cfg.to_file_string().contains("font=serif"),
        "infallible config formatting must not emit injected keys"
    );
}

#[test]
fn programmatic_config_save_validates_before_creating_parent_directories() {
    let config_path = temp_file("nested-programmatic-css-newline", "conf");
    let nested_dir = config_path.with_extension("dir");
    let nested_config = nested_dir.join("config");
    let cfg = FmdConfig {
        custom_css: Some(PathBuf::from("style.css\nfont=serif")),
        ..FmdConfig::default()
    };

    let err = cfg
        .save_to_path(&nested_config)
        .expect_err("invalid values must fail before any filesystem side effects");

    assert!(
        err.to_string()
            .contains("custom_css path must be a single line")
    );
    assert!(
        !nested_dir.exists(),
        "validation must happen before creating parent directories"
    );
    assert!(
        !nested_config.exists(),
        "validation must happen before writing the config file"
    );
}

/// Run `fmd` with a precise child-process environment. `None` removes the
/// variable for the child so the parent process environment is never mutated
/// (keeping these tests race-free under cargo's parallel test runner).
fn fmd_env(args: &[&str], envs: &[(&str, Option<&str>)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fmd"));
    cmd.args(args);
    for (key, value) in envs {
        match value {
            Some(v) => {
                cmd.env(key, v);
            }
            None => {
                cmd.env_remove(key);
            }
        }
    }
    cmd.output().unwrap()
}

// ---------------------------------------------------------------------------
// In-process unit coverage for the public config API (no env, no subprocess).
// ---------------------------------------------------------------------------

#[test]
fn config_error_displays_io_and_parse_variants() {
    // A real, non-synthetic IO error: reading a directory as a file fails with
    // something other than NotFound on every supported platform.
    let dir = std::env::temp_dir();
    let io_err = fs::read_to_string(&dir).expect_err("reading a directory as a file must fail");
    assert_ne!(io_err.kind(), std::io::ErrorKind::NotFound);
    let expected = io_err.to_string();
    // `From<std::io::Error>` conversion.
    let converted: ConfigError = io_err.into();
    assert!(matches!(converted, ConfigError::Io(_)));
    // `Display` for the Io variant delegates verbatim to the inner error.
    assert_eq!(converted.to_string(), expected);

    // `Display` for the Parse variant is the message verbatim.
    let parse_err = FmdConfig::parse("no-equals-here").expect_err("missing `=` must fail");
    assert!(matches!(parse_err, ConfigError::Parse(_)));
    assert_eq!(parse_err.to_string(), "line 1: expected key=value");
}

#[test]
fn parse_ignores_comments_blank_lines_and_whitespace() {
    let src = "\n   \n# a comment\n\t# indented comment\nfont = serif\n   dark_mode=off  \n";
    let cfg = FmdConfig::parse(src).expect("valid config must parse");
    assert_eq!(cfg.font, Some(FontFamily::Serif));
    assert_eq!(cfg.dark_mode, Some(DarkModePolicy::Disabled));
    assert_eq!(cfg.custom_css, None);
    assert_eq!(cfg.margins, None);
}

#[test]
fn parse_rejects_line_missing_equals_sign_with_line_number() {
    let err = FmdConfig::parse("font serif").expect_err("missing `=` is invalid");
    assert_eq!(err.to_string(), "line 1: expected key=value");
    let err2 = FmdConfig::parse("font=serif\nbroken").expect_err("missing `=` is invalid");
    assert_eq!(err2.to_string(), "line 2: expected key=value");
}

#[test]
fn parse_quoted_value_is_unquoted() {
    let cfg = FmdConfig::parse("custom_css=\"/themes/a.css\"").expect("quoted value parses");
    assert_eq!(cfg.custom_css, Some(PathBuf::from("/themes/a.css")));
}

#[test]
fn parse_custom_css_none_and_empty_clear_the_path() {
    assert_eq!(
        FmdConfig::parse("custom_css=none").unwrap().custom_css,
        None
    );
    assert_eq!(
        FmdConfig::parse("custom_css=NONE").unwrap().custom_css,
        None
    );
    assert_eq!(FmdConfig::parse("custom_css=").unwrap().custom_css, None);
    assert_eq!(FmdConfig::parse("custom_css=   ").unwrap().custom_css, None);
    assert_eq!(
        FmdConfig::parse("custom_css=/x/y.css").unwrap().custom_css,
        Some(PathBuf::from("/x/y.css"))
    );
}

#[test]
fn parse_page_size_accepts_letter_case_insensitively_and_rejects_others() {
    assert!(FmdConfig::parse("page_size=letter").is_ok());
    assert!(FmdConfig::parse("page_size=LETTER").is_ok());
    assert!(FmdConfig::parse("page_size= letter ").is_ok());
    let err = FmdConfig::parse("page_size=a4").expect_err("a4 is unsupported");
    assert_eq!(
        err.to_string(),
        "line 1: page_size currently supports only `letter`"
    );
}

#[test]
fn parse_rejects_unknown_key_and_lists_supported_keys() {
    let err = FmdConfig::parse("color=red").expect_err("unknown key is rejected");
    let msg = err.to_string();
    assert!(msg.contains("unknown config key `color`"), "{msg}");
    for &key in CONFIG_KEYS {
        assert!(msg.contains(key), "message should list `{key}`: {msg}");
    }
}

#[test]
fn font_parse_accepts_synonyms_and_rejects_others() {
    assert_eq!(
        FmdConfig::parse("font=sans").unwrap().font,
        Some(FontFamily::Sans)
    );
    assert_eq!(
        FmdConfig::parse("font=serif").unwrap().font,
        Some(FontFamily::Serif)
    );
    assert_eq!(
        FmdConfig::parse("font=sans-serif").unwrap().font,
        Some(FontFamily::Sans)
    );
    let err = FmdConfig::parse("font=comic").expect_err("unsupported font");
    assert_eq!(err.to_string(), "line 1: font must be `sans` or `serif`");
}

#[test]
fn dark_mode_accepts_documented_synonyms_and_rejects_others() {
    for on in ["auto", "on", "true", "AUTO", " On "] {
        let cfg = FmdConfig::parse(&format!("dark_mode={on}")).expect("on-synonym parses");
        assert_eq!(cfg.dark_mode, Some(DarkModePolicy::Auto), "input {on:?}");
    }
    for off in ["disabled", "off", "false", "none", "OFF"] {
        let cfg = FmdConfig::parse(&format!("dark_mode={off}")).expect("off-synonym parses");
        assert_eq!(
            cfg.dark_mode,
            Some(DarkModePolicy::Disabled),
            "input {off:?}"
        );
    }
    let err = FmdConfig::parse("dark_mode=maybe").expect_err("unknown dark_mode");
    assert_eq!(
        err.to_string(),
        "line 1: dark_mode must be `auto` or `disabled`"
    );
}

#[test]
fn set_margins_parse_resolve_and_reject_bad_values() {
    let cfg = FmdConfig::parse(
        "margin_top_pt=54.5\nmargin_right_pt=36\nmargin_bottom_pt=0\nmargin_left_pt=18.25\n",
    )
    .expect("valid margins parse");
    assert!(cfg.margins.is_some());
    // get_resolved formats via json_num (trailing-zero and trailing-dot trimming).
    assert_eq!(cfg.get_resolved("margin_top_pt").as_deref(), Some("54.5"));
    assert_eq!(cfg.get_resolved("margin_right_pt").as_deref(), Some("36"));
    assert_eq!(cfg.get_resolved("margin_bottom_pt").as_deref(), Some("0"));
    assert_eq!(cfg.get_resolved("margin_left_pt").as_deref(), Some("18.25"));

    let nan_err = FmdConfig::parse("margin_top_pt=abc").expect_err("not a number");
    assert_eq!(
        nan_err.to_string(),
        "line 1: margin value must be a number of points"
    );
    let neg_err = FmdConfig::parse("margin_left_pt=-1").expect_err("negative rejected");
    assert_eq!(
        neg_err.to_string(),
        "line 1: margin value must be finite and non-negative"
    );
    let inf_err = FmdConfig::parse("margin_right_pt=inf").expect_err("infinite rejected");
    assert_eq!(
        inf_err.to_string(),
        "line 1: margin value must be finite and non-negative"
    );
}

#[test]
fn to_theme_overlays_font_dark_mode_and_margins() {
    let margins = PageMargins {
        top_pt: 1.0,
        right_pt: 2.0,
        bottom_pt: 3.0,
        left_pt: 4.0,
    };
    let cfg = FmdConfig {
        font: Some(FontFamily::Serif),
        dark_mode: Some(DarkModePolicy::Disabled),
        custom_css: None,
        margins: Some(margins),
    };
    let theme = cfg.to_theme();
    assert_eq!(theme.font, FontFamily::Serif);
    assert_eq!(theme.dark_mode, DarkModePolicy::Disabled);
    assert_eq!(theme.page.margins, margins);

    // A default config leaves the theme defaults intact (the None branches).
    let default_theme = FmdConfig::default().to_theme();
    assert_eq!(default_theme.font, FontFamily::Sans);
    assert_eq!(default_theme.dark_mode, DarkModePolicy::Auto);
}

#[test]
fn get_resolved_covers_every_key_and_unknown_returns_none() {
    let cfg = FmdConfig {
        font: Some(FontFamily::Serif),
        dark_mode: Some(DarkModePolicy::Disabled),
        custom_css: Some(PathBuf::from("/x/y.css")),
        margins: Some(PageMargins {
            top_pt: 10.0,
            right_pt: 20.5,
            bottom_pt: 30.0,
            left_pt: 40.0,
        }),
    };
    assert_eq!(cfg.get_resolved("font").as_deref(), Some("serif"));
    assert_eq!(cfg.get_resolved("dark_mode").as_deref(), Some("disabled"));
    assert_eq!(cfg.get_resolved("custom_css").as_deref(), Some("/x/y.css"));
    assert_eq!(cfg.get_resolved("page_size").as_deref(), Some("letter"));
    assert_eq!(cfg.get_resolved("margin_top_pt").as_deref(), Some("10"));
    assert_eq!(cfg.get_resolved("margin_right_pt").as_deref(), Some("20.5"));
    assert_eq!(cfg.get_resolved("margin_bottom_pt").as_deref(), Some("30"));
    assert_eq!(cfg.get_resolved("margin_left_pt").as_deref(), Some("40"));
    // Key normalization: hyphenated/upper-case keys resolve identically.
    assert_eq!(cfg.get_resolved("MARGIN-TOP-PT").as_deref(), Some("10"));
    // Unknown key resolves to None.
    assert_eq!(cfg.get_resolved("nope"), None);
    // An unset custom_css resolves to the empty string, not None.
    assert_eq!(
        FmdConfig::default().get_resolved("custom_css").as_deref(),
        Some("")
    );
}

#[test]
fn to_file_string_serializes_all_fields_and_round_trips() {
    let cfg = FmdConfig {
        font: Some(FontFamily::Serif),
        dark_mode: Some(DarkModePolicy::Disabled),
        custom_css: Some(PathBuf::from("/themes/custom.css")),
        margins: Some(PageMargins {
            top_pt: 54.5,
            right_pt: 36.0,
            bottom_pt: 54.5,
            left_pt: 36.0,
        }),
    };
    let serialized = cfg.to_file_string();
    for expected in [
        "font=serif\n",
        "dark_mode=disabled\n",
        "custom_css=/themes/custom.css\n",
        "margin_top_pt=54.5\n",
        "margin_right_pt=36\n",
        "margin_bottom_pt=54.5\n",
        "margin_left_pt=36\n",
    ] {
        assert!(serialized.contains(expected), "missing line {expected:?}");
    }
    // Serializing then parsing yields an identical config.
    let reparsed = FmdConfig::parse(&serialized).expect("serialized config must parse");
    assert_eq!(reparsed, cfg);
    // For a representable config, the fallible variant agrees with the infallible one.
    assert_eq!(cfg.try_to_file_string().expect("representable"), serialized);
}

#[test]
fn to_json_escapes_control_and_quote_characters_in_custom_css() {
    let cfg = FmdConfig {
        custom_css: Some(PathBuf::from("q\"b\\c\nd\re\tf\u{0007}g")),
        ..FmdConfig::default()
    };
    let json = cfg.to_json();
    assert!(json.contains("q\\\"b"), "escaped quote: {json}");
    assert!(json.contains("b\\\\c"), "escaped backslash: {json}");
    assert!(json.contains("c\\nd"), "escaped newline: {json}");
    assert!(json.contains("d\\re"), "escaped carriage return: {json}");
    assert!(json.contains("e\\tf"), "escaped tab: {json}");
    assert!(json.contains("f g"), "control char becomes a space: {json}");
    assert!(
        json.contains("\"custom_css\":\"q\\\"b"),
        "custom_css field wrapped in quotes: {json}"
    );
}

#[test]
fn save_to_path_creates_missing_parent_directories_and_writes_file() {
    let base = temp_file("save-nested", "dir");
    let nested = base.join("a").join("b").join("config");
    let cfg = FmdConfig {
        font: Some(FontFamily::Serif),
        ..FmdConfig::default()
    };
    let written = cfg
        .save_to_path(&nested)
        .expect("save must create parents and write");
    assert_eq!(written, nested);
    assert!(nested.exists());
    let contents = fs::read_to_string(&nested).expect("written config is readable");
    assert_eq!(contents, "font=serif\n");
    assert_eq!(FmdConfig::parse(&contents).unwrap(), cfg);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn save_to_path_replaces_existing_config_without_temp_artifacts() {
    let base = temp_file("save-replace", "dir");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("config");
    fs::write(&path, "font=sans\n").unwrap();
    let cfg = FmdConfig {
        font: Some(FontFamily::Serif),
        dark_mode: Some(DarkModePolicy::Disabled),
        ..FmdConfig::default()
    };

    cfg.save_to_path(&path)
        .expect("save must replace an existing config file");

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "font=serif\ndark_mode=disabled\n"
    );
    assert!(
        fs::read_dir(&base).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.contains(".fmd-tmp") && !name.contains(".fmd-bak")
        }),
        "config save must not leave staged write artifacts"
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn save_to_path_rejects_directory_destination_before_staging() {
    let base = temp_file("save-dir-dest", "dir");
    let path = base.join("config");
    fs::create_dir_all(&path).unwrap();
    let cfg = FmdConfig {
        font: Some(FontFamily::Serif),
        ..FmdConfig::default()
    };

    let err = cfg
        .save_to_path(&path)
        .expect_err("a config path that is a directory must fail");

    assert!(matches!(err, ConfigError::Io(_)));
    assert!(path.is_dir(), "failed save must leave the directory intact");
    assert!(
        fs::read_dir(&base).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.contains(".fmd-tmp") && !name.contains(".fmd-bak")
        }),
        "preflight failure must not create staged write artifacts"
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn save_to_path_without_parent_writes_into_current_directory() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("fmd-config-bare-{}-{}.conf", std::process::id(), nanos);
    // A bare relative file name has an empty parent component, which the saver
    // filters out, so no directory is created.
    let bare = PathBuf::from(&name);
    let cfg = FmdConfig {
        dark_mode: Some(DarkModePolicy::Disabled),
        ..FmdConfig::default()
    };
    let written_ok = cfg.save_to_path(&bare).is_ok();
    let contents = fs::read_to_string(&bare).ok();
    let _ = fs::remove_file(&bare);
    assert!(
        written_ok,
        "a parent-less relative path must write without creating directories"
    );
    assert_eq!(contents.as_deref(), Some("dark_mode=disabled\n"));
}

// ---------------------------------------------------------------------------
// Env-dependent coverage via real child-process environments (config_path /
// load_default). These cannot be exercised in-process because the crate forbids
// `unsafe`, and edition-2024 `env::set_var` is `unsafe`.
// ---------------------------------------------------------------------------

#[test]
fn config_path_resolves_xdg_config_home_when_no_override() {
    let xdg = temp_file("xdg-root", "d");
    let xdg_s = xdg.display().to_string();
    let out = fmd_env(
        &["config", "path", "--json"],
        &[
            ("FMD_CONFIG", None),
            ("XDG_CONFIG_HOME", Some(xdg_s.as_str())),
            ("APPDATA", None),
        ],
    );
    assert!(out.status.success());
    let expected = xdg.join("fmd").join("config").display().to_string();
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains(&expected),
        "stdout {stdout} lacks {expected}"
    );
}

#[test]
fn config_path_falls_back_to_home_dot_config_when_xdg_absent() {
    let home = temp_file("home-root", "d");
    let home_s = home.display().to_string();
    let out = fmd_env(
        &["config", "path", "--json"],
        &[
            ("FMD_CONFIG", None),
            ("XDG_CONFIG_HOME", None),
            ("APPDATA", None),
            ("HOME", Some(home_s.as_str())),
        ],
    );
    assert!(out.status.success());
    let expected = home
        .join(".config")
        .join("fmd")
        .join("config")
        .display()
        .to_string();
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains(&expected),
        "stdout {stdout} lacks {expected}"
    );
}

#[test]
fn config_path_uses_relative_default_when_no_env_is_set() {
    let out = fmd_env(
        &["config", "path", "--json"],
        &[
            ("FMD_CONFIG", None),
            ("XDG_CONFIG_HOME", None),
            ("APPDATA", None),
            ("HOME", None),
        ],
    );
    assert!(out.status.success());
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("\"path\":\"fmd.config\""),
        "stdout: {stdout}"
    );
}

#[test]
fn config_show_reports_config_error_when_path_is_a_directory() {
    let dir = temp_file("dir-config", "d");
    fs::create_dir_all(&dir).unwrap();
    let out = fmd_with_config(&["config", "show", "--json"], &dir);
    assert_eq!(out.status.code(), Some(66));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"config_error\""),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reading config"), "stderr: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn config_get_unknown_key_reports_usage_error() {
    let config = temp_file("get-unknown", "conf");
    let _ = fs::remove_file(&config);
    let out = fmd_with_config(&["config", "get", "nope", "--json"], &config);
    assert_eq!(out.status.code(), Some(64));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"usage_error\""),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("unknown config key `nope`"),
        "stderr: {stderr}"
    );
    let _ = fs::remove_file(&config);
}

#[test]
fn config_set_is_rejected_with_no_config_flag() {
    let config = temp_file("no-config-set", "conf");
    let _ = fs::remove_file(&config);
    let out = fmd_with_config(
        &["config", "set", "font", "serif", "--no-config", "--json"],
        &config,
    );
    assert_eq!(out.status.code(), Some(64));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("\"code\":\"usage_error\""),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("cannot be combined with --no-config"),
        "stderr: {stderr}"
    );
    assert!(
        !config.exists(),
        "rejected set must not write a config file"
    );
    let _ = fs::remove_file(&config);
}

#[test]
fn config_set_and_get_margin_round_trips_through_real_file() {
    let config = temp_file("margin", "conf");
    let _ = fs::remove_file(&config);
    let set = fmd_with_config(
        &["config", "set", "margin_top_pt", "54.5", "--json"],
        &config,
    );
    assert!(set.status.success());
    assert!(text(&set.stdout).contains("\"value\":\"54.5\""));
    let on_disk = fs::read_to_string(&config).unwrap();
    assert!(
        on_disk.contains("margin_top_pt=54.5"),
        "config file: {on_disk}"
    );
    let get = fmd_with_config(&["config", "get", "margin_top_pt", "--json"], &config);
    assert!(get.status.success());
    assert!(text(&get.stdout).contains("\"value\":\"54.5\""));
    let _ = fs::remove_file(&config);
}
