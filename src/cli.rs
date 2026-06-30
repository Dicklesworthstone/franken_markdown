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
    /// Soft memory ceiling in bytes (bounds concurrent renders).
    #[arg(long)]
    mem_budget: Option<u64>,
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
            Err(e) => return fail_json(66, "input_error", &e, json),
        }
    } else {
        Vec::new()
    };

    if want_html {
        let opts = HtmlOptions {
            theme: theme.clone(),
            title: args.title.clone(),
            custom_css: custom_css.clone(),
            allow_raw_html: args.allow_html,
            font_assets: FontAssets::default(),
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
            author: args.author.clone(),
            metadata_epoch_seconds: pdf_metadata_epoch,
            allow_raw_html: args.allow_html,
            code_line_numbers: args.pdf_line_numbers,
            image_assets: pdf_image_assets,
            font_assets: FontAssets::default(),
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
                    report_pdf_warnings(&src, &opts, json);
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

#[cfg(feature = "batch")]
fn run_batch(args: BatchArgs, global_json: bool, no_config: bool) -> ExitCode {
    use crate::batch::{self, BatchOptions, BatchPlan, OutputFormat};

    let json = global_json || args.json;

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
    let pdf_epoch = match source_date_epoch() {
        Ok(epoch) => epoch,
        Err(e) => return fail_json(64, "usage_error", &e, json),
    };

    let inputs = match batch::expand_inputs(&args.inputs) {
        Ok(found) if !found.is_empty() => found,
        Ok(_) => {
            return fail_json(
                66,
                "input_error",
                "no Markdown inputs found (files/dirs expanded to nothing)",
                json,
            );
        }
        Err(e) => return fail_json(66, "input_error", &format!("expanding inputs: {e}"), json),
    };

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

    let continue_on_error = args.continue_on_error;
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
    };

    match batch::run_batch_blocking(plan, &opts) {
        Ok(receipt) => {
            // stdout is data (the receipt JSON) only with --json; otherwise a
            // human summary goes to stderr and stdout stays empty.
            if json {
                println!("{}", receipt.to_json());
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
            let hard_failure = (!continue_on_error && receipt.failed_count() > 0)
                || (total > 0 && receipt.ok_count() == 0);
            if hard_failure {
                ExitCode::from(70)
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

fn read_pdf_image_assets(
    specs: &[String],
    max_bytes: u64,
) -> std::result::Result<Vec<PdfImageAsset>, String> {
    let mut assets = Vec::with_capacity(specs.len());
    for spec in specs {
        let (destination, path) = parse_pdf_image_spec(spec)?;
        let label = format!("PDF image asset {destination} from {}", path.display());
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.len() > max_bytes
        {
            return Err(pdf_image_too_large(&label, meta.len(), max_bytes).to_string());
        }
        let file = std::fs::File::open(&path).map_err(|e| format!("reading {label}: {e}"))?;
        let bytes = read_limited_with_flag(file, max_bytes, &label, "--max-pdf-image-bytes")
            .map_err(|e| format!("reading {label}: {e}"))?;
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
    if json {
        println!(
            "{{\"ok\":true,\"tool\":\"fmd\",\"version\":\"{}\",\"engine\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"syntax_highlighting\":\"available\",\"wasm_core\":\"no-default-features\"}},\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"dependency_posture\":{{\"core\":\"std-only\",\"cli\":\"clap\"}},\"license\":\"LicenseRef-MIT-OpenAI-Anthropic-Rider\"}}",
            env!("CARGO_PKG_VERSION"),
            Theme::default().to_config_json()
        );
    } else {
        println!("fmd doctor");
        println!("  html: available");
        println!("  pdf: available v0 (embedded subset fonts, deterministic writer, hyphenation)");
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
        "{{\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"commands\":[{{\"name\":\"render\",\"examples\":[\"fmd README.md\",\"fmd - < README.md\",\"fmd --text '# Hello' --out hello.html\",\"fmd --text '# Hello' --out - > hello.html\",\"fmd render README.md --to both --out README.html\",\"fmd README.md --to pdf --out README.pdf\",\"fmd README.md --to pdf --pdf-line-numbers --out README.pdf\",\"fmd README.md --to pdf --pdf-image images/chart.png=./chart.png --out README.pdf\",\"fmd README.md --to pdf --title 'Quarterly Memo' --author 'FMD' --out README.pdf\",\"SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf\",\"fmd --max-input-bytes 1048576 README.md --out README.html\"]}},{{\"name\":\"config\",\"examples\":[\"fmd config show --json\",\"fmd config set font serif --json\",\"fmd --no-config README.md --out README.html\"]}},{{\"name\":\"capabilities\",\"examples\":[\"fmd capabilities --json\"]}},{{\"name\":\"robot-docs guide\",\"examples\":[\"fmd robot-docs guide\"]}},{{\"name\":\"doctor\",\"examples\":[\"fmd doctor --json\"]}},{{\"name\":\"--robot-triage\",\"examples\":[\"fmd --robot-triage\"]}}],\"outputs\":[\"html\",\"pdf\",\"both\"],\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"exit_codes\":{{\"0\":\"success\",\"64\":\"usage error\",\"66\":\"input error\",\"70\":\"render unavailable or failed\",\"73\":\"output file error\",\"74\":\"stdout/write error\"}},\"features\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"raw_text\":\"available\",\"stdin\":\"available\",\"html_stdout_dash\":\"available\",\"pdf_stdout_dash\":\"refused_usage_error\",\"pdf_default_output_path\":\"available_derived_from_input_stem\",\"custom_css\":\"available\",\"native_config\":\"available\",\"no_config\":\"available\",\"input_size_limit\":\"available\",\"pdf_image_assets\":\"available_png_v0\",\"font_sans_serif_toggle\":\"available\",\"shared_theme_model\":\"structured_v1\",\"syntax_highlighting\":\"available\",\"pdf_code_line_numbers\":\"available\",\"pdf_metadata\":\"available\",\"source_date_epoch_pdf\":\"available\",\"tagged_pdf\":\"available_hierarchical_accessible\",\"font_subsetting_pdf\":\"available\",\"embedded_subset_fonts_pdf\":\"available\",\"gpos_kerning_pdf\":\"available_focused\",\"gsub_ligatures_pdf\":\"available_focused\",\"knuth_plass_pdf\":\"available\",\"hyphenation_pdf\":\"available_discretionary_body_paragraphs\",\"pdf_justification\":\"available_body_paragraphs\",\"page_builder_pdf\":\"available_v0_keep_widow\",\"stream_compression_pdf\":\"available\",\"robot_triage\":\"available\",\"wasm_core\":\"no-default-features available\",\"wasm_browser_package\":\"publishable_unpublished\",\"commonmark_spec\":\"0.31.2_ratcheted_min_362_of_652_normalized\"}}}}",
        env!("CARGO_PKG_VERSION"),
        Theme::default().to_config_json()
    );
}

fn print_robot_triage() {
    println!(
        "{{\"ok\":true,\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"quick_ref\":[\"fmd README.md --out README.html\",\"fmd README.md --to pdf --out README.pdf\",\"fmd --text '# Hello' --out hello.html\",\"fmd --text '# Hello' --out - > hello.html\",\"fmd config show --json\",\"fmd capabilities --json\",\"fmd doctor --json\"],\"health\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"syntax_highlighting\":\"available\",\"theme_model\":\"structured_v1\",\"native_config\":\"available\",\"wasm_core\":\"no-default-features\"}},\"recommended_next_actions\":[{{\"command\":\"fmd capabilities --json\",\"reason\":\"discover the stable command and exit-code contract\"}},{{\"command\":\"fmd config show --json\",\"reason\":\"inspect native defaults without reading external docs\"}},{{\"command\":\"fmd robot-docs guide\",\"reason\":\"read the in-tool agent guide\"}},{{\"command\":\"fmd README.md --out README.html --json\",\"reason\":\"render HTML and receive machine-readable write status on stderr\"}},{{\"command\":\"fmd README.md --to pdf --out README.pdf --json\",\"reason\":\"render the current embedded-font PDF v0 and receive machine-readable write status on stderr\"}}]}}",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_robot_docs() {
    println!(
        "fmd agent guide\n\nCanonical commands:\n  fmd README.md --out README.html\n  fmd README.md --to pdf --out README.pdf\n  fmd README.md --to pdf --pdf-line-numbers --out README.pdf\n  fmd README.md --to pdf --pdf-image images/chart.png=./chart.png --out README.pdf\n  fmd README.md --to pdf --title 'Quarterly Memo' --author 'FMD' --out README.pdf\n  SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf\n  fmd --max-input-bytes 1048576 README.md --out README.html\n  fmd - --out stdin.html < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd --text '# Hello' --out - > hello.html\n  fmd render README.md --to both --out README.html\n  fmd config show --json\n  fmd config set font serif --json\n  fmd --no-config README.md --out README.html\n  fmd capabilities --json\n  fmd doctor --json\n  fmd --robot-triage\n\nRules for agents:\n  stdout is document data for HTML-to-stdout and JSON data for capabilities/doctor/config/robot-triage.\n  `--out -` writes HTML document data to stdout only; PDF and --to both require a real output path.\n  diagnostics and write confirmations go to stderr.\n  use --json on render when you need machine-readable status events on stderr.\n  --max-input-bytes caps file/stdin/--text ingress before parsing; oversized input exits 66 with no document data on stdout.\n  --pdf-image maps one Markdown image destination to a local file as DEST=PATH for PDF rendering; repeat it for multiple images. The core never fetches network images or reads files itself.\n  PDF output is available as a compact deterministic v0 with embedded per-document font subsets, real metrics, focused GPOS kerning, GSUB ligatures, Knuth-Plass paragraph layout, deterministic discretionary hyphenation and glue justification for body paragraphs, basic keep/widow page building, syntax-highlighted wrapped code blocks, optional --pdf-line-numbers, local PNG image assets via --pdf-image, PDF metadata via --title/--author/SOURCE_DATE_EPOCH, a hierarchical accessible tagged-PDF structure tree (Document root, per-cell tables with header column scope, nested lists, blockquotes, figures with alt/bbox, links referenced via /OBJR, decoration as /Artifact), selectable text, and FlateDecode-compressed large page streams; deeper page-builder polish is still planned.\n  Use --css <file> for a full custom stylesheet replacement, --font serif for one render, config set font serif for a persistent native default, and --no-config for reproducible config-free runs."
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
