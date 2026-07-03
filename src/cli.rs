//! The `fmd` command-line surface (only compiled with the `cli` feature). This
//! is the single shared entrypoint for both the long-name binary and the short
//! `fmd` alias.

use std::io::{Error, ErrorKind as IoErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};

use crate::config::{CONFIG_KEYS, FmdConfig, config_path};
use crate::{
    FontAssets, FontFamily, HtmlOptions, PdfImageAsset, PdfOptions, RenderError, Theme,
    parse_markdown, render_html, render_pdf, render_warnings,
};

const DEFAULT_MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_PDF_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// franken_markdown — Markdown to beautiful all-in-one HTML & tiny PDF.
#[derive(Parser)]
#[command(
    name = "fmd",
    version,
    about,
    long_about = "fmd converts Markdown files, stdin, or raw Markdown text into attractive self-contained HTML and compact deterministic PDF. The PDF path embeds curated per-document font subsets, uses Knuth-Plass paragraph breaking, applies deterministic discretionary hyphenation/justification for body paragraphs, and includes basic keep/widow pagination today; deeper page polish is still landing behind the same command contract.\n\nFirst tries that work:\n  fmd README.md\n  fmd - < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd --text '# Hello' --out - > hello.html\n  fmd render README.md --to both --out README.html\n  fmd config show --json\n  fmd capabilities --json\n  fmd robot-docs guide\n  fmd --robot-triage"
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
    /// Render many Markdown inputs in parallel under a bounded worker budget
    /// (native-only; Asupersync-backed). See docs/BATCH_ORCHESTRATION.md.
    #[cfg(feature = "batch")]
    Batch(BatchArgs),
}

#[cfg(feature = "batch")]
#[derive(Args)]
struct BatchArgs {
    /// Markdown files and/or directories (recursed for `*.md`/`*.markdown`).
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    /// Which output(s) to produce for every input.
    #[arg(long, value_enum, default_value_t = Target::Html)]
    to: Target,
    /// Directory for outputs (default: alongside each input).
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// Worker cap (default: derive from CPUs and the batch mode).
    #[arg(long)]
    workers: Option<usize>,
    /// Sizing mode: `interactive` reserves CPU headroom; `throughput` uses all.
    #[arg(long, value_enum, default_value_t = BatchModeArg::Interactive)]
    batch_mode: BatchModeArg,
    /// Soft memory ceiling in bytes. Enforced as a static concurrency cap
    /// (bytes / 64 MiB-per-job), NOT by measuring real resident memory.
    #[arg(long)]
    mem_budget: Option<u64>,
    /// Wall-clock deadline in seconds (best-effort). It is only checked at per-file
    /// boundaries — the render core never checkpoints mid-file — so a single large
    /// file runs to completion before the deadline can stop the remaining files.
    /// When it fires, not-yet-started files are skipped and the receipt is marked
    /// `cancelled`.
    #[arg(long)]
    timeout: Option<u64>,
    /// Refuse any single input larger than this many bytes (default 64 MiB),
    /// recording it as a failed entry, so a large tree cannot exhaust memory.
    #[arg(long, default_value_t = DEFAULT_MAX_INPUT_BYTES)]
    max_input_bytes: u64,
    /// Record per-file failures in the receipt instead of failing the run.
    #[arg(long)]
    continue_on_error: bool,
    /// Override the configured/default body font.
    #[arg(long, value_enum)]
    font: Option<FontArg>,
    /// Custom stylesheet that fully replaces the default theme CSS (HTML).
    #[arg(long)]
    css: Option<PathBuf>,
    /// Emit the machine-readable batch receipt JSON to stdout.
    #[arg(long)]
    json: bool,
}

#[cfg(feature = "batch")]
#[derive(Clone, Copy, clap::ValueEnum)]
enum BatchModeArg {
    Interactive,
    Throughput,
}

