//! The `fmd` command-line surface (only compiled with the `cli` feature). This
//! is the single shared entrypoint for both the long-name binary and the short
//! `fmd` alias.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};

use crate::config::{CONFIG_KEYS, FmdConfig, config_path};
use crate::{FontFamily, HtmlOptions, PdfOptions, RenderError, Theme, render_html, render_pdf};

/// franken_markdown — Markdown to beautiful all-in-one HTML & tiny PDF.
#[derive(Parser)]
#[command(
    name = "fmd",
    version,
    about,
    long_about = "fmd converts Markdown files, stdin, or raw Markdown text into attractive self-contained HTML and compact deterministic PDF. The PDF path embeds curated per-document font subsets today; LaTeX-grade paragraph and page layout are still landing behind the same command contract.\n\nFirst tries that work:\n  fmd README.md\n  fmd - < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd render README.md --to both --out README.html\n  fmd config show --json\n  fmd capabilities --json\n  fmd robot-docs guide\n  fmd --robot-triage"
)]
struct Cli {
    /// Emit stable machine-readable JSON for command metadata/status.
    #[arg(long, global = true)]
    json: bool,
    /// Disable human color/decorative terminal output. Accepted for env parity;
    /// current output is already plain.
    #[arg(long, global = true)]
    no_color: bool,
    /// Ignore native config files for this invocation.
    #[arg(long, global = true)]
    no_config: bool,
    /// Print one machine-readable triage envelope: quick reference, health,
    /// commands, and next recommended actions.
    #[arg(long, global = true)]
    robot_triage: bool,
    /// Command to run. If omitted, fmd prints help and exits successfully.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Render a Markdown file (or stdin) to HTML and/or PDF.
    Render(RenderArgs),
    /// Print the stable machine-readable command and feature contract.
    Capabilities,
    /// Print in-tool documentation written for coding agents.
    RobotDocs(RobotDocsArgs),
    /// Check local build/runtime capabilities and report implementation status.
    Doctor(DoctorArgs),
    /// Read or edit native fmd config (never used by the WASM/core library).
    Config(ConfigArgs),
}

#[derive(Args)]
struct RenderArgs {
    /// Input `.md` path, or `-` to read Markdown from stdin. If omitted, use
    /// `--text` or stdin.
    input: Option<String>,
    /// Raw Markdown text to render directly.
    #[arg(long)]
    text: Option<String>,
    /// Which output(s) to produce.
    #[arg(long, value_enum, default_value_t = Target::Html)]
    to: Target,
    /// Output path. For HTML-only with no path, writes to stdout. For `both`,
    /// the extension is swapped per format.
    #[arg(long, short)]
    out: Option<PathBuf>,
    /// Override the configured/default body font.
    #[arg(long, value_enum)]
    font: Option<FontArg>,
    /// Path to a custom stylesheet that fully replaces the default theme CSS.
    #[arg(long)]
    css: Option<PathBuf>,
    /// Document title (defaults to the first heading).
    #[arg(long)]
    title: Option<String>,
    /// Pass raw HTML in the source through instead of escaping it.
    #[arg(long)]
    allow_html: bool,
    /// Emit a stable JSON status envelope to stderr after writing outputs.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RobotDocsArgs {
    #[command(subcommand)]
    command: Option<RobotDocsCommand>,
}

#[derive(Subcommand)]
enum RobotDocsCommand {
    /// Print the coding-agent quick guide.
    Guide,
}

#[derive(Args)]
struct DoctorArgs {
    /// Emit a stable JSON health report.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: Option<ConfigCommand>,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Show the resolved native config and equivalent theme.
    Show(ConfigShowArgs),
    /// Print the resolved value for one key.
    Get(ConfigGetArgs),
    /// Set one key in the native config file.
    Set(ConfigSetArgs),
    /// Print the native config path.
    Path(ConfigPathArgs),
}

