//! The `fmd` command-line surface (only compiled with the `cli` feature). This
//! is the single shared entrypoint for both the long-name binary and the short
//! `fmd` alias.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::{render_html, render_pdf, FontFamily, HtmlOptions, PdfOptions, Theme};

/// franken_markdown — Markdown to beautiful all-in-one HTML & tiny PDF.
#[derive(Parser)]
#[command(name = "fmd", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a Markdown file (or stdin) to HTML and/or PDF.
    Render(RenderArgs),
}

#[derive(Args)]
struct RenderArgs {
    /// Input `.md` path, or `-` to read Markdown from stdin.
    input: String,
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
    let cli = Cli::parse();
    match cli.command {
        Command::Render(args) => run_render(args),
    }
}

fn run_render(args: RenderArgs) -> ExitCode {
    let src = match read_input(&args.input) {
        Ok(s) => s,
        Err(e) => return fail(66, &format!("reading input: {e}")),
    };

    let theme = Theme { font: args.font.into(), ..Theme::default() };

    let custom_css = match args.css.as_deref() {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(s) => Some(s),
            Err(e) => return fail(66, &format!("reading stylesheet {}: {e}", p.display())),
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
                            return fail(73, &format!("writing {}: {e}", path.display()));
                        }
                        eprintln!("fmd: wrote {} ({} bytes)", path.display(), bytes.len());
                    }
                    None => {
                        if let Err(e) = std::io::stdout().write_all(&bytes) {
                            return fail(74, &format!("writing stdout: {e}"));
                        }
                    }
                }
            }
            Err(e) => return fail(70, &e.to_string()),
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
                        return fail(73, &format!("writing {}: {e}", path.display()));
                    }
                    eprintln!("fmd: wrote {} ({} bytes)", path.display(), bytes.len());
                }
                None => return fail(64, "PDF output requires --out <path>"),
            },
            // The PDF subsystem is still being built; surface it as a typed,
            // non-crashing refusal with a distinct exit code (70 = unavailable).
            Err(e) => return fail(70, &e.to_string()),
        }
    }

    ExitCode::SUCCESS
}

fn read_input(input: &str) -> std::io::Result<String> {
    if input == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        std::fs::read_to_string(input)
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
    let stem = if args.input == "-" {
        Path::new("document")
    } else {
        Path::new(&args.input)
    };
    Some(stem.with_extension(ext))
}

fn fail(code: u8, msg: &str) -> ExitCode {
    eprintln!("fmd: {msg}");
    ExitCode::from(code)
}