#[cfg(feature = "batch")]
impl From<BatchModeArg> for crate::batch::BatchMode {
    fn from(m: BatchModeArg) -> Self {
        match m {
            BatchModeArg::Interactive => crate::batch::BatchMode::Interactive,
            BatchModeArg::Throughput => crate::batch::BatchMode::Throughput,
        }
    }
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
    /// Document author metadata for PDF output.
    #[arg(long)]
    author: Option<String>,
    /// Pass raw HTML in the source through instead of escaping it.
    #[arg(long)]
    allow_html: bool,
    /// Render muted line numbers in PDF fenced code blocks.
    #[arg(long)]
    pdf_line_numbers: bool,
    /// Provide a local PDF image asset as MARKDOWN_DEST=PATH. The render core
    /// never fetches or reads files; this native CLI flag resolves bytes before
    /// rendering. Repeat for multiple images.
    #[arg(long = "pdf-image", value_name = "DEST=PATH")]
    pdf_images: Vec<String>,
    /// Maximum bytes accepted for each `--pdf-image` file before rendering.
    #[arg(long, default_value_t = DEFAULT_MAX_PDF_IMAGE_BYTES)]
    max_pdf_image_bytes: u64,
    /// Maximum Markdown input bytes accepted before rendering.
    #[arg(long, default_value_t = DEFAULT_MAX_INPUT_BYTES)]
    max_input_bytes: u64,
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
        return print_robot_triage();
    }
    match cli.command {
        Some(Command::Render(args)) => run_render(args, json, no_config),
        Some(Command::Capabilities) => print_capabilities(),
        Some(Command::RobotDocs(args)) => {
            let _guide = args.command.unwrap_or(RobotDocsCommand::Guide);
            print_robot_docs()
        }
        Some(Command::Doctor(args)) => run_doctor(json || args.json),
        Some(Command::Config(args)) => run_config(args, json, no_config),
        #[cfg(feature = "batch")]
        Some(Command::Batch(args)) => run_batch(args, json, no_config),
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
    if out_is_stdout(&args) && !matches!(args.to, Target::Html) {
        return fail_json(
            64,
            "usage_error",
            "`--out -` writes HTML to stdout only; PDF and --to both require a real output path",
            json,
        );
    }
    let src = match read_input(
        args.input.as_deref(),
        args.text.as_deref(),
        args.max_input_bytes,
    ) {
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

    // Refuse to overwrite the input file. `read_input` already slurped the
    // source into memory, so writing an output onto the same path would
    // silently destroy the user's Markdown (e.g. `fmd README.md --out
    // README.md`, `fmd notes.md --to pdf --out notes.md`, or a `.md` file
    // misnamed `doc.pdf` rendered with `--to pdf`). Check every file target
    // before writing any of them so we fail fast with nothing written.
    let mut file_targets: Vec<PathBuf> = Vec::new();
    if want_html && let Some(p) = out_path(&args, single, "html") {
        file_targets.push(p);
    }
    if want_pdf && let Some(p) = out_path(&args, single, "pdf") {
        file_targets.push(p);
    }
    if let Some(clash) = find_input_overwrite(args.input.as_deref(), &file_targets) {
        return fail_json(
            64,
            "usage_error",
            &format!(
                "refusing to overwrite the input file {} with rendered output; write to a different --out path",
                clash.display()
            ),
            json,
        );
    }

    let pdf_metadata_epoch = if want_pdf {
        match source_date_epoch() {
            Ok(epoch) => epoch,
            Err(e) => return fail_json(64, "usage_error", &e, json),
        }
    } else {
        None
    };
    let pdf_image_assets = if want_pdf {
        match read_pdf_image_assets(&args.pdf_images, args.max_pdf_image_bytes) {
            Ok(assets) => assets,
            // A malformed `--pdf-image` spec is a usage error (64); a missing/
            // unreadable/oversized file is an input error (66).
            Err(PdfImageError::Usage(e)) => return fail_json(64, "usage_error", &e, json),
            Err(PdfImageError::Input(e)) => return fail_json(66, "input_error", &e, json),
        }
    } else {
        Vec::new()
    };

    // Render every requested format to bytes BEFORE writing any of them, so a
    // `--to both` run whose PDF render fails never leaves a stale HTML file on
    // disk (previously HTML was written, then a PDF failure returned exit 70
    // with the HTML already committed).
    let html_bytes = if want_html {
        let opts = HtmlOptions {
            theme: theme.clone(),
            title: args.title.clone(),
            custom_css: custom_css.clone(),
            allow_raw_html: args.allow_html,
            font_assets: FontAssets::default(),
        };
        match render_html(&src, &opts) {
            Ok(html) => Some(html.into_bytes()),
            Err(e) => return fail_render(e, json),
        }
    } else {
        None
    };

    let pdf_render = if want_pdf {
        let opts = PdfOptions {
            theme: theme.clone(),
            title: args.title.clone(),
            author: args.author.clone(),
            metadata_epoch_seconds: pdf_metadata_epoch,
            allow_raw_html: args.allow_html,
            code_line_numbers: args.pdf_line_numbers,
            image_assets: pdf_image_assets,
            font_assets: FontAssets::default(),
        };
        match render_pdf(&src, &opts) {
            // Keep render errors typed with a distinct exit code (70 = render
            // failure/unavailable subsystem) as richer PDF validation lands.
            Ok(bytes) => Some((opts, bytes)),
            Err(e) => return fail_render(e, json),
        }
    } else {
        None
    };

    let html_path = html_bytes
        .as_ref()
        .and_then(|_| out_path(&args, single, "html"));
    let pdf_path = if pdf_render.is_some() {
        match out_path(&args, single, "pdf") {
            Some(path) => Some(path),
            None => return fail_json(64, "usage_error", "PDF output requires --out <path>", json),
        }
    } else {
        None
    };

    let mut file_outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (html_path.as_deref(), html_bytes.as_deref()) {
        file_outputs.push(crate::file_write::OutputFile { path, bytes });
    }
    if let (Some(path), Some((_, bytes))) = (pdf_path.as_deref(), pdf_render.as_ref()) {
        file_outputs.push(crate::file_write::OutputFile { path, bytes });
    }
    if let Err(err) = crate::file_write::write_outputs_staged(&file_outputs) {
        return fail_json(
            73,
            "output_error",
            &format!("writing {}: {}", err.path.display(), err.source),
            json,
        );
    }

    if let (Some(path), Some(bytes)) = (html_path.as_deref(), html_bytes.as_deref()) {
        report_write("html", path, bytes.len(), json);
    } else if let Some(bytes) = html_bytes.as_deref() {
        let mut stdout = std::io::stdout().lock();
        match stdout.write_all(bytes) {
            Ok(()) => {}
            // The reader closed early (e.g. `fmd doc.md | head`). A broken
            // pipe is a clean exit, matching `emit_stdout` for the
            // discovery/config commands — the "stdout is data, exit codes
            // are stable when piped" contract must hold for the primary
            // rendered-document path too, not just metadata output.
            Err(e) if e.kind() == IoErrorKind::BrokenPipe => {}
            Err(e) => {
                return fail_json(74, "output_error", &format!("writing stdout: {e}"), json);
            }
        }
    }

    if let Some((opts, bytes)) = pdf_render.as_ref()
        && let Some(path) = pdf_path.as_deref()
    {
        report_pdf_warnings(&src, opts, json);
        report_write("pdf", path, bytes.len(), json);
    }

    ExitCode::SUCCESS
}

#[cfg(feature = "batch")]
fn run_batch(args: BatchArgs, global_json: bool, no_config: bool) -> ExitCode {
    use crate::batch::{self, BatchOptions, BatchPlan, OutputFormat};

    let json = global_json || args.json;

    // `--workers 0` would otherwise collapse into "unset" (automatic sizing);
    // reject it explicitly so the flag never silently means the opposite.
    if args.workers == Some(0) {
        return fail_json(
            64,
            "usage_error",
            "--workers must be at least 1 (omit --workers for automatic sizing)",
            json,
        );
    }

    // Batch cannot stream, so `--out-dir -` is meaningless; refuse it (as the
    // docs promise) instead of silently creating a directory literally named
    // `-` and writing every output into it.
    if args.out_dir.as_deref() == Some(Path::new("-")) {
        return fail_json(
            64,
            "usage_error",
            "--out-dir '-' is not valid (batch writes files, it cannot stream); pass a real directory or omit --out-dir",
            json,
        );
    }

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

    let format = match args.to {
        Target::Html => OutputFormat::Html,
        Target::Pdf => OutputFormat::Pdf,
        Target::Both => OutputFormat::Both,
    };
    // Only PDF output consults SOURCE_DATE_EPOCH, so an HTML-only batch must not
    // fail on a malformed value it never uses (matches single-render behavior).
    let want_pdf = matches!(format, OutputFormat::Pdf | OutputFormat::Both);
    let pdf_epoch = if want_pdf {
        match source_date_epoch() {
            Ok(epoch) => epoch,
            Err(e) => return fail_json(64, "usage_error", &e, json),
        }
    } else {
        None
    };

    let continue_on_error = args.continue_on_error;
    let batch::ExpandedInputs {
        inputs,
        errors: expand_errors,
    } = batch::expand_inputs(&args.inputs);

    // In strict mode (the default) any unexpandable path aborts the whole run
    // (exit 66), as before. With --continue-on-error the bad paths are recorded
    // as per-file failures in the receipt and the valid files still render.
    if !continue_on_error && let Some(first) = expand_errors.first() {
        return fail_json(
            66,
            "input_error",
            &format!("expanding {}: {}", first.path.display(), first.message),
            json,
        );
    }
    if inputs.is_empty() {
        let msg = if expand_errors.is_empty() {
            "no Markdown inputs found (files/dirs expanded to nothing)".to_string()
        } else {
            format!(
                "no readable Markdown inputs ({} path(s) could not be expanded)",
                expand_errors.len()
            )
        };
        return fail_json(66, "input_error", &msg, json);
    }

    let html = HtmlOptions {
        theme: theme.clone(),
        title: None,
        custom_css,
        allow_raw_html: false,
        font_assets: FontAssets::default(),
    };
    let pdf = PdfOptions {
        theme,
        title: None,
        author: None,
        metadata_epoch_seconds: pdf_epoch,
        allow_raw_html: false,
        code_line_numbers: false,
        image_assets: Vec::new(),
        font_assets: FontAssets::default(),
    };

    let plan = BatchPlan {
        inputs,
        format,
        out_dir: args.out_dir.clone(),
    };
    let opts = BatchOptions {
        html,
        pdf,
        mode: args.batch_mode.into(),
        workers: args.workers,
        mem_budget: args.mem_budget,
        continue_on_error,
        timeout_secs: args.timeout,
        max_input_bytes: args.max_input_bytes,
    };

    match batch::run_batch_blocking(plan, &opts) {
        Ok(mut receipt) => {
            // Record any unexpandable paths as per-file failures. Strict mode
            // already returned above, so this only runs under --continue-on-error
            // (or when there were no expansion errors at all).
            for e in &expand_errors {
                receipt.files.push(batch::FileEntry::expansion_failure(
                    &e.path,
                    e.message.clone(),
                ));
            }
            // stdout is data (the receipt JSON) only with --json; otherwise a
            // human summary goes to stderr and stdout stays empty. A broken pipe
            // is swallowed here so it never panics or overrides the batch result
            // exit code computed below.
            if json {
                let _ = emit_stdout(&receipt.to_json());
            } else {
                eprintln!(
                    "fmd batch: {} ok, {} failed, {} skipped across {} input(s) on {} worker(s)",
                    receipt.ok_count(),
                    receipt.failed_count(),
                    receipt.skipped_count(),
                    receipt.files.len(),
                    receipt.workers,
                );
            }
            let total = receipt.files.len();
            // A cancelled run (the `--timeout` deadline fired) leaves not-yet-started
            // inputs *skipped*, not rendered. The documented contract is "0 = all
            // inputs rendered", so a partial cancellation (some ok, none failed, but
            // work skipped) must not report success — otherwise an agent keying on the
            // exit code alone believes the batch finished. A fully-cancelled run
            // already exits non-zero via `ok_count() == 0`; this covers the partial case.
            let hard_failure = (!continue_on_error && receipt.failed_count() > 0)
                || (total > 0 && receipt.ok_count() == 0)
                || receipt.cancelled;
            if hard_failure {
                // Return the documented exit code for the FIRST failure's
                // category, so agents keying on exit codes get the same
                // 66/70/73 distinction as a single render instead of a blanket
                // 70 (docs/BATCH_ORCHESTRATION.md). No typed failure (e.g. an
                // all-skipped cancelled run) falls back to 70.
                let code = match receipt.first_failure_kind() {
                    Some(batch::FileErrorKind::Input) => 66,
                    Some(batch::FileErrorKind::Output) => 73,
                    Some(batch::FileErrorKind::Render) | None => 70,
                };
                ExitCode::from(code)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => fail_json(70, "render_error", &format!("batch failed: {e}"), json),
    }
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
            print_config_show(&config, json)
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
            let out = if json {
                format!(
                    "{{\"ok\":true,\"key\":\"{}\",\"value\":\"{}\",\"path\":\"{}\"}}",
                    json_escape(&args.key),
                    json_escape(&value),
                    json_escape(&config_path().display().to_string())
                )
            } else {
                value
            };
            emit_stdout(&out)
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
            let out = if json {
                format!(
                    "{{\"ok\":true,\"event\":\"config_set\",\"key\":\"{}\",\"value\":\"{}\",\"path\":\"{}\"}}",
                    json_escape(&args.key),
                    json_escape(&value),
                    json_escape(&path.display().to_string())
                )
            } else {
                format!("fmd: set {}={} in {}", args.key, value, path.display())
            };
            emit_stdout(&out)
        }
        ConfigCommand::Path(args) => {
            let json = global_json || args.json;
            let path = config_path();
            let out = if json {
                format!(
                    "{{\"ok\":true,\"path\":\"{}\"}}",
                    json_escape(&path.display().to_string())
                )
            } else {
                path.display().to_string()
            };
            emit_stdout(&out)
        }
    }
}

fn print_config_show(config: &FmdConfig, json: bool) -> ExitCode {
    let path = config_path();
    let out = if json {
        format!(
            "{{\"ok\":true,\"path\":\"{}\",\"config\":{},\"theme\":{}}}",
            json_escape(&path.display().to_string()),
            config.to_json(),
            config.to_theme().to_config_json()
        )
    } else {
        let mut lines = vec![
            "fmd config".to_string(),
            format!("  path: {}", path.display()),
        ];
        for key in CONFIG_KEYS {
            if let Some(value) = config.get_resolved(key) {
                lines.push(format!("  {key}: {value}"));
            }
        }
        lines.join("\n")
    };
    emit_stdout(&out)
}

fn load_config(no_config: bool) -> std::result::Result<FmdConfig, crate::config::ConfigError> {
    if no_config {
        Ok(FmdConfig::default())
    } else {
        FmdConfig::load_default()
    }
}

fn read_input(input: Option<&str>, text: Option<&str>, max_bytes: u64) -> std::io::Result<String> {
    if let Some(raw) = text {
        if raw.len() as u64 > max_bytes {
            return Err(input_too_large(
                "raw --text input",
                raw.len() as u64,
                max_bytes,
            ));
        }
        return Ok(raw.to_string());
    }
    if input == Some("-") || input.is_none() {
        let stdin = std::io::stdin();
        let bytes = read_limited(stdin.lock(), max_bytes, "stdin input")?;
        string_from_input_bytes(bytes)
    } else {
        let path = input.unwrap_or_default();
        let label = format!("input file {path}");
        if let Ok(meta) = std::fs::metadata(path)
            && meta.len() > max_bytes
        {
            return Err(input_too_large(&label, meta.len(), max_bytes));
        }
        let file = std::fs::File::open(path)?;
        let bytes = read_limited(file, max_bytes, &label)?;
        string_from_input_bytes(bytes)
    }
}

fn read_limited<R: Read>(reader: R, max_bytes: u64, label: &str) -> std::io::Result<Vec<u8>> {
    read_limited_with_flag(reader, max_bytes, label, "--max-input-bytes")
}

fn read_limited_with_flag<R: Read>(
    reader: R,
    max_bytes: u64,
    label: &str,
    flag: &str,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut limited = reader.take(max_bytes.saturating_add(1));
    limited.read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(size_limit_error(label, bytes.len() as u64, max_bytes, flag));
    }
    Ok(bytes)
}