#[derive(Args)]
struct ConfigShowArgs {
    /// Emit stable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ConfigGetArgs {
    /// Config key to read.
    key: String,
    /// Emit stable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ConfigSetArgs {
    /// Config key to write.
    key: String,
    /// Config value to write.
    value: String,
    /// Emit stable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ConfigPathArgs {
    /// Emit stable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Copy, Clone, ValueEnum)]
enum Target {
    Html,
    Pdf,
    Both,
}

#[derive(Copy, Clone, ValueEnum)]
enum FontArg {
    Sans,
    Serif,
}

impl From<FontArg> for FontFamily {
    fn from(f: FontArg) -> Self {
        match f {
            FontArg::Sans => FontFamily::Sans,
            FontArg::Serif => FontFamily::Serif,
        }
    }
}

/// Entry point shared by `src/main.rs` and `src/bin/fmd.rs`.
#[must_use]
pub fn main() -> ExitCode {
    let cli = match Cli::try_parse_from(normalized_args()) {
        Ok(cli) => cli,
        Err(err) => return handle_parse_error(err),
    };
    let json = cli.json;
    let _no_color = cli.no_color;
    let no_config = cli.no_config;
    if cli.robot_triage {
        print_robot_triage();
        return ExitCode::SUCCESS;
    }
    match cli.command {
        Some(Command::Render(args)) => run_render(args, json, no_config),
        Some(Command::Capabilities) => {
            print_capabilities();
            ExitCode::SUCCESS
        }
        Some(Command::RobotDocs(args)) => {
            let _guide = args.command.unwrap_or(RobotDocsCommand::Guide);
            print_robot_docs();
            ExitCode::SUCCESS
        }
        Some(Command::Doctor(args)) => run_doctor(json || args.json),
        Some(Command::Config(args)) => run_config(args, json, no_config),
        None => {
            let mut cmd = Cli::command();
            if cmd.print_long_help().is_err() {
                return fail(74, "writing help to stdout");
            }
            println!();
            ExitCode::SUCCESS
        }
    }
}

fn run_render(args: RenderArgs, global_json: bool, no_config: bool) -> ExitCode {
    let json = global_json || args.json;
    let src = match read_input(args.input.as_deref(), args.text.as_deref()) {
        Ok(s) => s,
        Err(e) => return fail_json(66, "input_error", &format!("reading input: {e}"), json),
    };

    let config = match load_config(no_config) {
        Ok(config) => config,
        Err(e) => return fail_json(66, "config_error", &format!("reading config: {e}"), json),
    };

    let mut theme = config.to_theme();
    if let Some(font) = args.font {
        theme = theme.with_font(font.into());
    }

    let css_path = args.css.clone().or_else(|| config.custom_css.clone());
    let custom_css = match css_path.as_deref() {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(s) => Some(s),
            Err(e) => {
                return fail_json(
                    66,
                    "input_error",
                    &format!("reading stylesheet {}: {e}", p.display()),
                    json,
                );
            }
        },
        None => None,
    };

    let want_html = matches!(args.to, Target::Html | Target::Both);
    let want_pdf = matches!(args.to, Target::Pdf | Target::Both);
    let single = !matches!(args.to, Target::Both);

    if want_html {
        let opts = HtmlOptions {
            theme: theme.clone(),
            title: args.title.clone(),
            custom_css: custom_css.clone(),
            allow_raw_html: args.allow_html,
        };
        match render_html(&src, &opts) {
            Ok(html) => {
                let bytes = html.into_bytes();
                match out_path(&args, single, "html") {
                    Some(path) => {
                        if let Err(e) = std::fs::write(&path, &bytes) {
                            return fail_json(
                                73,
                                "output_error",
                                &format!("writing {}: {e}", path.display()),
                                json,
                            );
                        }
                        report_write("html", &path, bytes.len(), json);
                    }
                    None => {
                        if let Err(e) = std::io::stdout().write_all(&bytes) {
                            return fail_json(
                                74,
                                "output_error",
                                &format!("writing stdout: {e}"),
                                json,
                            );
                        }
                    }
                }
            }
            Err(e) => return fail_render(e, json),
        }
    }

    if want_pdf {
        let opts = PdfOptions {
            theme: theme.clone(),
            title: args.title.clone(),
            allow_raw_html: args.allow_html,
        };
        match render_pdf(&src, &opts) {
            Ok(bytes) => match out_path(&args, single, "pdf") {
                Some(path) => {
                    if let Err(e) = std::fs::write(&path, &bytes) {
                        return fail_json(
                            73,
                            "output_error",
                            &format!("writing {}: {e}", path.display()),
                            json,
                        );
                    }
                    report_write("pdf", &path, bytes.len(), json);
                }
                None => {
                    return fail_json(64, "usage_error", "PDF output requires --out <path>", json);
                }
            },
            // Keep render errors typed with a distinct exit code (70 = render
            // failure/unavailable subsystem) as richer PDF validation lands.
            Err(e) => return fail_render(e, json),
        }
    }

