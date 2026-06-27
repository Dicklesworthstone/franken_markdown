//! The `fmd` command-line surface (only compiled with the `cli` feature). This
//! is the single shared entrypoint for both the long-name binary and the short
//! `fmd` alias.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};

use crate::{FontFamily, HtmlOptions, PdfOptions, RenderError, Theme, render_html, render_pdf};

/// franken_markdown — Markdown to beautiful all-in-one HTML & tiny PDF.
#[derive(Parser)]
#[command(
    name = "fmd",
    version,
    about,
    long_about = "fmd converts Markdown files, stdin, or raw Markdown text into attractive self-contained HTML today and optimized PDF as the clean-room layout engine lands.\n\nFirst tries that work:\n  fmd README.md\n  fmd - < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd render README.md --to both --out README.html\n  fmd capabilities --json\n  fmd robot-docs guide"
)]
struct Cli {
    /// Emit stable machine-readable JSON for command metadata/status.
    #[arg(long, global = true)]
    json: bool,
    /// Disable human color/decorative terminal output. Accepted for env parity;
    /// current output is already plain.
    #[arg(long, global = true)]
    no_color: bool,
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
    /// Default body font.
    #[arg(long, value_enum, default_value_t = FontArg::Sans)]
    font: FontArg,
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
    let cli = Cli::parse_from(normalized_args());
    let json = cli.json;
    let _no_color = cli.no_color;
    match cli.command {
        Some(Command::Render(args)) => run_render(args, json),
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
        None => {
            let mut cmd = Cli::command();
            if cmd.print_help().is_err() {
                return fail(74, "writing help to stdout");
            }
            println!();
            ExitCode::SUCCESS
        }
    }
}

fn run_render(args: RenderArgs, global_json: bool) -> ExitCode {
    let json = global_json || args.json;
    let src = match read_input(args.input.as_deref(), args.text.as_deref()) {
        Ok(s) => s,
        Err(e) => return fail_json(66, "input_error", &format!("reading input: {e}"), json),
    };

    let theme = Theme {
        font: args.font.into(),
        ..Theme::default()
    };

    let custom_css = match args.css.as_deref() {
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
            // The PDF subsystem is still being built; surface it as a typed,
            // non-crashing refusal with a distinct exit code (70 = unavailable).
            Err(e) => return fail_render(e, json),
        }
    }

    ExitCode::SUCCESS
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
            "{{\"ok\":true,\"tool\":\"fmd\",\"version\":\"{}\",\"engine\":{{\"html\":\"available\",\"pdf\":\"planned\",\"wasm_core\":\"no-default-features\"}},\"dependency_posture\":{{\"core\":\"std-only\",\"cli\":\"clap\"}},\"license\":\"LicenseRef-MIT-OpenAI-Anthropic-Rider\"}}",
            env!("CARGO_PKG_VERSION")
        );
    } else {
        println!("fmd doctor");
        println!("  html: available");
        println!("  pdf: planned (text/layout/font/PDF writer beads)");
        println!("  core dependencies: std-only");
        println!("  cli dependency: clap");
        println!("  wasm posture: core builds with --no-default-features");
        println!("  license: MIT with OpenAI/Anthropic rider");
    }
    ExitCode::SUCCESS
}

fn print_capabilities() {
    println!(
        "{{\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"commands\":[{{\"name\":\"render\",\"examples\":[\"fmd README.md\",\"fmd - < README.md\",\"fmd --text '# Hello' --out hello.html\",\"fmd render README.md --to both --out README.html\"]}},{{\"name\":\"capabilities\",\"examples\":[\"fmd capabilities --json\"]}},{{\"name\":\"robot-docs guide\",\"examples\":[\"fmd robot-docs guide\"]}},{{\"name\":\"doctor\",\"examples\":[\"fmd doctor --json\"]}}],\"outputs\":[\"html\",\"pdf\",\"both\"],\"exit_codes\":{{\"0\":\"success\",\"64\":\"usage error\",\"66\":\"input error\",\"70\":\"render unavailable or failed\",\"73\":\"output file error\",\"74\":\"stdout/write error\"}},\"features\":{{\"html\":\"available\",\"pdf\":\"planned\",\"raw_text\":\"available\",\"stdin\":\"available\",\"custom_css\":\"available\",\"font_sans_serif_toggle\":\"available\",\"syntax_highlighting\":\"planned\",\"knuth_plass_pdf\":\"planned\",\"font_subsetting_pdf\":\"planned\",\"wasm_core\":\"planned via --no-default-features\"}}}}",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_robot_docs() {
    println!(
        "fmd agent guide\n\nCanonical commands:\n  fmd README.md --out README.html\n  fmd - --out stdin.html < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd render README.md --to both --out README.html\n  fmd capabilities --json\n  fmd doctor --json\n\nRules for agents:\n  stdout is document data for HTML-to-stdout and JSON data for capabilities/doctor.\n  diagnostics and write confirmations go to stderr.\n  use --json on render when you need machine-readable status events on stderr.\n  PDF currently refuses with exit 70 until the clean-room text/layout/PDF pipeline lands.\n  Use --css <file> for a full custom stylesheet replacement and --font serif for long-form prose."
    );
}

fn normalized_args() -> Vec<String> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return args;
    }

    let known = ["render", "capabilities", "robot-docs", "doctor", "help"];
    let global_no_value = ["--json", "--no-color"];
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