fn string_from_input_bytes(bytes: Vec<u8>) -> std::io::Result<String> {
    String::from_utf8(bytes)
        .map_err(|e| Error::new(IoErrorKind::InvalidData, format!("input is not UTF-8: {e}")))
}

fn input_too_large(label: &str, observed: u64, max_bytes: u64) -> Error {
    size_limit_error(label, observed, max_bytes, "--max-input-bytes")
}

fn pdf_image_too_large(label: &str, observed: u64, max_bytes: u64) -> Error {
    size_limit_error(label, observed, max_bytes, "--max-pdf-image-bytes")
}

fn size_limit_error(label: &str, observed: u64, max_bytes: u64, flag: &str) -> Error {
    Error::new(
        IoErrorKind::InvalidData,
        format!("{label} is {observed} bytes; exceeds {flag} {max_bytes}"),
    )
}

/// A `--pdf-image` failure, tagged with the exit-code category it maps to: a
/// malformed spec is a usage error (64), while a missing/unreadable/oversized
/// file is an input error (66).
enum PdfImageError {
    Usage(String),
    Input(String),
}

fn read_pdf_image_assets(
    specs: &[String],
    max_bytes: u64,
) -> std::result::Result<Vec<PdfImageAsset>, PdfImageError> {
    let mut assets = Vec::with_capacity(specs.len());
    for spec in specs {
        let (destination, path) = parse_pdf_image_spec(spec).map_err(PdfImageError::Usage)?;
        let label = format!("PDF image asset {destination} from {}", path.display());
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.len() > max_bytes
        {
            return Err(PdfImageError::Input(
                pdf_image_too_large(&label, meta.len(), max_bytes).to_string(),
            ));
        }
        let file = std::fs::File::open(&path)
            .map_err(|e| PdfImageError::Input(format!("reading {label}: {e}")))?;
        let bytes = read_limited_with_flag(file, max_bytes, &label, "--max-pdf-image-bytes")
            .map_err(|e| PdfImageError::Input(format!("reading {label}: {e}")))?;
        assets.push(PdfImageAsset::new(destination, bytes));
    }
    Ok(assets)
}