    ExitCode::SUCCESS
}

fn run_config(args: ConfigArgs, global_json: bool, no_config: bool) -> ExitCode {
    let command = args
        .command
        .unwrap_or(ConfigCommand::Show(ConfigShowArgs { json: false }));
    match command {
        ConfigCommand::Show(args) => {
            let json = global_json || args.json;
            let config = match load_config(no_config) {
                Ok(config) => config,
                Err(e) => {
                    return fail_json(66, "config_error", &format!("reading config: {e}"), json);
                }
            };
            print_config_show(&config, json);
            ExitCode::SUCCESS
        }
        ConfigCommand::Get(args) => {
            let json = global_json || args.json;
            let config = match load_config(no_config) {
                Ok(config) => config,
                Err(e) => {
                    return fail_json(66, "config_error", &format!("reading config: {e}"), json);
                }
            };
            let Some(value) = config.get_resolved(&args.key) else {
                return fail_json(
                    64,
                    "usage_error",
                    &format!(
                        "unknown config key `{}`; supported keys: {}",
                        args.key,
                        CONFIG_KEYS.join(", ")
                    ),
                    json,
                );
            };
            if json {
                println!(
                    "{{\"ok\":true,\"key\":\"{}\",\"value\":\"{}\",\"path\":\"{}\"}}",
                    json_escape(&args.key),
                    json_escape(&value),
                    json_escape(&config_path().display().to_string())
                );
            } else {
                println!("{value}");
            }
            ExitCode::SUCCESS
        }
        ConfigCommand::Set(args) => {
            let json = global_json || args.json;
            if no_config {
                return fail_json(
                    64,
                    "usage_error",
                    "`config set` cannot be combined with --no-config",
                    json,
                );
            }
            let mut config = match FmdConfig::load_default() {
                Ok(config) => config,
                Err(e) => {
                    return fail_json(66, "config_error", &format!("reading config: {e}"), json);
                }
            };
            if let Err(e) = config.set_key_value(&args.key, &args.value) {
                return fail_json(64, "usage_error", &e, json);
            }
            let path = match config.save_default() {
                Ok(path) => path,
                Err(e) => {
                    return fail_json(73, "config_error", &format!("writing config: {e}"), json);
                }
            };
            let value = config.get_resolved(&args.key).unwrap_or_default();
            if json {
                println!(
                    "{{\"ok\":true,\"event\":\"config_set\",\"key\":\"{}\",\"value\":\"{}\",\"path\":\"{}\"}}",
                    json_escape(&args.key),
                    json_escape(&value),
                    json_escape(&path.display().to_string())
                );
            } else {
                println!("fmd: set {}={} in {}", args.key, value, path.display());
            }
            ExitCode::SUCCESS
        }
        ConfigCommand::Path(args) => {
            let json = global_json || args.json;
            let path = config_path();
            if json {
                println!(
                    "{{\"ok\":true,\"path\":\"{}\"}}",
                    json_escape(&path.display().to_string())
                );
            } else {
                println!("{}", path.display());
            }
            ExitCode::SUCCESS
        }
    }
}

fn print_config_show(config: &FmdConfig, json: bool) {
    let path = config_path();
    if json {
        println!(
            "{{\"ok\":true,\"path\":\"{}\",\"config\":{},\"theme\":{}}}",
            json_escape(&path.display().to_string()),
            config.to_json(),
            config.to_theme().to_config_json()
        );
    } else {
        println!("fmd config");
        println!("  path: {}", path.display());
        for key in CONFIG_KEYS {
            if let Some(value) = config.get_resolved(key) {
                println!("  {key}: {value}");
            }
        }
    }
}

fn load_config(no_config: bool) -> std::result::Result<FmdConfig, crate::config::ConfigError> {
    if no_config {
        Ok(FmdConfig::default())
    } else {
        FmdConfig::load_default()
    }
}

fn read_input(input: Option<&str>, text: Option<&str>) -> std::io::Result<String> {
    if let Some(raw) = text {
        return Ok(raw.to_string());
    }
    if input == Some("-") || input.is_none() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        std::fs::read_to_string(input.unwrap_or_default())
    }
}