fn parse_pdf_image_spec(spec: &str) -> std::result::Result<(String, PathBuf), String> {
    let Some((dest, path)) = split_pdf_image_spec(spec) else {
        return Err(format!(
            "invalid --pdf-image {spec:?}; expected MARKDOWN_DEST=PATH, for example --pdf-image images/chart.png=./chart.png"
        ));
    };
    let dest = dest.trim();
    let path = path.trim();
    if dest.is_empty() {
        return Err("invalid --pdf-image: MARKDOWN_DEST must not be blank".to_string());
    }
    if path.is_empty() {
        return Err("invalid --pdf-image: PATH must not be blank".to_string());
    }
    Ok((dest.to_string(), PathBuf::from(path)))
}

fn split_pdf_image_spec(spec: &str) -> Option<(&str, &str)> {
    for (idx, _) in spec.match_indices('=') {
        let dest = &spec[..idx];
        let path = &spec[idx + 1..];
        if dest.trim().is_empty() || path.trim().is_empty() {
            continue;
        }
        if std::fs::metadata(path.trim()).is_ok() {
            return Some((dest, path));
        }
    }
    spec.rsplit_once('=')
}

fn source_date_epoch() -> std::result::Result<Option<u64>, String> {
    let raw = match std::env::var_os("SOURCE_DATE_EPOCH") {
        Some(raw) => raw,
        None => return Ok(None),
    };
    let Some(raw) = raw.to_str() else {
        return Err(
            "SOURCE_DATE_EPOCH must be UTF-8 decimal seconds since the Unix epoch".to_string(),
        );
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(
            "SOURCE_DATE_EPOCH must be non-negative decimal seconds since the Unix epoch"
                .to_string(),
        );
    }
    trimmed.parse::<u64>().map(Some).map_err(|_| {
        "SOURCE_DATE_EPOCH is too large; expected decimal seconds since the Unix epoch".to_string()
    })
}