/// Compute the output path for a given extension, or `None` to mean stdout
/// (only valid for a single HTML target with no `--out`).
fn out_path(args: &RenderArgs, single: bool, ext: &str) -> Option<PathBuf> {
    if let Some(p) = &args.out {
        if single {
            return Some(p.clone());
        }
        return Some(p.with_extension(ext));
    }
    if single && ext == "html" {
        return None; // stdout
    }
    // Derive from the input filename when no --out was given.
    let stem = if args.input.as_deref() == Some("-") || args.input.is_none() {
        Path::new("document")
    } else {
        Path::new(args.input.as_deref().unwrap_or("document"))
    };
    Some(stem.with_extension(ext))
}

fn fail(code: u8, msg: &str) -> ExitCode {
    eprintln!("fmd: {msg}");
    ExitCode::from(code)
}

fn handle_parse_error(err: clap::Error) -> ExitCode {
    let kind = err.kind();
    if err.print().is_err() {
        return fail(74, "writing command-line diagnostics");
    }
    match kind {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
        _ => {
            eprintln!(
                "fmd: try `fmd --help`, `fmd capabilities --json`, or `fmd robot-docs guide`."
            );
            ExitCode::from(64)
        }
    }
}

fn fail_json(code: u8, err_code: &str, msg: &str, json: bool) -> ExitCode {
    if json {
        eprintln!(
            "{{\"ok\":false,\"error\":{{\"code\":\"{}\",\"message\":\"{}\"}},\"exit_code\":{}}}",
            json_escape(err_code),
            json_escape(msg),
            code
        );
    } else {
        eprintln!("fmd: {msg}");
    }
    ExitCode::from(code)
}

fn fail_render(err: RenderError, json: bool) -> ExitCode {
    fail_json(70, err.code(), &err.to_string(), json)
}

fn report_write(kind: &str, path: &Path, bytes: usize, json: bool) {
    if json {
        eprintln!(
            "{{\"ok\":true,\"event\":\"wrote\",\"format\":\"{}\",\"path\":\"{}\",\"bytes\":{}}}",
            kind,
            json_escape(&path.display().to_string()),
            bytes
        );
    } else {
        eprintln!("fmd: wrote {} ({} bytes)", path.display(), bytes);
    }
}

fn run_doctor(json: bool) -> ExitCode {
    if json {
        println!(
            "{{\"ok\":true,\"tool\":\"fmd\",\"version\":\"{}\",\"engine\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"syntax_highlighting\":\"available\",\"wasm_core\":\"no-default-features\"}},\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"dependency_posture\":{{\"core\":\"std-only\",\"cli\":\"clap\"}},\"license\":\"LicenseRef-MIT-OpenAI-Anthropic-Rider\"}}",
            env!("CARGO_PKG_VERSION"),
            Theme::default().to_config_json()
        );
    } else {
        println!("fmd doctor");
        println!("  html: available");
        println!("  pdf: available v0 (embedded subset fonts, deterministic writer)");
        println!("  syntax highlighting: available for common documentation languages");
        println!("  theme model: structured v1");
        println!("  core dependencies: std-only");
        println!("  cli dependency: clap");
        println!("  wasm posture: core builds with --no-default-features");
        println!("  license: MIT with OpenAI/Anthropic rider");
    }
    ExitCode::SUCCESS
}

fn print_capabilities() {
    println!(
        "{{\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"commands\":[{{\"name\":\"render\",\"examples\":[\"fmd README.md\",\"fmd - < README.md\",\"fmd --text '# Hello' --out hello.html\",\"fmd render README.md --to both --out README.html\",\"fmd README.md --to pdf --out README.pdf\"]}},{{\"name\":\"config\",\"examples\":[\"fmd config show --json\",\"fmd config set font serif --json\",\"fmd --no-config README.md --out README.html\"]}},{{\"name\":\"capabilities\",\"examples\":[\"fmd capabilities --json\"]}},{{\"name\":\"robot-docs guide\",\"examples\":[\"fmd robot-docs guide\"]}},{{\"name\":\"doctor\",\"examples\":[\"fmd doctor --json\"]}},{{\"name\":\"--robot-triage\",\"examples\":[\"fmd --robot-triage\"]}}],\"outputs\":[\"html\",\"pdf\",\"both\"],\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"exit_codes\":{{\"0\":\"success\",\"64\":\"usage error\",\"66\":\"input error\",\"70\":\"render unavailable or failed\",\"73\":\"output file error\",\"74\":\"stdout/write error\"}},\"features\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"raw_text\":\"available\",\"stdin\":\"available\",\"custom_css\":\"available\",\"native_config\":\"available\",\"no_config\":\"available\",\"font_sans_serif_toggle\":\"available\",\"shared_theme_model\":\"structured_v1\",\"syntax_highlighting\":\"available\",\"embedded_subset_fonts_pdf\":\"available\",\"gpos_kerning_pdf\":\"available_focused\",\"gsub_ligatures_pdf\":\"available_focused\",\"knuth_plass_pdf\":\"planned\",\"hyphenation_pdf\":\"planned\",\"page_builder_pdf\":\"planned\",\"stream_compression_pdf\":\"planned\",\"robot_triage\":\"available\",\"wasm_core\":\"no-default-features available\"}}}}",
        env!("CARGO_PKG_VERSION"),
        Theme::default().to_config_json()
    );
}

fn print_robot_triage() {
    println!(
        "{{\"ok\":true,\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"quick_ref\":[\"fmd README.md --out README.html\",\"fmd README.md --to pdf --out README.pdf\",\"fmd --text '# Hello' --out hello.html\",\"fmd config show --json\",\"fmd capabilities --json\",\"fmd doctor --json\"],\"health\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"syntax_highlighting\":\"available\",\"theme_model\":\"structured_v1\",\"native_config\":\"available\",\"wasm_core\":\"no-default-features\"}},\"recommended_next_actions\":[{{\"command\":\"fmd capabilities --json\",\"reason\":\"discover the stable command and exit-code contract\"}},{{\"command\":\"fmd config show --json\",\"reason\":\"inspect native defaults without reading external docs\"}},{{\"command\":\"fmd robot-docs guide\",\"reason\":\"read the in-tool agent guide\"}},{{\"command\":\"fmd README.md --out README.html --json\",\"reason\":\"render HTML and receive machine-readable write status on stderr\"}},{{\"command\":\"fmd README.md --to pdf --out README.pdf --json\",\"reason\":\"render the current embedded-font PDF v0 and receive machine-readable write status on stderr\"}}]}}",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_robot_docs() {
    println!(
        "fmd agent guide\n\nCanonical commands:\n  fmd README.md --out README.html\n  fmd README.md --to pdf --out README.pdf\n  fmd - --out stdin.html < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd render README.md --to both --out README.html\n  fmd config show --json\n  fmd config set font serif --json\n  fmd --no-config README.md --out README.html\n  fmd capabilities --json\n  fmd doctor --json\n  fmd --robot-triage\n\nRules for agents:\n  stdout is document data for HTML-to-stdout and JSON data for capabilities/doctor/config/robot-triage.\n  diagnostics and write confirmations go to stderr.\n  use --json on render when you need machine-readable status events on stderr.\n  PDF output is available as a compact deterministic v0 with embedded per-document font subsets, real metrics, focused GPOS kerning, GSUB ligatures, and selectable text; Knuth-Plass paragraph layout, hyphenation, page-builder polish, and compression are still planned.\n  Use --css <file> for a full custom stylesheet replacement, --font serif for one render, config set font serif for a persistent native default, and --no-config for reproducible config-free runs."
    );
}

fn normalized_args() -> Vec<String> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return args;
    }

    normalize_agent_typos(&mut args);

    let known = [
        "render",
        "capabilities",
        "robot-docs",
        "doctor",
        "config",
        "help",
    ];
    let global_no_value = ["--json", "--no-color", "--no-config", "--robot-triage"];
    let global_with_value: [&str; 0] = [];
    let root_flags = ["--help", "-h", "--version", "-V"];

    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        if root_flags.contains(&arg) || known.contains(&arg) {
            return args;
        }
        if global_no_value.contains(&arg) {
            i += 1;
            continue;
        }
        if global_with_value.contains(&arg) {
            i += 2;
            continue;
        }
        args.insert(i, "render".to_string());
        return args;
    }
    args
}

fn normalize_agent_typos(args: &mut [String]) {
    for arg in args.iter_mut().skip(1) {
        match arg.as_str() {
            "--jsno" | "--jsoon" | "--jason" | "--json=true" => *arg = "--json".to_string(),
            "--no-colour" | "--colour=never" | "--color=never" => {
                *arg = "--no-color".to_string();
            }
            _ => {}
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