/// Compute the output path for a given extension, or `None` to mean stdout
/// (only valid for a single HTML target with no `--out`).
fn out_path(args: &RenderArgs, single: bool, ext: &str) -> Option<PathBuf> {
    if let Some(p) = &args.out {
        if single && ext == "html" && is_stdout_path(p) {
            return None;
        }
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

fn out_is_stdout(args: &RenderArgs) -> bool {
    args.out.as_deref().is_some_and(is_stdout_path)
}

fn is_stdout_path(path: &Path) -> bool {
    path == Path::new("-")
}

/// Return the first `output` path that names the same existing on-disk file as
/// `input`, or `None` if writing every output is safe. stdin (`-`) and `--text`
/// (`None`) have no source file to clobber.
fn find_input_overwrite(input: Option<&str>, outputs: &[PathBuf]) -> Option<PathBuf> {
    let input = input.filter(|p| *p != "-")?;
    let input = Path::new(input);
    outputs.iter().find(|out| same_file(input, out)).cloned()
}

/// True iff `a` and `b` name the same existing on-disk file. On Unix this
/// compares the resolved `(device, inode)` pair — which catches hard links,
/// symlinks, `./x` vs `x`, and case-insensitive aliases; elsewhere it compares
/// canonicalized paths. A path that does not exist yet can never be the input we
/// just read, so a missing file compares as "not the same".
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        }
    }
}

fn fail(code: u8, msg: &str) -> ExitCode {
    eprintln!("fmd: {msg}");
    ExitCode::from(code)
}

/// Write `text` plus a trailing newline to stdout, returning the process exit
/// code. A broken pipe — the reader closed early, e.g. `fmd capabilities --json
/// | head` — exits cleanly (0) instead of the panic `println!` would raise, so
/// the "stdout is data, exit codes are stable" contract survives piping. Any
/// other write failure is a stdout/write error (74). Equivalent to
/// `println!("{text}")` byte-for-byte on success.
fn emit_stdout(text: &str) -> ExitCode {
    let mut out = std::io::stdout().lock();
    match out
        .write_all(text.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == IoErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(74),
    }
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

/// Print non-fatal PDF render warnings (degraded content that would otherwise be
/// dropped silently) so they are never invisible. In `--json` mode each warning
/// is its own JSONL object before the `wrote` envelope; otherwise a plain line.
fn report_pdf_warnings(src: &str, opts: &PdfOptions, json: bool) {
    let doc = parse_markdown(src);
    for warning in render_warnings(&doc, opts) {
        if json {
            eprintln!(
                "{{\"ok\":true,\"event\":\"warning\",\"warning\":\"{}\",\"detail\":\"{}\"}}",
                warning.code(),
                json_escape(&warning.message())
            );
        } else {
            eprintln!("fmd: warning: {}", warning.message());
        }
    }
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
    let out = if json {
        format!(
            "{{\"ok\":true,\"tool\":\"fmd\",\"version\":\"{}\",\"engine\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"syntax_highlighting\":\"available\",\"wasm_core\":\"no-default-features\"}},\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"dependency_posture\":{{\"core\":\"std-only\",\"cli\":\"clap\"}},\"license\":\"LicenseRef-MIT-OpenAI-Anthropic-Rider\"}}",
            env!("CARGO_PKG_VERSION"),
            Theme::default().to_config_json()
        )
    } else {
        [
            "fmd doctor",
            "  html: available",
            "  pdf: available v0 (embedded subset fonts, deterministic writer, hyphenation)",
            "  syntax highlighting: available for common documentation languages",
            "  theme model: structured v1",
            "  core dependencies: std-only",
            "  cli dependency: clap",
            "  wasm posture: core builds with --no-default-features",
            "  license: MIT with OpenAI/Anthropic rider",
        ]
        .join("\n")
    };
    emit_stdout(&out)
}

fn print_capabilities() -> ExitCode {
    emit_stdout(&format!(
        "{{\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"commands\":[{{\"name\":\"render\",\"examples\":[\"fmd README.md\",\"fmd - < README.md\",\"fmd --text '# Hello' --out hello.html\",\"fmd --text '# Hello' --out - > hello.html\",\"fmd render README.md --to both --out README.html\",\"fmd README.md --to pdf --out README.pdf\",\"fmd README.md --to pdf --pdf-line-numbers --out README.pdf\",\"fmd README.md --to pdf --pdf-image images/chart.png=./chart.png --out README.pdf\",\"fmd README.md --to pdf --title 'Quarterly Memo' --author 'FMD' --out README.pdf\",\"SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf\",\"fmd --max-input-bytes 1048576 README.md --out README.html\"]}},{{\"name\":\"config\",\"examples\":[\"fmd config show --json\",\"fmd config set font serif --json\",\"fmd --no-config README.md --out README.html\"]}},{{\"name\":\"capabilities\",\"examples\":[\"fmd capabilities --json\"]}},{{\"name\":\"robot-docs guide\",\"examples\":[\"fmd robot-docs guide\"]}},{{\"name\":\"doctor\",\"examples\":[\"fmd doctor --json\"]}},{{\"name\":\"--robot-triage\",\"examples\":[\"fmd --robot-triage\"]}}],\"outputs\":[\"html\",\"pdf\",\"both\"],\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"exit_codes\":{{\"0\":\"success\",\"64\":\"usage error\",\"66\":\"input error\",\"70\":\"render unavailable or failed\",\"73\":\"output file error\",\"74\":\"stdout/write error\"}},\"features\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"raw_text\":\"available\",\"stdin\":\"available\",\"html_stdout_dash\":\"available\",\"pdf_stdout_dash\":\"refused_usage_error\",\"pdf_default_output_path\":\"available_derived_from_input_stem\",\"custom_css\":\"available\",\"native_config\":\"available\",\"no_config\":\"available\",\"input_size_limit\":\"available\",\"pdf_image_assets\":\"available_png_v0\",\"font_sans_serif_toggle\":\"available\",\"shared_theme_model\":\"structured_v1\",\"syntax_highlighting\":\"available\",\"pdf_code_line_numbers\":\"available\",\"pdf_metadata\":\"available\",\"source_date_epoch_pdf\":\"available\",\"tagged_pdf\":\"available_hierarchical_accessible\",\"font_subsetting_pdf\":\"available\",\"embedded_subset_fonts_pdf\":\"available\",\"gpos_kerning_pdf\":\"available_focused\",\"gsub_ligatures_pdf\":\"available_focused\",\"knuth_plass_pdf\":\"available\",\"hyphenation_pdf\":\"available_discretionary_body_paragraphs\",\"pdf_justification\":\"available_body_paragraphs\",\"page_builder_pdf\":\"available_v0_keep_widow\",\"stream_compression_pdf\":\"available\",\"robot_triage\":\"available\",\"wasm_core\":\"no-default-features available\",\"wasm_browser_package\":\"publishable_unpublished\",\"commonmark_spec\":\"0.31.2_ratcheted_min_379_of_652_normalized\"}}}}",
        env!("CARGO_PKG_VERSION"),
        Theme::default().to_config_json()
    ))
}

fn print_robot_triage() -> ExitCode {
    emit_stdout(&format!(
        "{{\"ok\":true,\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"quick_ref\":[\"fmd README.md --out README.html\",\"fmd README.md --to pdf --out README.pdf\",\"fmd --text '# Hello' --out hello.html\",\"fmd --text '# Hello' --out - > hello.html\",\"fmd config show --json\",\"fmd capabilities --json\",\"fmd doctor --json\"],\"health\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"syntax_highlighting\":\"available\",\"theme_model\":\"structured_v1\",\"native_config\":\"available\",\"wasm_core\":\"no-default-features\"}},\"recommended_next_actions\":[{{\"command\":\"fmd capabilities --json\",\"reason\":\"discover the stable command and exit-code contract\"}},{{\"command\":\"fmd config show --json\",\"reason\":\"inspect native defaults without reading external docs\"}},{{\"command\":\"fmd robot-docs guide\",\"reason\":\"read the in-tool agent guide\"}},{{\"command\":\"fmd README.md --out README.html --json\",\"reason\":\"render HTML and receive machine-readable write status on stderr\"}},{{\"command\":\"fmd README.md --to pdf --out README.pdf --json\",\"reason\":\"render the current embedded-font PDF v0 and receive machine-readable write status on stderr\"}}]}}",
        env!("CARGO_PKG_VERSION")
    ))
}

fn print_robot_docs() -> ExitCode {
    emit_stdout(
        "fmd agent guide\n\nCanonical commands:\n  fmd README.md --out README.html\n  fmd README.md --to pdf --out README.pdf\n  fmd README.md --to pdf --pdf-line-numbers --out README.pdf\n  fmd README.md --to pdf --pdf-image images/chart.png=./chart.png --out README.pdf\n  fmd README.md --to pdf --title 'Quarterly Memo' --author 'FMD' --out README.pdf\n  SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf\n  fmd --max-input-bytes 1048576 README.md --out README.html\n  fmd - --out stdin.html < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd --text '# Hello' --out - > hello.html\n  fmd render README.md --to both --out README.html\n  fmd config show --json\n  fmd config set font serif --json\n  fmd --no-config README.md --out README.html\n  fmd capabilities --json\n  fmd doctor --json\n  fmd --robot-triage\n\nRules for agents:\n  stdout is document data for HTML-to-stdout and JSON data for capabilities/doctor/config/robot-triage.\n  `--out -` writes HTML document data to stdout only; PDF and --to both require a real output path.\n  diagnostics and write confirmations go to stderr.\n  use --json on render when you need machine-readable status events on stderr.\n  --max-input-bytes caps file/stdin/--text ingress before parsing; oversized input exits 66 with no document data on stdout.\n  --pdf-image maps one Markdown image destination to a local file as DEST=PATH for PDF rendering; repeat it for multiple images. The core never fetches network images or reads files itself.\n  PDF output is available as a compact deterministic v0 with embedded per-document font subsets, real metrics, focused GPOS kerning, GSUB ligatures, Knuth-Plass paragraph layout, deterministic discretionary hyphenation and glue justification for body paragraphs, basic keep/widow page building, syntax-highlighted wrapped code blocks, optional --pdf-line-numbers, local PNG image assets via --pdf-image, PDF metadata via --title/--author/SOURCE_DATE_EPOCH, a hierarchical accessible tagged-PDF structure tree (Document root, per-cell tables with header column scope, nested lists, blockquotes, figures with alt/bbox, links referenced via /OBJR, decoration as /Artifact), selectable text, and FlateDecode-compressed large page streams; deeper page-builder polish is still planned.\n  Use --css <file> for a full custom stylesheet replacement, --font serif for one render, config set font serif for a persistent native default, and --no-config for reproducible config-free runs.",
    )
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
        // Recognized even without the `batch` feature so it is never rewritten to
        // `render batch ...`; clap then reports a clean "unrecognized subcommand".
        "batch",
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod overwrite_guard_tests {
    use super::find_input_overwrite;
    use std::path::PathBuf;

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("fmd_overwrite_test_{}_{tag}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn refuses_when_an_output_equals_the_input() {
        let dir = tmp_dir("same");
        let input = dir.join("doc.md");
        std::fs::write(&input, b"# hi").unwrap();
        let clash = find_input_overwrite(input.to_str(), std::slice::from_ref(&input));
        assert_eq!(clash.as_deref(), Some(input.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn allows_distinct_and_not_yet_existing_output_paths() {
        let dir = tmp_dir("diff");
        let input = dir.join("doc.md");
        std::fs::write(&input, b"# hi").unwrap();
        let html = dir.join("doc.html"); // distinct, exists
        std::fs::write(&html, b"x").unwrap();
        let pdf = dir.join("doc.pdf"); // does not exist yet
        assert!(find_input_overwrite(input.to_str(), &[html, pdf]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_overwrite_through_a_relative_alias() {
        // A `dir/./doc.md` alias resolves to the same file as the input.
        let dir = tmp_dir("alias");
        let input = dir.join("doc.md");
        std::fs::write(&input, b"# hi").unwrap();
        let aliased = dir.join(".").join("doc.md");
        assert!(find_input_overwrite(input.to_str(), &[aliased]).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn detects_overwrite_through_a_hard_link() {
        // A hard link is a distinct path but the SAME file (inode) — writing to
        // it destroys the source, so the guard must catch it (path comparison
        // alone would miss it).
        let dir = tmp_dir("hardlink");
        let input = dir.join("doc.md");
        std::fs::write(&input, b"# hi").unwrap();
        let link = dir.join("alias.md");
        std::fs::hard_link(&input, &link).unwrap();
        assert!(
            find_input_overwrite(input.to_str(), std::slice::from_ref(&link)).is_some(),
            "a hard link to the input must be treated as the same file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stdin_and_text_inputs_have_no_file_to_clobber() {
        let outputs = [PathBuf::from("out.html")];
        assert!(find_input_overwrite(Some("-"), &outputs).is_none()); // stdin
        assert!(find_input_overwrite(None, &outputs).is_none()); // --text
    }

    #[test]
    fn a_nonexistent_input_path_is_never_a_clash() {
        // Exercises the `canonicalize(input)` failure arm: a missing input can't
        // be overwritten, so the guard stays out of the way.
        let outputs = [PathBuf::from("out.html")];
        assert!(find_input_overwrite(Some("/no/such/fmd/input/doc.md"), &outputs).is_none());
    }
}
