//! The `fmd` command-line surface (only compiled with the `cli` feature). This
//! is the single shared entrypoint for both the long-name binary and the short
//! `fmd` alias.

use std::io::{Error, ErrorKind as IoErrorKind, IsTerminal, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};

mod font_coverage;

use crate::ast::{Block, Document, Inline};
use crate::config::{CONFIG_KEYS, FmdConfig, config_path};
use crate::watch::{
    DEFAULT_INTERVAL_MS, PollWatcher, Route, SystemClock, bind_loopback, collect_watch_paths,
    expand_md_directory, referenced_local_paths, render_response, route_for, sse_preamble,
    sse_reload_event,
};
use crate::{
    FontAssetSlot, FontAssets, FontFamily, FontScale, HtmlFontFormat, HtmlOptions, PdfAMode,
    PdfASettings, PdfImageAsset, PdfOptions, RenderError, RenderWarning, Theme, parse_markdown,
    render_html_document, render_pdf_document, render_pdf_document_pdfa, render_warnings,
};

const DEFAULT_MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_PDF_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const DEFAULT_REMOTE_IMAGE_TIMEOUT_SECS: u64 = 20;
/// Caller-supplied stylesheets are inlined into `<style>`; a multi-gigabyte
/// sheet is an unmetered read. 1 MiB is far larger than any real theme CSS.
const MAX_STYLESHEET_BYTES: u64 = 1024 * 1024;

/// franken_markdown — Markdown to beautiful all-in-one HTML & tiny PDF.
#[derive(Parser)]
#[command(
    name = "fmd",
    version,
    about,
    long_about = "fmd converts Markdown files, stdin, or raw Markdown text into attractive self-contained HTML and compact deterministic PDF. The PDF path embeds curated per-document font subsets, uses Knuth-Plass paragraph breaking, applies deterministic discretionary hyphenation/justification for body paragraphs, and includes basic keep/widow pagination today; deeper page polish is still landing behind the same command contract.\n\nFirst tries that work:\n  fmd README.md\n  fmd - < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd --text '# Hello' --out - > hello.html\n  fmd render README.md --to both --out README.html\n  fmd config show --json\n  fmd capabilities --json\n  fmd robot-docs guide\n  fmd watch README.md --out README.html\n  fmd --robot-triage"
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
#[allow(clippy::large_enum_variant)]
enum Command {
    /// Render a Markdown file (or stdin) to HTML and/or PDF.
    Render(RenderArgs),
    /// Print the stable machine-readable command and feature contract.
    Capabilities,
    /// Print in-tool documentation written for coding agents.
    RobotDocs(RobotDocsArgs),
    /// Check a rendered document: text layer, internal anchors, warnings,
    /// overflow — machine-readable JSON on stdout (beads yo83).
    Verify(VerifyArgs),
    /// Rebuild when the Markdown file, `--css`, or referenced local assets change.
    Watch(WatchArgs),
    /// Check local build/runtime capabilities and report implementation status.
    Doctor(DoctorArgs),
    /// Read or edit native fmd config (never used by the WASM/core library).
    Config(ConfigArgs),
    /// Analyze document intelligence: word counts, reading/speaking time,
    /// Flesch readability scores, heading outline, and structural health checks.
    Stats(StatsArgs),
    /// Compare two Markdown documents and render semantic visual diff (HTML, JSON).
    Diff(DiffArgs),
    /// Assemble a directory of Markdown files into a unified HTML site and/or a
    /// single PDF book (global outline, continuous page numbers).
    Book(BookArgs),
    /// Render many Markdown inputs in parallel under a bounded worker budget
    /// (native-only; Asupersync-backed). See docs/BATCH_ORCHESTRATION.md.
    #[cfg(feature = "batch")]
    Batch(BatchArgs),
    /// Run Model Context Protocol (MCP) stdio server exposing tools for agents.
    #[cfg(feature = "mcp")]
    Mcp(McpArgs),
}

#[derive(Args, Clone)]
struct BookArgs {
    /// Book directory (walked recursively for *.md/*.markdown, sorted
    /// deterministically by path).
    #[arg(value_name = "DIR")]
    input: PathBuf,
    /// Output directory for the site and/or book file (default: alongside the
    /// input directory as `<dir>-site/` and `<dir>.pdf`).
    #[arg(long, short)]
    out_dir: Option<PathBuf>,
    /// Which output(s) to produce.
    #[arg(long, value_enum, default_value_t = Target::Both)]
    to: Target,
    /// Emit the deterministic book receipt JSON to stdout.
    #[arg(long)]
    json: bool,
    /// Maximum Markdown input bytes per file accepted before parsing (default 64 MiB).
    #[arg(long, default_value_t = DEFAULT_MAX_INPUT_BYTES)]
    max_input_bytes: u64,
}

#[derive(Args)]
struct DiffArgs {
    /// Old / base Markdown file.
    #[arg(value_name = "OLD_FILE")]
    old_file: PathBuf,
    /// New / updated Markdown file.
    #[arg(value_name = "NEW_FILE")]
    new_file: PathBuf,
    /// Output file path (defaults to stdout for HTML).
    #[arg(short, long, value_name = "OUT")]
    out: Option<PathBuf>,
    /// Emit machine-readable JSON diff to stdout.
    #[arg(long)]
    json: bool,
    /// Maximum Markdown input bytes accepted before parsing (default 64 MiB).
    #[arg(long, default_value_t = DEFAULT_MAX_INPUT_BYTES)]
    max_input_bytes: u64,
}

#[derive(Args)]
struct StatsArgs {
    /// Markdown input file (or '-' for stdin).
    #[arg(value_name = "INPUT", default_value = "-")]
    input: PathBuf,
    /// Raw Markdown text to analyze (alternative to input file).
    #[arg(long, value_name = "TEXT")]
    text: Option<String>,
    /// Emit machine-readable JSON stats to stdout.
    #[arg(long)]
    json: bool,
    /// Maximum Markdown input bytes accepted before parsing (default 64 MiB).
    #[arg(long, default_value_t = DEFAULT_MAX_INPUT_BYTES)]
    max_input_bytes: u64,
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
    /// Maximum bytes accepted for each auto-loaded local PNG/SVG/JPEG image asset.
    #[arg(long, default_value_t = DEFAULT_MAX_PDF_IMAGE_BYTES)]
    max_pdf_image_bytes: u64,
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

#[cfg(feature = "mcp")]
#[derive(Args, Clone)]
struct McpArgs {
    /// Maximum Markdown input bytes accepted per tool call (default 64 MiB).
    #[arg(long, default_value_t = DEFAULT_MAX_INPUT_BYTES)]
    max_input_bytes: u64,
}

#[cfg(feature = "batch")]
#[derive(Clone, Copy, clap::ValueEnum)]
enum BatchModeArg {
    Interactive,
    Throughput,
}

#[derive(Args)]
struct VerifyArgs {
    /// Markdown file to verify (`-` reads stdin).
    #[arg(required = true)]
    input: PathBuf,
    /// Emit the stable JSON report. Implied when stdout is not a TTY (pipes,
    /// CI). On a TTY the default is a human caret report; pass `--json` to
    /// force the schema.
    #[arg(long)]
    json: bool,
    /// Restrict findings to the accessibility audit (missing alt text,
    /// heading-level jumps, generic link text, headerless tables). The a11y
    /// findings also appear in the default verify report; this flag is the
    /// focused view for docs accessibility sweeps.
    #[arg(long)]
    a11y: bool,
    /// Check external http(s) links with the system curl/wget (HEAD, then a
    /// ranged GET fallback), reporting broken/redirected links. Opt-in: it
    /// touches the network, so it is NEVER part of the default verify.
    #[arg(long)]
    links: bool,
    /// Cache file for --links results (JSONL: url, status, checked_unix).
    /// Reuses entries newer than --links-ttl-secs. Default: none (no cache).
    #[arg(long, value_name = "FILE")]
    links_cache: Option<PathBuf>,
    /// Per-link timeout in seconds for --links (default 10).
    #[arg(long, default_value_t = 10)]
    links_timeout_secs: u64,
    /// Cache entry age to accept for --links (default 86400 = 1 day).
    #[arg(long, default_value_t = 86400)]
    links_ttl_secs: u64,
}

#[derive(Args, Clone)]
struct WatchArgs {
    /// Markdown file to watch (a real path; stdin cannot be polled).
    input: PathBuf,
    /// Which output(s) to produce on each rebuild.
    #[arg(long, value_enum, default_value_t = Target::Html)]
    to: Target,
    /// Output path. Required: a watch loop cannot own stdout as a document sink.
    #[arg(long, short)]
    out: PathBuf,
    /// Override the configured/default body font.
    #[arg(long, value_enum)]
    font: Option<FontArg>,
    /// Custom stylesheet watched together with the Markdown file.
    #[arg(long)]
    css: Option<PathBuf>,
    /// Poll and debounce window in milliseconds (default 300).
    #[arg(long, default_value_t = DEFAULT_INTERVAL_MS)]
    interval: u64,
    /// Extra stderr detail on each rebuild (path counts).
    #[arg(long)]
    verbose: bool,
    /// JSONL rebuild/watch events on stderr; stdout stays empty.
    #[arg(long)]
    json: bool,
    /// Serve a loopback-only HTML preview (`127.0.0.1`, OS-chosen port) with
    /// auto-reload. The reload snippet is injected only into the in-memory
    /// preview, never into `--out`.
    #[arg(long)]
    serve: bool,
    /// Take N edit-to-rebuild samples, print p95 timings on stderr, and exit.
    /// Each sample appends a marker comment to the input file. p95 of total
    /// (detect+render+serve) must be ≤ 150ms or the process exits 1 (j3e0.3).
    #[arg(long, value_name = "N")]
    measure: Option<u32>,
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
    /// Document language tag for hyphenation and HTML lang attribute (e.g. "en", "de", "fr", "es", "nl").
    #[arg(long)]
    lang: Option<String>,
    /// Markdown authoring profile (e.g. "commonmark-gfm", "gfm-plus").
    #[arg(long)]
    profile: Option<String>,
    /// Pass raw HTML in the source through instead of escaping it.
    #[arg(long)]
    allow_html: bool,
    /// Generate a table of contents.
    #[arg(long)]
    toc: bool,
    /// Maximum heading depth for table of contents (default: 3).
    #[arg(long)]
    toc_depth: Option<u8>,
    /// Font container format for embedded HTML font subsets: woff1 (default,
    /// ~half the bytes, every modern browser) or ttf (raw subset bytes).
    #[arg(long, value_enum)]
    html_font_format: Option<HtmlFontFormatArg>,
    /// Generate an interactive, self-hosting single-file HTML workspace with live editor,
    /// real-time preview, document statistics, and client-side PDF export.
    #[arg(long, visible_alias = "self-hosting")]
    interactive_html: bool,
    /// Typographic scale factor or preset for uniform type sizing across HTML and PDF.
    ///
    /// Accepts named presets (`xs`, `sm`, `compact`, `md`, `normal`, `default`, `lg`, `xl`, `2xl`, `huge`),
    /// percentages (e.g. `125%`), multipliers (e.g. `1.125`), or CSS sizes (`18px`, `12pt`).
    /// Scales body, headings, code, tables, and layout measure proportionally without aliasing.
    #[arg(long, value_name = "SCALE|PRESET", visible_alias = "type-size")]
    font_scale: Option<String>,
    /// Write a deterministic JSON search index (headings + anchored paragraph
    /// chunks, schema fmd-search-index-v1) for docs-site search integrations.
    #[arg(long, value_name = "PATH")]
    search_index: Option<PathBuf>,
    /// Adaptive page budgeting solver to fit rendered PDF content into target pages.
    ///
    /// Automatically tunes micro-typographic scale (base font size, line height,
    /// and margins) via binary search to strictly fit the document in at most N pages.
    #[arg(long, value_name = "PAGES", visible_alias = "target-pages")]
    fit_to_pages: Option<usize>,
    /// Opt-in microtypography for justified PDF body paragraphs: `protrusion`
    /// enables optical-margin alignment (punctuation hangs into the margin)
    /// via the precomputed per-box hooks in docs/MICROTYPOGRAPHY.md. Default
    /// `off` keeps output byte-identical to previous versions.
    #[arg(long, value_enum, default_value_t = MicrotypeArg::Off)]
    microtype: MicrotypeArg,
    /// Enable gradual adjacent demerits (Verna, DocEng '25) in the
    /// Knuth-Plass breaker for justified paragraphs: replaces the coarse
    /// 4-class fitness check with a penalty proportional to the spacing
    /// difference between consecutive lines, producing more homogeneous
    /// inter-word spacing. Default off — classic behavior.
    #[arg(long = "typography-homogeneous")]
    typography_homogeneous: bool,
    /// Enable river-seed demerits: penalize break candidates whose previous
    /// line's last inter-word space aligns horizontally with a space in the
    /// candidate line — breaking up vertical whitespace channels ("rivers").
    /// Works for justified and ragged text. Default off — classic behavior.
    #[arg(long = "typography-antiriver")]
    typography_antiriver: bool,
    /// Multi-objective (Pareto) line breaking (Holkner): track bounded fronts
    /// of non-dominated break paths over structure and hyphenation demerits
    /// instead of a single scalar winner. Default off — byte-identical.
    #[arg(long = "typography-pareto")]
    typography_pareto: bool,
    /// Plass-style optimal pagination (Plass & Li, 1981): replace greedy
    /// per-page breaking with a document-wide DP minimizing total void
    /// badness plus keep-penalties. Better page fills under tight content;
    /// default off — greedy pagination.
    #[arg(long = "pdf-optimal-pagination")]
    pdf_optimal_pagination: bool,
    /// Render muted line numbers in PDF fenced code blocks.
    #[arg(long)]
    pdf_line_numbers: bool,
    /// Render running page numbers in the bottom margin of PDF pages.
    #[arg(long)]
    pdf_page_numbers: bool,
    /// Base body font size override in points (clamped to [6, 24]).
    #[arg(long)]
    pdf_base_font_size: Option<f32>,
    /// Per-step heading geometric scale ratio (e.g. 1.25 for Major Third, clamped to [1.05, 2.0]).
    #[arg(long)]
    pdf_heading_scale: Option<f32>,
    /// Nominal table cell font size override in points (clamped to [5, base_font_size]).
    #[arg(long)]
    pdf_table_font_size: Option<f32>,
    /// Provide or override a local PDF image asset as MARKDOWN_DEST=PATH.
    /// File-based HTML/PDF renders also auto-load relative local PNG/SVG/JPEG
    /// image destinations, and PDF renders fetch remote http(s) destinations
    /// unless --no-remote-images is set; the render core itself never fetches
    /// or reads files.
    /// Repeat for multiple images.
    #[arg(long = "pdf-image", value_name = "DEST=PATH")]
    pdf_images: Vec<String>,
    /// Host TrueType face for a renderer slot as SLOT=PATH. Repeatable.
    /// SLOT is body-regular, body-bold, body-italic, body-bold-italic, or
    /// mono-regular. Applies to both HTML and PDF so the two stay coherent.
    /// Variable `wght` faces instance at the slot's pinned weight (see
    /// `--pdf-font-weight`); static faces ignore the pin. When body-bold is
    /// omitted and body-regular is a variable face, bold instances from that
    /// same file at weight 700.
    #[arg(long = "pdf-font", value_name = "SLOT=PATH")]
    pdf_fonts: Vec<String>,
    /// Pin CSS font-weight for a host font slot. Repeatable. Bare WEIGHT pins
    /// body-regular; SLOT=WEIGHT pins one slot. Range 1..=1000. Static faces
    /// ignore the pin (stderr warning `font_weight_ignored_static`).
    #[arg(long = "pdf-font-weight", value_name = "WEIGHT|SLOT=WEIGHT")]
    pdf_font_weights: Vec<String>,
    /// Maximum bytes accepted for each explicit PDF image or auto-loaded local
    /// HTML/PDF image file before rendering.
    #[arg(long, default_value_t = DEFAULT_MAX_PDF_IMAGE_BYTES)]
    max_pdf_image_bytes: u64,
    /// Do not fetch remote http(s) image destinations for PDF output. By
    /// default the CLI downloads each hotlinked image (via the system `curl`
    /// or `wget`) with a timeout and the `--max-pdf-image-bytes` size cap so
    /// PDFs match the HTML preview; a failed or disabled fetch degrades to the
    /// image's alt text with a warning. The render core itself never touches
    /// the network.
    #[arg(long)]
    no_remote_images: bool,
    /// Emit PDF/A-2b identification: XMP packet + sRGB OutputIntent. Spelling
    /// is `2b` (also `pdf-a-2b`). Default is off so historical PDF bytes stay
    /// identical. Library equivalent: `render_pdf_pdfa(..., PdfASettings::a2b())`.
    #[arg(long = "pdf-a", value_name = "PROFILE")]
    pdf_a: Option<String>,
    /// Fail closed on PDF/A-2b constructs this engine cannot carry (named
    /// `pdf_a_*` errors: `javascript:` / `file:` URI actions). Requires `--pdf-a 2b`.
    #[arg(long = "pdf-a-strict")]
    pdf_a_strict: bool,
    /// Per-image timeout in seconds for remote PDF image fetches.
    #[arg(long, default_value_t = DEFAULT_REMOTE_IMAGE_TIMEOUT_SECS)]
    remote_image_timeout_secs: u64,
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
    #[command(subcommand)]
    command: Option<DoctorCommand>,
}

#[derive(Subcommand)]
enum DoctorCommand {
    /// Audit Markdown corpus glyph coverage against bundled faces.
    Fonts(DoctorFontsArgs),
}

#[derive(Args)]
struct DoctorFontsArgs {
    /// Markdown file or directory to scan (recursed; no symlink follow).
    #[arg(long)]
    corpus: PathBuf,
    /// Emit the stable JSON schema on stdout.
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

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Target {
    Html,
    Pdf,
    Both,
    /// EPUB 3 e-book (single file; a one-chapter book). Writes a real file —
    /// binary zips cannot stream to stdout.
    Epub,
    /// Standalone vector SVG poster (glyphs as paths, zero fonts). Text format
    /// — streams to stdout like HTML when no --out is given.
    Svg,
}

#[derive(Copy, Clone, ValueEnum)]
enum FontArg {
    Sans,
    Serif,
}

#[derive(Copy, Clone, ValueEnum)]
enum HtmlFontFormatArg {
    Ttf,
    Woff1,
}

#[derive(Copy, Clone, Default, ValueEnum)]
enum MicrotypeArg {
    #[default]
    Off,
    Protrusion,
    /// Glyph expansion only (Hàn Thế Thành): justified lines stretch/shrink
    /// word glyphs horizontally via the PDF `Tz` operator (±1.5%) instead of
    /// letter-spacing, keeping inter-word spaces closer to natural.
    Expansion,
}

impl From<HtmlFontFormatArg> for HtmlFontFormat {
    fn from(f: HtmlFontFormatArg) -> Self {
        match f {
            HtmlFontFormatArg::Ttf => HtmlFontFormat::Ttf,
            HtmlFontFormatArg::Woff1 => HtmlFontFormat::Woff1,
        }
    }
}

impl From<MicrotypeArg> for crate::layout::MicrotypeOptions {
    fn from(m: MicrotypeArg) -> Self {
        match m {
            MicrotypeArg::Off => crate::layout::MicrotypeOptions::DISABLED,
            // CONSERVATIVE carries the 15 per-mille expansion budget; the
            // Tz emitter applies it as true glyph scaling on justified lines.
            MicrotypeArg::Protrusion => crate::layout::MicrotypeOptions::CONSERVATIVE,
            MicrotypeArg::Expansion => crate::layout::MicrotypeOptions {
                protrusion: false,
                max_expansion_per_mille: 15,
            },
        }
    }
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

    let no_color = cli.no_color;
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
        Some(Command::Doctor(args)) => run_doctor(args, json),
        Some(Command::Config(args)) => run_config(args, json, no_config),
        Some(Command::Stats(args)) => run_stats(args, json),
        Some(Command::Diff(args)) => run_diff(args, json, no_config),
        Some(Command::Book(args)) => run_book(args, json, no_config),
        #[cfg(feature = "batch")]
        Some(Command::Batch(args)) => run_batch(args, json, no_config),
        #[cfg(feature = "mcp")]
        Some(Command::Mcp(args)) => run_mcp(args),
        Some(Command::Verify(args)) => run_verify(args, json, no_color),
        Some(Command::Watch(args)) => run_watch(args, json, no_config),
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

#[cfg(feature = "mcp")]
fn run_mcp(args: McpArgs) -> ExitCode {
    match crate::mcp::run_stdio_server(args.max_input_bytes) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(70, &format!("mcp server error: {e}")),
    }
}

fn watch_to_render(args: &WatchArgs) -> RenderArgs {
    RenderArgs {
        input: Some(args.input.display().to_string()),
        text: None,
        to: args.to,
        out: Some(args.out.clone()),
        font: args.font,
        css: args.css.clone(),
        title: None,
        author: None,
        lang: None,
        profile: None,
        allow_html: false,
        toc: false,
        toc_depth: None,
        html_font_format: None,
        search_index: None,
        interactive_html: false,
        font_scale: None,
        fit_to_pages: None,
        pdf_line_numbers: false,
        pdf_page_numbers: false,
        pdf_base_font_size: None,
        pdf_heading_scale: None,
        pdf_table_font_size: None,
        pdf_images: Vec::new(),
        pdf_fonts: Vec::new(),
        pdf_font_weights: Vec::new(),
        pdf_a: None,
        pdf_a_strict: false,
        max_pdf_image_bytes: DEFAULT_MAX_PDF_IMAGE_BYTES,
        no_remote_images: false,
        remote_image_timeout_secs: DEFAULT_REMOTE_IMAGE_TIMEOUT_SECS,
        max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
        microtype: Default::default(),
        typography_homogeneous: false,
        typography_antiriver: false,
        typography_pareto: false,
        pdf_optimal_pagination: false,
        json: args.json,
    }
}

fn run_watch(args: WatchArgs, global_json: bool, no_config: bool) -> ExitCode {
    let json = global_json || args.json;
    if args.input.as_os_str() == "-" {
        return fail_json(
            64,
            "usage_error",
            "fmd watch needs a file path; stdin cannot be polled",
            json,
        );
    }
    if args.out.as_os_str() == "-" {
        return fail_json(
            64,
            "usage_error",
            "fmd watch requires a real --out path; stdout is not a watch sink",
            json,
        );
    }
    if args.measure == Some(0) {
        return fail_json(64, "usage_error", "--measure must be at least 1", json);
    }
    let interval_ms = args.interval.max(1);
    let interval = Duration::from_millis(interval_ms);
    let mut extras = Vec::new();
    if let Some(css) = &args.css {
        extras.push(css.clone());
    }
    if let Ok(src) = std::fs::read_to_string(&args.input) {
        let base = args.input.parent().unwrap_or_else(|| Path::new("."));
        extras.extend(referenced_local_paths(&src, base));
    }
    // xjld: a directory input expands to every `*.md` file under it.
    // A directory with no `*.md` is a usage error (silent no-watch
    // would surprise agents that typed the wrong path).
    if args.input.is_dir() {
        let md_files = expand_md_directory(&args.input);
        if md_files.is_empty() {
            return fail_json(
                64,
                "usage_error",
                &format!(
                    "no *.md files found under {}; fmd watch <dir> requires at least one Markdown input",
                    args.input.display()
                ),
                json,
            );
        }
        return run_watch_directory(args, md_files, interval, json, no_config);
    }
    let paths = collect_watch_paths(&args.input, &extras);
    let debounce = if args.measure.is_some() {
        Duration::ZERO
    } else {
        interval
    };
    let mut watcher = PollWatcher::new(paths, debounce, SystemClock);
    let _ = run_render(watch_to_render(&args), json, no_config);
    let preview = if args.serve {
        match start_watch_preview() {
            Ok(preview) => Some(preview),
            Err(e) => {
                return fail_json(
                    70,
                    "preview_bind_failed",
                    &format!("loopback preview bind failed: {e}"),
                    json,
                );
            }
        }
    } else {
        None
    };
    if let Some(preview) = preview.as_ref() {
        refresh_watch_preview(preview, &args, no_config);
    }
    if json {
        eprint!(
            "{{\"ok\":true,\"event\":\"watching\",\"paths\":{},\"interval_ms\":{}",
            watcher.paths().len(),
            interval_ms
        );
        if let Some(preview) = preview.as_ref() {
            eprint!(",\"preview\":\"http://127.0.0.1:{}/\"", preview.port);
        }
        eprintln!("}}");
    } else {
        eprint!(
            "fmd watch: {} path(s), interval {interval_ms}ms",
            watcher.paths().len()
        );
        if let Some(preview) = preview.as_ref() {
            eprint!("; preview http://127.0.0.1:{}/", preview.port);
        }
        eprintln!("; Ctrl-C to stop");
    }
    if let Some(samples) = args.measure {
        return run_watch_measure(
            &args,
            json,
            no_config,
            samples,
            &mut watcher,
            preview.as_ref(),
        );
    }
    loop {
        std::thread::sleep(interval);
        let events = watcher.poll();
        if events.is_empty() {
            continue;
        }
        if json {
            let mut changed = String::from("[");
            for (i, event) in events.iter().enumerate() {
                if i > 0 {
                    changed.push(',');
                }
                changed.push('"');
                changed.push_str(&json_escape(&event.path.display().to_string()));
                changed.push('"');
            }
            changed.push(']');
            eprintln!(
                "{{\"ok\":true,\"event\":\"rebuild\",\"changed\":{changed},\"count\":{}}}",
                events.len()
            );
        } else if args.verbose {
            eprintln!(
                "fmd watch: {} change(s) across {} path(s); rebuilding",
                events.len(),
                watcher.paths().len()
            );
        } else {
            eprintln!(
                "fmd watch: {} changed; rebuilding",
                events[0].path.display()
            );
        }
        let _ = run_render(watch_to_render(&args), json, no_config);
        if let Some(preview) = preview.as_ref() {
            refresh_watch_preview(preview, &args, no_config);
        }
        if let Ok(src) = std::fs::read_to_string(&args.input) {
            let base = args.input.parent().unwrap_or_else(|| Path::new("."));
            for path in referenced_local_paths(&src, base) {
                watcher.add_path(path);
            }
        }
    }
}

/// xjld: handle `fmd watch <dir>` — a directory of Markdown files. Every
/// `*.md` file under `args.input` is watched; on a change, the file is
/// rendered individually to a sibling output path (under `args.out`,
/// which must be a directory in this mode). The first file (lex order)
/// is the "primary" input used for the initial render and for the
/// loopback preview.
fn run_watch_directory(
    args: WatchArgs,
    md_files: Vec<PathBuf>,
    interval: Duration,
    json: bool,
    no_config: bool,
) -> ExitCode {
    if !args.out.is_dir() {
        return fail_json(
            64,
            "usage_error",
            "fmd watch <dir> requires --out to be a directory; the per-file output is written there",
            json,
        );
    }
    let primary = md_files[0].clone();
    let primary_args = WatchArgs {
        input: primary.clone(),
        out: args.out.join(derive_sibling_html(&primary)),
        ..args.clone()
    };
    let _ = run_render(watch_to_render(&primary_args), json, no_config);
    // Build the watch set: primary + every other `.md` file + CSS
    // (deduplicated by collect_watch_paths).
    let mut others: Vec<PathBuf> = md_files.iter().skip(1).cloned().collect();
    if let Some(css) = &primary_args.css {
        others.push(css.clone());
    }
    if let Ok(src) = std::fs::read_to_string(&primary) {
        let base = primary.parent().unwrap_or_else(|| Path::new("."));
        others.extend(referenced_local_paths(&src, base));
    }
    let paths = collect_watch_paths(&primary, &others);
    let mut watcher = PollWatcher::new(paths, interval, SystemClock);
    let preview = if args.serve {
        match start_watch_preview() {
            Ok(preview) => Some(preview),
            Err(e) => {
                return fail_json(
                    70,
                    "preview_bind_failed",
                    &format!("loopback preview bind failed: {e}"),
                    json,
                );
            }
        }
    } else {
        None
    };
    loop {
        let events = watcher.poll();
        if events.is_empty() {
            std::thread::sleep(interval);
            continue;
        }
        // Render the most recently changed `.md` file in the
        // directory; other changes (e.g. CSS) are picked up by the
        // watcher but do not dispatch a per-file render.
        let changed = events
            .iter()
            .rev()
            .map(|e| e.path.clone())
            .find(|p| md_files.iter().any(|m| m == p))
            .unwrap_or(primary.clone());
        let per_file_args = WatchArgs {
            input: changed.clone(),
            out: args.out.join(derive_sibling_html(&changed)),
            ..primary_args.clone()
        };
        let _ = run_render(watch_to_render(&per_file_args), json, no_config);
        if let Some(preview) = preview.as_ref() {
            refresh_watch_preview(preview, &per_file_args, no_config);
        }
        if json {
            eprintln!(
                "{{\"ok\":true,\"event\":\"watched_rebuild\",\"path\":\"{}\",\"out\":\"{}\"}}",
                changed.display(),
                per_file_args.out.display()
            );
        } else if args.verbose {
            eprintln!(
                "fmd: re-rendered {} -> {} ({} files watched)",
                changed.display(),
                per_file_args.out.display(),
                md_files.len()
            );
        } else {
            eprintln!("fmd: re-rendered {}", changed.display());
        }
    }
}

/// Derive a sibling `.html` path for a `.md` input: `foo/bar.md` ->
/// `foo/bar.html`. Used by `fmd watch <dir>` to compute the per-file
/// output under the user-supplied `--out` directory.
fn derive_sibling_html(md_path: &Path) -> PathBuf {
    md_path.with_extension("html")
}

const WATCH_MEASURE_BUDGET_MS: f64 = 150.0;

fn run_watch_measure(
    args: &WatchArgs,
    json: bool,
    no_config: bool,
    samples: u32,
    watcher: &mut PollWatcher<SystemClock>,
    preview: Option<&WatchPreview>,
) -> ExitCode {
    let mut totals = Vec::with_capacity(samples as usize);
    for n in 1..=samples {
        if let Err(e) = append_watch_measure_marker(&args.input, n) {
            return fail_json(
                66,
                "input_error",
                &format!("writing measure marker: {e}"),
                json,
            );
        }
        let t0 = Instant::now();
        let detect = match wait_for_watch_change(watcher, Duration::from_secs(5)) {
            Some(elapsed) => elapsed,
            None => {
                return fail_json(
                    70,
                    "measure_timeout",
                    &format!("sample {n}: no change detected within 5s"),
                    json,
                );
            }
        };
        let t_render = Instant::now();
        let _ = run_render(watch_to_render_quiet(args), false, no_config);
        let render_ms = t_render.elapsed().as_secs_f64() * 1000.0;
        let t_serve = Instant::now();
        if let Some(preview) = preview {
            refresh_watch_preview(preview, args, no_config);
        }
        let serve_ms = t_serve.elapsed().as_secs_f64() * 1000.0;
        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let detect_ms = detect.as_secs_f64() * 1000.0;
        totals.push(total_ms);
        eprintln!(
            "{{\"ok\":true,\"event\":\"sample\",\"n\":{n},\"detect_ms\":{},\"render_ms\":{},\"serve_ms\":{},\"total_ms\":{}}}",
            json_num(detect_ms),
            json_num(render_ms),
            json_num(serve_ms),
            json_num(total_ms)
        );
    }
    let p95 = percentile(&totals, 0.95);
    let pass = p95 <= WATCH_MEASURE_BUDGET_MS;
    eprintln!(
        "{{\"ok\":{},\"event\":\"measure\",\"samples\":{samples},\"p95_ms\":{},\"budget_ms\":{},\"verdict\":\"{}\"}}",
        if pass { "true" } else { "false" },
        json_num(p95),
        json_num(WATCH_MEASURE_BUDGET_MS),
        if pass { "pass" } else { "fail" }
    );
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn watch_to_render_quiet(args: &WatchArgs) -> RenderArgs {
    let mut render = watch_to_render(args);
    render.json = false;
    render
}

fn append_watch_measure_marker(path: &Path, n: u32) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new().append(true).open(path)?;
    writeln!(file, "\n<!-- fmd-watch-measure-{n} -->")?;
    file.flush()
}

fn wait_for_watch_change(
    watcher: &mut PollWatcher<SystemClock>,
    timeout: Duration,
) -> Option<Duration> {
    let start = Instant::now();
    loop {
        let events = watcher.poll();
        if !events.is_empty() {
            return Some(start.elapsed());
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn percentile(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = (p * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

fn json_num(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let mut s = format!("{value:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() { "0".to_string() } else { s }
}

struct WatchPreview {
    port: u16,
    html: Arc<Mutex<String>>,
    clients: Arc<Mutex<Vec<TcpStream>>>,
}

fn start_watch_preview() -> std::io::Result<WatchPreview> {
    let (listener, port) = bind_loopback()?;
    let html = Arc::new(Mutex::new(String::new()));
    let clients = Arc::new(Mutex::new(Vec::new()));
    let html_for_thread = Arc::clone(&html);
    let clients_for_thread = Arc::clone(&clients);
    std::thread::Builder::new()
        .name("fmd-watch-preview".to_string())
        .spawn(move || preview_accept_loop(listener, html_for_thread, clients_for_thread))?;
    Ok(WatchPreview {
        port,
        html,
        clients,
    })
}

fn refresh_watch_preview(preview: &WatchPreview, args: &WatchArgs, no_config: bool) {
    if let Ok(html) = watch_preview_html(args, no_config) {
        {
            let mut slot = preview.html.lock().unwrap_or_else(|e| e.into_inner());
            *slot = html;
        }
        let mut clients = preview.clients.lock().unwrap_or_else(|e| e.into_inner());
        clients.retain_mut(|stream| {
            stream
                .write_all(sse_reload_event().as_bytes())
                .and_then(|()| stream.flush())
                .is_ok()
        });
    }
}

fn watch_preview_html(args: &WatchArgs, no_config: bool) -> Result<String, String> {
    let src = std::fs::read_to_string(&args.input).map_err(|e| e.to_string())?;
    let config = load_config(no_config).map_err(|e| e.to_string())?;
    let mut theme = config.to_theme();
    if let Some(font) = args.font {
        theme = theme.with_font(font.into());
    }
    let custom_css = match args.css.as_deref().or(config.custom_css.as_deref()) {
        Some(path) => Some(read_stylesheet(path).map_err(|e| e.to_string())?),
        None => None,
    };
    let doc = parse_markdown(&src);
    let opts = HtmlOptions {
        theme,
        title: None,
        custom_css,
        allow_raw_html: false,
        font_assets: FontAssets::default(),
        image_assets: Vec::new(),
        lang: None,
        profile: None,
        toc: false,
        toc_depth: None,
        html_font_format: HtmlFontFormat::default(),
    };
    render_html_document(&doc, &opts).map_err(|e| e.to_string())
}

fn preview_accept_loop(
    listener: TcpListener,
    html: Arc<Mutex<String>>,
    clients: Arc<Mutex<Vec<TcpStream>>>,
) {
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else {
            continue;
        };
        let _ = stream.set_nodelay(true);
        let head = read_http_head(&mut stream);
        match route_for(&head) {
            Route::Index => {
                let body = html.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let bytes = render_response(Route::Index, &body, "");
                let _ = stream.write_all(&bytes);
            }
            Route::Events => {
                let bytes = render_response(Route::Events, "", &sse_preamble());
                if stream
                    .write_all(&bytes)
                    .and_then(|()| stream.flush())
                    .is_ok()
                {
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                    clients
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(stream);
                }
            }
            Route::NotFound => {
                let bytes = render_response(Route::NotFound, "", "");
                let _ = stream.write_all(&bytes);
            }
        }
    }
}

fn read_http_head(stream: &mut TcpStream) -> Vec<u8> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 512];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n")
                    || buf.windows(2).any(|w| w == b"\n\n")
                    || buf.len() >= 8192
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    buf
}

fn run_render(args: RenderArgs, global_json: bool, no_config: bool) -> ExitCode {
    let json = global_json || args.json;
    if out_is_stdout(&args) && !matches!(args.to, Target::Html | Target::Svg) {
        return fail_json(
            64,
            "usage_error",
            "`--out -` writes HTML/SVG to stdout only; PDF and --to both require a real output path",
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

    // Transclusion (qpqv): expand {{#include path}} for FILE inputs before
    // parsing. The resolver sandboxes to the top input's directory and
    // resolves each include relative to the INCLUDING file. stdin/--text have
    // no base directory: a directive there is a usage error, never a silent
    // skip.
    let src = if crate::transclude::has_includes(&src) {
        match args.input.as_deref() {
            Some("-") | None => {
                return fail_json(
                    64,
                    "usage_error",
                    "{{#include}} needs a file input (stdin/--text have no base directory)",
                    json,
                );
            }
            Some(input_path) => {
                match expand_file_includes(&src, input_path, args.max_input_bytes) {
                    Ok(expanded) => expanded,
                    Err(e) => return fail_json(66, "input_error", &e, json),
                }
            }
        }
    } else {
        src
    };

    let config = match load_config(no_config) {
        Ok(config) => config,
        Err(e) => return fail_json(66, "config_error", &format!("reading config: {e}"), json),
    };

    let mut theme = config.to_theme();
    if let Some(font) = args.font {
        theme = theme.with_font(font.into());
    }

    let font_scale = if let Some(scale_str) = &args.font_scale {
        match FontScale::parse(scale_str) {
            Some(scale) => Some(scale),
            None => {
                return fail_json(
                    64,
                    "usage_error",
                    &format!(
                        "unknown font scale: '{scale_str}'. Valid choices: 'xs', 'sm', 'compact', 'md', 'normal', 'default', 'lg', 'xl', '2xl', 'huge', percentages like '125%', or numbers like '1.2'"
                    ),
                    json,
                );
            }
        }
    } else {
        None
    };

    if let Some(scale) = font_scale {
        theme = theme.with_font_scale(scale);
    }

    let css_path = args.css.clone().or_else(|| config.custom_css.clone());
    let custom_css = match css_path.as_deref() {
        Some(p) => match read_stylesheet(p) {
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
    let want_epub = matches!(args.to, Target::Epub);
    let want_svg = matches!(args.to, Target::Svg);
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
    if want_epub && let Some(p) = out_path(&args, single, "epub") {
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
    let font_assets = match load_host_font_assets(&args.pdf_fonts, &args.pdf_font_weights) {
        Ok(assets) => assets,
        Err(HostFontError::Usage(e)) => return fail_json(64, "usage_error", &e, json),
        Err(HostFontError::Input(e)) => return fail_json(66, "input_error", &e, json),
    };
    if json {
        report_font_assets(&font_assets);
    }
    // Frontmatter metadata (qqst): the parser skips a leading --- key=value
    // block; harvest it here so document metadata flows into HTML <title> /
    // PDF metadata / hyphenation language / TOC. Precedence: explicit CLI flag
    // > frontmatter > first-heading default (unchanged when absent).
    let frontmatter = crate::parse::split_frontmatter(&src).0;
    let frontmatter_unknown = frontmatter
        .as_ref()
        .map(|fm| fm.unknown_keys.clone())
        .unwrap_or_default();
    for key in &frontmatter_unknown {
        eprintln!(
            "fmd: warning: unknown frontmatter key \"{key}\" (supported: title, author, lang, toc, toc_depth)"
        );
    }
    let frontmatter_title = frontmatter.as_ref().and_then(|fm| fm.title.clone());
    let frontmatter_author = frontmatter.as_ref().and_then(|fm| fm.author.clone());
    let frontmatter_lang = frontmatter.as_ref().and_then(|fm| fm.lang.clone());
    let frontmatter_toc = frontmatter.as_ref().and_then(|fm| fm.toc);
    let frontmatter_toc_depth = frontmatter.as_ref().and_then(|fm| fm.toc_depth);
    let doc = parse_markdown(&src);
    let mut image_destinations = Vec::new();
    collect_image_destinations(&doc.blocks, &mut image_destinations);
    let base_image_dir = auto_pdf_image_base_dir(args.input.as_deref(), args.text.as_deref());
    let html_image_assets = if want_html {
        let mut assets = Vec::new();
        if let Some(base_dir) = base_image_dir.as_deref()
            && let Err(e) = append_auto_image_assets(
                &doc,
                base_dir,
                &mut assets,
                args.max_pdf_image_bytes,
                "HTML",
            )
        {
            return fail_json(66, "input_error", &e, json);
        }
        assets
    } else {
        Vec::new()
    };
    let pdf_image_assets = if want_pdf {
        let mut assets = match read_pdf_image_assets(
            &args.pdf_images,
            args.max_pdf_image_bytes,
            &image_destinations,
        ) {
            Ok(assets) => assets,
            // A malformed `--pdf-image` spec is a usage error (64); a missing/
            // unreadable/oversized file is an input error (66).
            Err(PdfImageError::Usage(e)) => return fail_json(64, "usage_error", &e, json),
            Err(PdfImageError::Input(e)) => return fail_json(66, "input_error", &e, json),
        };
        if let Some(base_dir) = base_image_dir.as_deref()
            && let Err(e) = append_auto_image_assets(
                &doc,
                base_dir,
                &mut assets,
                args.max_pdf_image_bytes,
                "PDF",
            )
        {
            return fail_json(66, "input_error", &e, json);
        }
        if !args.no_remote_images {
            append_remote_image_assets(
                &image_destinations,
                &mut assets,
                args.max_pdf_image_bytes,
                args.remote_image_timeout_secs,
                json,
            );
        }
        assets
    } else {
        Vec::new()
    };

    let profile = if let Some(prof_str) = &args.profile {
        match crate::Profile::parse(prof_str) {
            Some(p) => Some(p),
            None => {
                return fail_json(
                    64,
                    "usage_error",
                    &format!(
                        "unknown markdown authoring profile: '{prof_str}'. Valid choices: 'commonmark-gfm', 'gfm-plus'"
                    ),
                    json,
                );
            }
        }
    } else {
        None
    };
    // `--to both` run whose PDF render fails never leaves a stale HTML file on
    // disk (previously HTML was written, then a PDF failure returned exit 70
    // with the HTML already committed).
    let html_bytes = if want_html {
        let opts = HtmlOptions {
            theme: theme.clone(),
            title: args.title.clone().or_else(|| frontmatter_title.clone()),
            custom_css: custom_css.clone(),
            allow_raw_html: args.allow_html,
            font_assets: font_assets.clone(),
            image_assets: html_image_assets,
            lang: args.lang.clone().or_else(|| frontmatter_lang.clone()),
            profile,
            toc: args.toc || frontmatter_toc.unwrap_or(false),
            toc_depth: args.toc_depth.or(frontmatter_toc_depth),
            html_font_format: args
                .html_font_format
                .map(HtmlFontFormat::from)
                .unwrap_or_default(),
        };
        if args.interactive_html {
            let html = crate::interactive::render_interactive_html(&doc, &src, &opts);
            Some(html.into_bytes())
        } else {
            match render_html_document(&doc, &opts) {
                Ok(html) => Some(html.into_bytes()),
                Err(e) => return fail_render(e, json),
            }
        }
    } else {
        None
    };

    let pdf_render = if want_pdf {
        let opts = PdfOptions {
            theme: theme.clone(),
            title: args.title.clone().or_else(|| frontmatter_title.clone()),
            author: args.author.clone().or_else(|| frontmatter_author.clone()),
            lang: args.lang.clone().or_else(|| frontmatter_lang.clone()),
            profile,
            metadata_epoch_seconds: pdf_metadata_epoch,
            allow_raw_html: args.allow_html,
            code_line_numbers: args.pdf_line_numbers,
            page_numbers: args.pdf_page_numbers,
            base_font_size: args
                .pdf_base_font_size
                .or_else(|| font_scale.map(|s| s.pdf_base_pt())),
            heading_scale: args.pdf_heading_scale,
            table_font_size: args.pdf_table_font_size,
            image_assets: pdf_image_assets,
            font_assets: font_assets.clone(),
            toc: args.toc || frontmatter_toc.unwrap_or(false),
            toc_depth: args.toc_depth.or(frontmatter_toc_depth),
            fit_to_pages: args.fit_to_pages,
            microtype: args.microtype.into(),
            gradual_demerits: args.typography_homogeneous,
            river_penalty: args.typography_antiriver,
            pareto_line_breaking: args.typography_pareto,
            optimal_pagination: args.pdf_optimal_pagination,
        };
        match render_pdf_with_pdfa(&doc, &opts, &args, json) {
            // Keep render errors typed with a distinct exit code (70 = render
            // failure/unavailable subsystem) as richer PDF validation lands.
            Ok(bytes) => Some((opts, bytes)),
            Err(code) => return code,
        }
    } else {
        None
    };

    // EPUB: one-chapter book from the same AST (bead 28t8). Binary zip cannot
    // stream; like PDF it needs a real path (derived from the input stem).
    let epub_render = if want_epub {
        let opts = HtmlOptions {
            theme: theme.clone(),
            title: args.title.clone().or_else(|| frontmatter_title.clone()),
            custom_css: None,
            allow_raw_html: args.allow_html,
            font_assets: FontAssets::default(),
            image_assets: Vec::new(),
            lang: args.lang.clone().or_else(|| frontmatter_lang.clone()),
            profile,
            toc: args.toc || frontmatter_toc.unwrap_or(false),
            toc_depth: args.toc_depth.or(frontmatter_toc_depth),
            html_font_format: HtmlFontFormat::default(),
        };
        match crate::render_epub(&doc, &opts) {
            Ok(bytes) => Some(bytes),
            Err(e) => return fail_json(70, "render_error", &format!("epub render: {e}"), json),
        }
    } else {
        None
    };
    let epub_path = if epub_render.is_some() {
        match out_path(&args, single, "epub") {
            Some(path) if !is_stdout_path(&path) => Some(path),
            _ => {
                return fail_json(
                    64,
                    "usage_error",
                    "EPUB output requires a real --out <path> (binary zip cannot stream to stdout)",
                    json,
                );
            }
        }
    } else {
        None
    };

    // SVG poster (bead y0vu): text format, so like HTML it may stream to
    // stdout; with a path it rides the staged write.
    let svg_render = if want_svg {
        let opts = crate::svg::SvgOptions {
            theme: theme.clone(),
            ..crate::svg::SvgOptions::default()
        };
        Some(crate::render_svg(&doc, &opts))
    } else {
        None
    };
    let svg_path = if svg_render.is_some() {
        out_path(&args, single, "svg")
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
    // --search-index: deterministic JSON sidecar for docs-site search (r9z4).
    // Built from the same AST; rides the staged write so a failure anywhere
    // rolls back all outputs together.
    let search_index_bytes = if let Some(index_path) = args.search_index.as_deref() {
        let index = crate::search_index::build_search_index(&doc);
        let bytes = crate::search_index::search_index_json(&index).into_bytes();
        Some((index_path, bytes))
    } else {
        None
    };
    if let Some((path, bytes)) = search_index_bytes.as_ref() {
        file_outputs.push(crate::file_write::OutputFile {
            path,
            bytes: bytes.as_slice(),
        });
    }
    if let (Some(path), Some(bytes)) = (html_path.as_deref(), html_bytes.as_deref()) {
        file_outputs.push(crate::file_write::OutputFile { path, bytes });
    }
    if let (Some(path), Some((_, bytes))) = (pdf_path.as_deref(), pdf_render.as_ref()) {
        file_outputs.push(crate::file_write::OutputFile { path, bytes });
    }
    if let (Some(path), Some(bytes)) = (epub_path.as_deref(), epub_render.as_ref()) {
        file_outputs.push(crate::file_write::OutputFile {
            path,
            bytes: bytes.as_slice(),
        });
    }
    // SVG streams to stdout like HTML when no path derived (stdout case), or
    // rides the staged write when a file path exists.
    if let (Some(path), Some(bytes)) = (svg_path.as_deref(), svg_render.as_ref()) {
        if !is_stdout_path(path) {
            file_outputs.push(crate::file_write::OutputFile {
                path,
                bytes: bytes.as_slice(),
            });
        }
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

    if let Some(bytes) = epub_render.as_ref()
        && let Some(path) = epub_path.as_deref()
    {
        report_write("epub", path, bytes.len(), json);
    }
    if let Some(bytes) = svg_render.as_ref() {
        match svg_path.as_deref() {
            Some(path) if !is_stdout_path(path) => report_write("svg", path, bytes.len(), json),
            _ => {
                let mut stdout = std::io::stdout().lock();
                match stdout.write_all(bytes) {
                    Ok(()) => {}
                    Err(e) if e.kind() == IoErrorKind::BrokenPipe => {}
                    Err(e) => {
                        return fail_json(
                            74,
                            "stdout_error",
                            &format!("writing stdout: {e}"),
                            json,
                        );
                    }
                }
            }
        }
    }
    if let Some((_, bytes)) = search_index_bytes.as_ref()
        && let Some(path) = args.search_index.as_deref()
    {
        report_write("search-index", path, bytes.len(), json);
    }

    if let Some((opts, bytes)) = pdf_render.as_ref()
        && let Some(path) = pdf_path.as_deref()
    {
        report_pdf_warnings(&doc, opts, json);
        report_write("pdf", path, bytes.len(), json);
    } else {
        // PDF `render_warnings` is not run on HTML-only output; still surface
        // a weight pin that did not instance so the drop is not silent.
        report_font_pin_warnings(&font_assets, json);
    }

    ExitCode::SUCCESS
}

/// `fmd verify` — render through the same layout+pagination pipeline the PDF
/// writer uses and emit a stable-schema JSON report: per-page text runs,
/// internal-anchor audit, render warnings, horizontal overflow findings, and
/// a content digest (beads yo83.1-3). Exit codes: 0 clean, 1 findings, 2/66
/// usage/input errors, 70 font load failure.
///
/// --links (fjzd): external http(s) link check. HEAD first via the system
/// curl (wget fallback), ranged GET when HEAD is unsupported; results cached
/// as JSONL (url, status, checked_unix) with a TTL. NEVER part of default
/// verify — network is non-deterministic by definition. All fetches honor the
/// project user agent.
fn check_external_links(
    doc: &crate::Document,
    timeout_secs: u64,
    cache_path: Option<&std::path::Path>,
    ttl_secs: u64,
    json: bool,
) -> Vec<crate::verify::VerifyFinding> {
    use std::collections::BTreeMap;

    let urls = collect_external_links(doc);
    if urls.is_empty() {
        return Vec::new();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Cache: read JSONL entries within TTL.
    let mut cache: BTreeMap<String, (u16, bool, u64)> = BTreeMap::new();
    if let Some(path) = cache_path
        && let Ok(text) = std::fs::read_to_string(path)
    {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // entry: {"url":..,"status":NNN,"ok":bool,"checked":NNN}
            let Some(v) = parse_cache_line(line) else {
                continue;
            };
            cache.insert(v.0, (v.1, v.2, v.3));
        }
    }

    let mut findings = Vec::new();
    let mut out_cache: BTreeMap<String, (u16, bool, u64)> = BTreeMap::new();
    for url in &urls {
        let (status, ok) = if let Some((status, ok, checked)) = cache.get(url) {
            if now.saturating_sub(*checked) <= ttl_secs {
                (*status, *ok)
            } else {
                check_one_link(url, timeout_secs)
            }
        } else {
            check_one_link(url, timeout_secs)
        };
        out_cache.insert(url.clone(), (status, ok, now));
        if !ok {
            findings.push(crate::verify::VerifyFinding {
                code: "link_broken",
                detail: format!("external link {url} returned HTTP {status}"),
            });
        } else if (300..400).contains(&status) {
            findings.push(crate::verify::VerifyFinding {
                code: "link_redirected",
                detail: format!("external link {url} redirected (HTTP {status})"),
            });
        }
    }

    // Persist the cache (fresh entries only; concurrent writers last-wins).
    if let Some(path) = cache_path
        && !out_cache.is_empty()
    {
        let mut text = String::new();
        for (url, (status, ok, checked)) in &out_cache {
            let esc = url.replace('\\', "\\\\").replace('"', "\\\"");
            text.push_str(&format!(
                "{{\"url\":\"{esc}\",\"status\":{status},\"ok\":{ok},\"checked\":{checked}}}\n"
            ));
        }
        if let Err(e) = std::fs::write(path, text) {
            eprintln!(
                "fmd: warning: could not write link cache {}: {e}",
                path.display()
            );
        }
    }
    if json {
        eprintln!(
            "fmd: checked {} external link(s), {} broken",
            urls.len(),
            findings.iter().filter(|f| f.code == "link_broken").count()
        );
    }
    findings
}

fn parse_cache_line(line: &str) -> Option<(String, u16, bool, u64)> {
    // Minimal, allocation-light JSON field scrape for our own fixed shape.
    let pick = |key: &str| -> Option<String> {
        let pat = format!("\"{key}\":");
        let start = line.find(&pat)? + pat.len();
        let rest = &line[start..];
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        if let Some(stripped) = rest.strip_prefix('"') {
            let end = stripped.find('"')?;
            Some(stripped[..end].replace("\\\\", "\\").replace("\\\"", "\""))
        } else {
            let end = rest.find([',', '}']).unwrap_or(rest.len());
            Some(rest[..end].to_string())
        }
    };
    let url = pick("url")?;
    let status: u16 = pick("status")?.parse().ok()?;
    let ok: bool = pick("ok")?.parse().ok()?;
    let checked: u64 = pick("checked")?.parse().ok()?;
    Some((url, status, ok, checked))
}

/// One link probe: HEAD, then a ranged GET when HEAD fails (405/501), via the
/// system curl or wget. ok = 2xx (3xx reported separately, not broken).
fn check_one_link(url: &str, timeout_secs: u64) -> (u16, bool) {
    let ua = "--user-agent";
    let ua_owned = format!("fmd/{}", crate::VERSION);
    let ua_val = ua_owned.as_str();
    let timeout = timeout_secs.to_string();
    let head = |program: &str| -> Option<(u16, bool)> {
        let args: Vec<String> = match program {
            "curl" => vec![
                "-sS".into(),
                "-o".into(),
                "/dev/null".into(),
                "-w".into(),
                "%{http_code}".into(),
                "--max-time".into(),
                timeout.clone(),
                "-I".into(),
                ua.into(),
                ua_val.into(),
                url.into(),
            ],
            _ => vec![
                "--server-response".into(),
                "--spider".into(),
                format!("--timeout={timeout}"),
                ua.into(),
                ua_val.into(),
                url.into(),
            ],
        };
        let out = std::process::Command::new(program)
            .args(&args)
            .output()
            .ok()?;
        // curl -w %{http_code} writes the code to stdout; wget
        // --server-response writes headers to stderr. Read the right stream.
        let text = if program == "curl" {
            String::from_utf8_lossy(&out.stdout)
        } else {
            String::from_utf8_lossy(&out.stderr)
        };
        let code: u16 = if program == "curl" {
            // curl's -w output is exactly "NNN\n".
            text.trim()
                .chars()
                .take(3)
                .collect::<String>()
                .parse()
                .ok()?
        } else {
            // wget's --server-response stderr contains lines like
            // "  HTTP/1.1 200 OK"; find the last one and take the code.
            text.lines().rev().find_map(|l| {
                let trimmed = l.trim();
                let rest = trimmed.strip_prefix("HTTP/")?;
                let after_version = rest.split_once(' ')?.1;
                let code_str: String = after_version
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                code_str.parse().ok()
            })?
        };
        Some((code, (200..300).contains(&code)))
    };
    for program in ["curl", "wget"] {
        if let Some((status, ok)) = head(program) {
            if ok || !(status == 405 || status == 501) {
                return (status, ok);
            }
        }
    }
    // GET fallback (range-limited so we never download bodies).
    let timeout_str = timeout_secs.to_string();
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            timeout_str.as_str(),
            "--range",
            "0-0",
            ua,
            ua_val,
            url,
        ])
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            match text
                .trim()
                .chars()
                .take(3)
                .collect::<String>()
                .parse::<u16>()
            {
                Ok(code) => (code, (200..400).contains(&code)),
                Err(_) => (0, false),
            }
        }
        Err(_) => (0, false),
    }
}

fn collect_external_links(doc: &crate::Document) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    fn inlines(inl: &[crate::Inline], out: &mut Vec<String>) {
        for i in inl {
            match i {
                crate::Inline::Link { dest, content, .. } => {
                    let d = dest.as_str();
                    if (d.starts_with("http://") || d.starts_with("https://"))
                        && !out.iter().any(|u| u == d)
                    {
                        out.push(d.to_string());
                    }
                    inlines(content, out);
                }
                crate::Inline::Emphasis(c)
                | crate::Inline::Strong(c)
                | crate::Inline::Strikethrough(c) => inlines(c, out),
                _ => {}
            }
        }
    }
    fn blocks(blk: &[crate::Block], out: &mut Vec<String>) {
        for b in blk {
            match b {
                crate::Block::Paragraph(i) | crate::Block::Heading { inlines: i, .. } => {
                    inlines(i, out);
                }
                crate::Block::BlockQuote(inner) => blocks(inner, out),
                crate::Block::List(list) => {
                    for item in &list.items {
                        blocks(&item.blocks, out);
                    }
                }
                _ => {}
            }
        }
    }
    blocks(&doc.blocks, &mut out);
    out
}

fn run_verify(args: VerifyArgs, global_json: bool, no_color: bool) -> ExitCode {
    // Agent/CI pipes get JSON without `--json` (VERIFY_RECIPE, `> report.json`).
    // A TTY without `--json` gets the caret report. `--json` / global `--json`
    // always force the schema.
    let json = global_json || args.json || !std::io::stdout().is_terminal();
    let Some(path_str) = args.input.to_str() else {
        return fail_json(
            64,
            "usage_error",
            "verify input path must be valid UTF-8",
            json,
        );
    };
    let src = match read_input(Some(path_str), None, DEFAULT_MAX_INPUT_BYTES) {
        Ok(s) => s,
        Err(e) => return fail_json(66, "input_error", &format!("reading input: {e}"), json),
    };
    let doc = parse_markdown(&src);
    // Verification baseline: the default theme/options, deliberately
    // independent of config, so the same document verifies identically on
    // every machine (the digest is a portable change detector).
    let opts = PdfOptions::default();
    let Some(report) = crate::verify::verify_pdf(&doc, &opts) else {
        return fail_json(
            70,
            "font_load_failed",
            "verification could not load the bundled fonts",
            json,
        );
    };
    let report = if args.a11y {
        crate::verify::filter_a11y(report)
    } else {
        report
    };
    // --links (fjzd): external link check via the system fetcher. Adds
    // link_broken / link_redirected findings; pure addition, never in the
    // default path (network is non-deterministic by definition).
    let report = if args.links {
        let extra = check_external_links(
            &doc,
            args.links_timeout_secs,
            args.links_cache.as_deref(),
            args.links_ttl_secs,
            json,
        );
        crate::verify::with_extra_findings(report, extra)
    } else {
        report
    };
    let out_status = if json {
        // JSON contract: stdout carries the report, nothing else. The schema
        // is pinned by golden fixtures and must stay caret-free.
        emit_stdout(&crate::verify::to_json(&report))
    } else {
        // Human mode: caret blocks for each finding pointing back into the
        // source markdown. Color follows stdout (the actual sink), not stderr,
        // so `fmd verify file.md > out.txt` never embeds ANSI.
        let mode = if no_color {
            crate::caret::ColorMode::Never
        } else {
            crate::caret::ColorMode::from_env()
        };
        let style = crate::caret::style_for_stderr(mode, std::io::stdout().is_terminal(), None);
        let human = crate::verify::to_human(&report, &src, Some(path_str), style);
        emit_stdout(&human)
    };
    if out_status != ExitCode::SUCCESS {
        return out_status;
    }
    if report.verdict == "clean" {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
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
        Some(p) => match read_stylesheet(p) {
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
        // Multi-file EPUB is the fmd book epic's job (7tus); a batch run of
        // one-chapter epubs would silently skip the unified-book semantics.
        // SVG posters are a per-document display artifact, not a batch target.
        Target::Epub | Target::Svg => {
            return fail_json(
                64,
                "usage_error",
                "--to epub/svg is not supported in batch; epub books await fmd book (7tus), svg posters are single-document artifacts",
                json,
            );
        }
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
        custom_css,
        ..Default::default()
    };
    let pdf = PdfOptions {
        theme,
        metadata_epoch_seconds: pdf_epoch,
        ..Default::default()
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
        max_pdf_image_bytes: args.max_pdf_image_bytes,
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
            receipt.files.sort_by(|a, b| a.input.cmp(&b.input));
            // stdout is data (the receipt JSON) only with --json; otherwise a
            // human summary goes to stderr and stdout stays empty. A broken pipe
            // is swallowed here so it never panics or overrides the batch result
            // exit code computed below. Any other stdout write failure is an
            // output error and must not be hidden behind the batch status.
            let mut stdout_status = ExitCode::SUCCESS;
            if json {
                stdout_status = emit_stdout(&receipt.to_json());
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
            if stdout_status != ExitCode::SUCCESS {
                return stdout_status;
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
        let initial_cap = if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > max_bytes {
                return Err(input_too_large(&label, meta.len(), max_bytes));
            }
            usize::try_from(meta.len()).unwrap_or(0)
        } else {
            0
        };
        let file = std::fs::File::open(path)?;
        let bytes =
            read_limited_with_capacity(file, max_bytes, initial_cap, &label, "--max-input-bytes")?;
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
    read_limited_with_capacity(reader, max_bytes, 0, label, flag)
}

fn read_limited_with_capacity<R: Read>(
    reader: R,
    max_bytes: u64,
    initial_cap: usize,
    label: &str,
    flag: &str,
) -> std::io::Result<Vec<u8>> {
    let cap = initial_cap.min(usize::try_from(max_bytes).unwrap_or(usize::MAX));
    let mut bytes = Vec::with_capacity(cap);
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

fn read_stylesheet(path: &Path) -> std::io::Result<String> {
    let label = format!("stylesheet {}", path.display());
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > MAX_STYLESHEET_BYTES
    {
        return Err(size_limit_error(
            &label,
            meta.len(),
            MAX_STYLESHEET_BYTES,
            "the stylesheet size limit",
        ));
    }
    let file = std::fs::File::open(path)?;
    let bytes = read_limited_with_flag(
        file,
        MAX_STYLESHEET_BYTES,
        &label,
        "the stylesheet size limit",
    )?;
    string_from_input_bytes(bytes)
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

enum HostFontError {
    Usage(String),
    Input(String),
}

fn load_host_font_assets(
    fonts: &[String],
    weights: &[String],
) -> std::result::Result<FontAssets, HostFontError> {
    let mut assets = FontAssets::default();
    let max_bytes = crate::MAX_FONT_ASSET_BYTES as u64;
    for spec in fonts {
        let (slot, path) = parse_pdf_font_spec(spec).map_err(HostFontError::Usage)?;
        let label = format!("{} font from {}", slot.as_str(), path.display());
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.len() > max_bytes
        {
            return Err(HostFontError::Input(format!(
                "{label} is {} bytes; exceeds the {max_bytes}-byte font-asset limit",
                meta.len()
            )));
        }
        let file = std::fs::File::open(&path)
            .map_err(|e| HostFontError::Input(format!("reading {label}: {e}")))?;
        let bytes = read_limited_with_flag(file, max_bytes, &label, "the font-asset size limit")
            .map_err(|e| HostFontError::Input(format!("reading {label}: {e}")))?;
        assets
            .set_slot(slot, bytes)
            .map_err(|e| HostFontError::Input(e.to_string()))?;
    }
    for spec in weights {
        let (slot, weight) = parse_pdf_font_weight_spec(spec).map_err(HostFontError::Usage)?;
        assets
            .set_slot_weight(slot, weight)
            .map_err(|e| HostFontError::Usage(e.to_string()))?;
    }
    Ok(assets)
}

fn parse_pdf_font_spec(spec: &str) -> std::result::Result<(FontAssetSlot, PathBuf), String> {
    let Some((slot_s, path)) = spec.split_once('=') else {
        return Err(format!(
            "invalid --pdf-font {spec:?}; expected SLOT=PATH, for example --pdf-font body-regular=./Body.ttf"
        ));
    };
    let slot = FontAssetSlot::parse(slot_s).ok_or_else(|| {
        format!(
            "unknown --pdf-font slot '{}'; use body-regular, body-bold, body-italic, body-bold-italic, or mono-regular",
            slot_s.trim()
        )
    })?;
    let path = path.trim();
    if path.is_empty() {
        return Err("invalid --pdf-font: PATH must not be blank".to_string());
    }
    Ok((slot, PathBuf::from(path)))
}

fn parse_pdf_font_weight_spec(spec: &str) -> std::result::Result<(FontAssetSlot, u16), String> {
    let spec = spec.trim();
    if let Some((slot_s, weight_s)) = spec.split_once('=') {
        let slot = FontAssetSlot::parse(slot_s).ok_or_else(|| {
            format!(
                "unknown --pdf-font-weight slot '{}'; use body-regular, body-bold, body-italic, body-bold-italic, or mono-regular",
                slot_s.trim()
            )
        })?;
        return Ok((slot, parse_css_weight(weight_s)?));
    }
    Ok((FontAssetSlot::BodyRegular, parse_css_weight(spec)?))
}

fn parse_css_weight(raw: &str) -> std::result::Result<u16, String> {
    let trimmed = raw.trim();
    let weight = trimmed
        .parse::<u16>()
        .map_err(|_| format!("invalid --pdf-font-weight {trimmed:?}; expected integer 1..=1000"))?;
    if (1..=1000).contains(&weight) {
        Ok(weight)
    } else {
        Err(format!(
            "invalid --pdf-font-weight {weight}; CSS font-weight is 1..=1000"
        ))
    }
}

fn report_font_pin_warnings(assets: &FontAssets, json: bool) {
    for slot in FontAssetSlot::ALL {
        let Some(weight) = assets.slot_weight(slot) else {
            continue;
        };
        if !font_pin_failed_to_instance(assets, slot, weight) {
            continue;
        }
        let warning = RenderWarning::FontWeightIgnoredStatic {
            slot: slot.as_str().to_string(),
            weight,
        };
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

/// True when this slot has host (or VF-shared) bytes and `Font::instance`
/// could not produce a face at `weight`. A pin with no host bytes is a
/// no-op on the bundled path — not a static-face ignore.
fn font_pin_failed_to_instance(assets: &FontAssets, slot: FontAssetSlot, weight: u16) -> bool {
    let Some(bytes) = assets.resolved_bytes(slot) else {
        return false;
    };
    crate::text::Font::parse(bytes.to_vec())
        .ok()
        .and_then(|font| font.instance(f32::from(weight)))
        .is_none()
}

fn report_font_assets(assets: &FontAssets) {
    let mut slots = 0u32;
    for slot in FontAssetSlot::ALL {
        if assets.slot_bytes(slot).is_some() {
            slots += 1;
        }
        if let Some(weight) = assets.slot_weight(slot) {
            eprintln!(
                "{{\"ok\":true,\"event\":\"font_instance\",\"slot\":\"{}\",\"weight\":{}}}",
                slot.as_str(),
                weight
            );
        }
    }
    if slots == 0
        && FontAssetSlot::ALL
            .iter()
            .all(|&s| assets.slot_weight(s).is_none())
    {
        return;
    }
    eprintln!("{{\"ok\":true,\"event\":\"font_assets\",\"phase\":\"load\",\"slots\":{slots}}}");
}

fn read_pdf_image_assets(
    specs: &[String],
    max_bytes: u64,
    destination_hints: &[&str],
) -> std::result::Result<Vec<PdfImageAsset>, PdfImageError> {
    let mut assets = Vec::with_capacity(specs.len());
    for spec in specs {
        let (destination, path) =
            parse_pdf_image_spec(spec, destination_hints).map_err(PdfImageError::Usage)?;
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

fn auto_pdf_image_base_dir(input: Option<&str>, text: Option<&str>) -> Option<PathBuf> {
    if text.is_some() {
        return None;
    }
    let input = input?;
    if input == "-" {
        return None;
    }
    let input_path = Path::new(input);
    Some(
        input_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    )
}

fn append_auto_image_assets(
    doc: &Document,
    base_dir: &Path,
    assets: &mut Vec<PdfImageAsset>,
    max_bytes: u64,
    output_kind: &str,
) -> std::result::Result<(), String> {
    let Ok(canonical_base_dir) = std::fs::canonicalize(base_dir) else {
        return Ok(());
    };
    let mut destinations = Vec::new();
    collect_image_destinations(&doc.blocks, &mut destinations);
    for destination in destinations {
        let destination = destination.trim();
        if destination.is_empty()
            || assets
                .iter()
                .any(|asset| asset.destination.trim() == destination)
        {
            continue;
        }
        let Some(path) = auto_pdf_image_path(destination, base_dir) else {
            continue;
        };
        let Ok(canonical_path) = std::fs::canonicalize(&path) else {
            continue;
        };
        if !canonical_path.starts_with(&canonical_base_dir) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&canonical_path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let label = format!(
            "auto {output_kind} image asset {destination} from {}",
            path.display()
        );
        if meta.len() > max_bytes {
            return Err(pdf_image_too_large(&label, meta.len(), max_bytes).to_string());
        }
        let file =
            std::fs::File::open(&canonical_path).map_err(|e| format!("reading {label}: {e}"))?;
        let bytes = read_limited_with_flag(file, max_bytes, &label, "--max-pdf-image-bytes")
            .map_err(|e| format!("reading {label}: {e}"))?;
        assets.push(PdfImageAsset::new(destination, bytes));
    }
    Ok(())
}

fn collect_image_destinations<'a>(blocks: &'a [Block], out: &mut Vec<&'a str>) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                collect_image_destinations_inlines(inlines, out);
            }
            Block::BlockQuote(inner) => collect_image_destinations(inner, out),
            Block::FootnoteDefinition { blocks: inner, .. } => {
                collect_image_destinations(inner, out);
            }
            Block::List(list) => {
                for item in &list.items {
                    collect_image_destinations(&item.blocks, out);
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for term in &item.terms {
                        collect_image_destinations_inlines(term, out);
                    }
                    for def in &item.definitions {
                        collect_image_destinations_inlines(def, out);
                    }
                }
            }
            Block::Table(table) => {
                for cell in &table.head {
                    collect_image_destinations_inlines(cell, out);
                }
                for row in &table.rows {
                    for cell in row {
                        collect_image_destinations_inlines(cell, out);
                    }
                }
            }
            Block::CodeBlock { .. }
            | Block::ThematicBreak
            | Block::HtmlBlock(_)
            | Block::MathBlock(_)
            | Block::PageBreak => {}
        }
    }
}

fn collect_image_destinations_inlines<'a>(inlines: &'a [Inline], out: &mut Vec<&'a str>) {
    for inline in inlines {
        match inline {
            Inline::Image { dest, .. } => out.push(dest),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children)
            | Inline::Link {
                content: children, ..
            } => collect_image_destinations_inlines(children, out),
            Inline::FootnoteRef { .. } => {}
            Inline::Text(_)
            | Inline::Code(_)
            | Inline::Math(_)
            | Inline::DisplayMath(_)
            | Inline::SoftBreak
            | Inline::HardBreak
            | Inline::Html(_) => {}
        }
    }
}

fn auto_pdf_image_path(destination: &str, base_dir: &Path) -> Option<PathBuf> {
    let path_part = destination
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if path_part.is_empty()
        || path_part.starts_with("//")
        || path_part.contains('\\')
        || has_uri_scheme(path_part)
    {
        return None;
    }
    let relative = Path::new(path_part);
    if relative.is_absolute() || !has_supported_pdf_image_extension(relative) {
        return None;
    }
    let mut has_normal_component = false;
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(_) => has_normal_component = true,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    has_normal_component.then(|| base_dir.join(relative))
}

fn has_uri_scheme(value: &str) -> bool {
    let first_path_part = value
        .split(['/', '\\', '?', '#'])
        .next()
        .unwrap_or_default();
    let Some((scheme, _)) = first_path_part.split_once(':') else {
        return false;
    };
    let mut bytes = scheme.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn has_supported_pdf_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "svg" | "jpg" | "jpeg"
            )
        })
}

/// The trimmed destination when it is a remote image URL the CLI may fetch.
/// Only literal `http://` / `https://` destinations qualify; every other
/// scheme (or scheme-less path) is left to the local-asset machinery.
fn remote_image_url(destination: &str) -> Option<&str> {
    let dest = destination.trim();
    for prefix in ["https://", "http://"] {
        if dest.len() > prefix.len()
            && dest
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return Some(dest);
        }
    }
    None
}

/// Fetch remote http(s) image destinations that no explicit `--pdf-image`
/// mapping or auto-loaded local file already resolved, so hotlinked images
/// render in PDFs just like they do in the HTML preview (GH issue #2).
///
/// Every failure is non-fatal: a warning is reported and the destination
/// degrades to alt text through the renderer's existing unresolved-image
/// path, which keeps offline renders working. The render core itself never
/// performs network I/O — fetched bytes enter it as ordinary caller-supplied
/// assets, and a fixed set of bytes still renders byte-identically.
fn append_remote_image_assets(
    destinations: &[&str],
    assets: &mut Vec<PdfImageAsset>,
    max_bytes: u64,
    timeout_secs: u64,
    json: bool,
) {
    let mut attempted: Vec<&str> = Vec::new();
    for destination in destinations {
        let Some(url) = remote_image_url(destination) else {
            continue;
        };
        if attempted.contains(&url) || assets.iter().any(|asset| asset.destination.trim() == url) {
            continue;
        }
        attempted.push(url);
        match fetch_remote_image(url, max_bytes, timeout_secs) {
            Ok(bytes) => assets.push(PdfImageAsset::new(url, bytes)),
            Err(reason) => report_remote_image_warning(url, &reason, json),
        }
    }
}

/// Download one remote image via the system `curl` (preferred) or `wget`,
/// with a hard timeout, an HTTP(S)-only protocol allowlist, bounded
/// redirects, and the caller's byte cap enforced while the body is read.
fn fetch_remote_image(
    url: &str,
    max_bytes: u64,
    timeout_secs: u64,
) -> std::result::Result<Vec<u8>, String> {
    let timeout = timeout_secs.max(1).to_string();
    let user_agent = format!("fmd/{}", crate::VERSION);
    let max_bytes_s = max_bytes.to_string();
    let curl_args = [
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--max-redirs",
        "5",
        "--proto",
        "=http,https",
        "--proto-redir",
        "=http,https",
        "--max-time",
        timeout.as_str(),
        "--max-filesize",
        max_bytes_s.as_str(),
        "--user-agent",
        user_agent.as_str(),
        "--output",
        "-",
        "--",
        url,
    ];
    match run_capped_fetch("curl", &curl_args, url, max_bytes) {
        Ok(bytes) => return Ok(bytes),
        Err(FetchSpawn::NotFound) => {}
        Err(FetchSpawn::Failed(reason)) => return Err(reason),
    }

    let timeout_flag = format!("--timeout={timeout}");
    let ua_flag = format!("--user-agent={user_agent}");
    let wget_args = wget_remote_image_args(url, &timeout_flag, &ua_flag);
    match run_capped_fetch("wget", &wget_args, url, max_bytes) {
        Ok(bytes) => Ok(bytes),
        Err(FetchSpawn::NotFound) => Err(
            "neither curl nor wget is available; pass --pdf-image 'URL=PATH' or use \
             --no-remote-images"
                .to_string(),
        ),
        Err(FetchSpawn::Failed(reason)) => Err(reason),
    }
}

enum FetchSpawn {
    NotFound,
    Failed(String),
}

/// GNU wget has no `--proto` jail. `--https-only` blocks `file://` (and
/// `ftp://`) redirects when the requested URL is already HTTPS. HTTP URLs
/// keep following redirects; the initial URL is still http(s)-only.
fn wget_remote_image_args<'a>(
    url: &'a str,
    timeout_flag: &'a str,
    user_agent_flag: &'a str,
) -> Vec<&'a str> {
    let mut args = vec![
        "--quiet",
        "--tries=1",
        timeout_flag,
        "--max-redirect=5",
        user_agent_flag,
        "--output-document=-",
    ];
    if url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://") {
        args.push("--https-only");
    }
    args.push("--");
    args.push(url);
    args
}

fn run_capped_fetch(
    tool: &str,
    args: &[&str],
    url: &str,
    max_bytes: u64,
) -> std::result::Result<Vec<u8>, FetchSpawn> {
    let mut child = match std::process::Command::new(tool)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == IoErrorKind::NotFound => return Err(FetchSpawn::NotFound),
        Err(e) => return Err(FetchSpawn::Failed(format!("running {tool}: {e}"))),
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FetchSpawn::Failed(format!(
                "{tool} produced no stdout pipe"
            )));
        }
    };
    let label = format!("remote image {url}");
    let bytes = match read_limited_with_flag(stdout, max_bytes, &label, "--max-pdf-image-bytes") {
        Ok(bytes) => bytes,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FetchSpawn::Failed(e.to_string()));
        }
    };
    let mut stderr = Vec::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_end(&mut stderr);
    }
    let status = match child.wait() {
        Ok(status) => status,
        Err(e) => return Err(FetchSpawn::Failed(format!("waiting for {tool}: {e}"))),
    };
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        return Err(FetchSpawn::Failed(if detail.is_empty() {
            format!("{tool} exited with {status}")
        } else {
            format!("{tool}: {detail}")
        }));
    }
    if bytes.is_empty() {
        return Err(FetchSpawn::Failed(format!("{tool} returned an empty body")));
    }
    Ok(bytes)
}

fn report_remote_image_warning(url: &str, reason: &str, json: bool) {
    let message = format!(
        "fetching remote image '{url}': {reason}; rendered as alt text \
         (map it with --pdf-image '{url}=PATH' or silence fetching with --no-remote-images)"
    );
    if json {
        eprintln!(
            "{{\"ok\":true,\"event\":\"warning\",\"warning\":\"remote_image_fetch_failed\",\"detail\":\"{}\"}}",
            json_escape(&message)
        );
    } else {
        eprintln!("fmd: warning: {message}");
    }
}

fn parse_pdf_image_spec(
    spec: &str,
    destination_hints: &[&str],
) -> std::result::Result<(String, PathBuf), String> {
    let Some((dest, path)) = split_pdf_image_spec(spec, destination_hints) else {
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

fn split_pdf_image_spec<'a>(
    spec: &'a str,
    destination_hints: &[&str],
) -> Option<(&'a str, &'a str)> {
    let mut first_nonblank = None;
    let mut first_existing_path = None;
    for (idx, _) in spec.match_indices('=') {
        let dest = &spec[..idx];
        let path = &spec[idx + 1..];
        let dest = dest.trim();
        let path = path.trim();
        if destination_hints.iter().any(|hint| hint.trim() == dest) {
            return Some((dest, path));
        }
        if dest.is_empty() || path.is_empty() {
            continue;
        }
        if first_nonblank.is_none() {
            first_nonblank = Some((dest, path));
        }
        if first_existing_path.is_none() && std::fs::metadata(path).is_ok() {
            first_existing_path = Some((dest, path));
        }
    }
    first_existing_path
        .or(first_nonblank)
        .or_else(|| spec.rsplit_once('='))
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

fn expand_file_includes(
    src: &str,
    input_path: &str,
    max_input_bytes: u64,
) -> std::result::Result<String, String> {
    let input_p = Path::new(input_path);
    let base_dir = input_p.parent().unwrap_or_else(|| Path::new("."));
    // Sandbox root: the canonical directory of the TOP input. Every include
    // (including nested ones, which carry the including file's path as origin)
    // must canonicalize inside it — `..` escapes are refused with a stable
    // include_escape detail instead of being silently read.
    let root = std::fs::canonicalize(base_dir)
        .map_err(|e| format!("include sandbox root {}: {e}", base_dir.display()))?;
    crate::transclude::expand_includes(src, &|rel_path, origin| {
        let path = if origin == "<input>" {
            base_dir.join(rel_path)
        } else {
            Path::new(origin)
                .parent()
                .unwrap_or(base_dir)
                .join(rel_path)
        };
        let canon = match std::fs::canonicalize(&path) {
            Ok(c) => c,
            Err(_) => return Ok(None), // missing: core reports include_missing
        };
        if !canon.starts_with(&root) {
            return Err(format!(
                "include_escape: {} leaves the document root {}",
                path.display(),
                root.display()
            ));
        }
        let bytes = match std::fs::read(&canon) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        if (bytes.len() as u64) > max_input_bytes {
            return Err(format!(
                "include_oversize: {} is {} bytes, over the {}-byte input cap",
                path.display(),
                bytes.len(),
                max_input_bytes
            ));
        }
        match String::from_utf8(bytes) {
            Ok(text) => Ok(Some((text, canon.to_string_lossy().into_owned()))),
            Err(_) => Err(format!(
                "include_invalid_utf8: {} is not UTF-8",
                path.display()
            )),
        }
    })
    .map_err(|e| e.to_string())
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
    if single && (ext == "html" || ext == "svg") {
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

fn parse_pdf_a_settings(
    args: &RenderArgs,
    json: bool,
) -> std::result::Result<PdfASettings, ExitCode> {
    if args.pdf_a_strict && args.pdf_a.is_none() {
        return Err(fail_json(
            64,
            "usage_error",
            "--pdf-a-strict requires --pdf-a 2b",
            json,
        ));
    }
    let Some(raw) = args.pdf_a.as_deref() else {
        return Ok(PdfASettings::OFF);
    };
    let Some(mode) = PdfAMode::parse(raw) else {
        return Err(fail_json(
            64,
            "usage_error",
            &format!("unknown --pdf-a {raw:?}; use 2b (PDF/A-2b) or off"),
            json,
        ));
    };
    Ok(PdfASettings {
        mode,
        strict: args.pdf_a_strict,
    })
}

fn render_pdf_with_pdfa(
    doc: &Document,
    opts: &PdfOptions,
    args: &RenderArgs,
    json: bool,
) -> std::result::Result<Vec<u8>, ExitCode> {
    let settings = parse_pdf_a_settings(args, json)?;
    if json && settings.mode.is_a2b() {
        eprintln!(
            "{{\"ok\":true,\"event\":\"pdf_a\",\"profile\":\"{}\",\"strict\":{}}}",
            settings.mode.as_str(),
            settings.strict
        );
    }
    let rendered = if settings.mode.is_a2b() {
        render_pdf_document_pdfa(doc, opts, settings)
    } else {
        render_pdf_document(doc, opts)
    };
    rendered.map_err(|e| fail_render(e, json))
}

/// Print non-fatal PDF render warnings (degraded content that would otherwise be
/// dropped silently) so they are never invisible. In `--json` mode each warning
/// is its own JSONL object before the `wrote` envelope; otherwise a plain line.
fn report_pdf_warnings(doc: &Document, opts: &PdfOptions, json: bool) {
    for warning in render_warnings(doc, opts) {
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

fn run_doctor(args: DoctorArgs, global_json: bool) -> ExitCode {
    match args.command {
        None => run_doctor_health(global_json || args.json),
        Some(DoctorCommand::Fonts(fonts)) => run_doctor_fonts(fonts, global_json || args.json),
    }
}

fn run_doctor_fonts(args: DoctorFontsArgs, parent_json: bool) -> ExitCode {
    let json = parent_json || args.json;
    if args.corpus.as_os_str().is_empty() {
        return fail_json(
            64,
            "usage_error",
            "fmd doctor fonts requires --corpus <dir-or-file>",
            json,
        );
    }
    let paths = match font_coverage::expand_corpus(&args.corpus) {
        Ok(p) => p,
        Err(e) => return fail_json(66, "input_error", &e, json),
    };
    if json {
        eprintln!(
            "{{\"ok\":true,\"event\":\"doctor_fonts\",\"phase\":\"scan\",\"files\":{},\"corpus\":\"{}\"}}",
            paths.len(),
            json_escape(&args.corpus.display().to_string())
        );
    }
    let report = match font_coverage::audit_files(&paths) {
        Ok(r) => r,
        Err(e) => return fail_json(66, "input_error", &e, json),
    };
    let out = if json {
        report.to_json()
    } else {
        report.to_human()
    };
    let status = emit_stdout(&out);
    if status != ExitCode::SUCCESS {
        return status;
    }
    if font_coverage::report_has_gaps(&report) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_doctor_health(json: bool) -> ExitCode {
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

fn run_stats(args: StatsArgs, global_json: bool) -> ExitCode {
    let json = args.json || global_json;
    let input_path = args.input.display().to_string();
    let md = match read_input(
        if args.text.is_some() {
            None
        } else {
            Some(&input_path)
        },
        args.text.as_deref(),
        args.max_input_bytes,
    ) {
        Ok(s) => s,
        Err(e) => {
            return fail_json(
                66,
                "input_error",
                &format!("reading input {}: {e}", args.input.display()),
                json,
            );
        }
    };
    let doc = crate::parse_markdown(&md);
    let stats = crate::doc_stats::compute_doc_stats(&md, &doc);
    if json {
        emit_stdout(&stats.to_json())
    } else {
        emit_stdout(&stats.to_human_report())
    }
}

fn run_diff(args: DiffArgs, global_json: bool, no_config: bool) -> ExitCode {
    let json = args.json || global_json;
    let old_path = args.old_file.display().to_string();
    let new_path = args.new_file.display().to_string();

    let old_md = match read_input(Some(&old_path), None, args.max_input_bytes) {
        Ok(s) => s,
        Err(e) => {
            return fail_json(66, "input_error", &format!("reading {old_path}: {e}"), json);
        }
    };
    let new_md = match read_input(Some(&new_path), None, args.max_input_bytes) {
        Ok(s) => s,
        Err(e) => {
            return fail_json(66, "input_error", &format!("reading {new_path}: {e}"), json);
        }
    };

    let doc_a = crate::parse_markdown(&old_md);
    let doc_b = crate::parse_markdown(&new_md);
    let diff = crate::diff::compute_diff(&doc_a, &doc_b, &old_path, &new_path);

    if json {
        return emit_stdout(&diff.to_json());
    }

    let theme = if no_config {
        Theme::default()
    } else {
        match load_config(false) {
            Ok(c) => c.to_theme(),
            Err(_) => Theme::default(),
        }
    };

    let html_out = diff.to_html(&theme);
    if let Some(out_path) = args.out {
        if out_path.as_os_str() == "-" {
            emit_stdout(&html_out)
        } else {
            match std::fs::write(&out_path, &html_out) {
                Ok(()) => {
                    eprintln!("fmd: wrote diff HTML -> {}", out_path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(
                    73,
                    &format!("writing diff HTML to {}: {e}", out_path.display()),
                ),
            }
        }
    } else {
        emit_stdout(&html_out)
    }
}

fn run_book(args: BookArgs, global_json: bool, no_config: bool) -> ExitCode {
    let json = args.json || global_json;
    if !args.input.exists() {
        return fail_json(
            66,
            "input_error",
            &format!("book input directory not found: {}", args.input.display()),
            json,
        );
    }
    if !args.input.is_dir() {
        return fail_json(
            66,
            "input_error",
            &format!("book input is not a directory: {}", args.input.display()),
            json,
        );
    }
    match args.to {
        Target::Html | Target::Pdf | Target::Both => {}
        Target::Epub | Target::Svg => {
            return fail_json(
                64,
                "usage_error",
                "--to epub/svg is not supported for fmd book",
                json,
            );
        }
    }

    let mut files = Vec::new();
    if let Err(e) = collect_markdown_files(&args.input, &mut files) {
        return fail_json(
            66,
            "input_error",
            &format!("walking {}: {e}", args.input.display()),
            json,
        );
    }

    let manifest_path = args.input.join("book.toml");
    let mut manifest_order = Vec::new();
    let mut manifest_title = None;
    if manifest_path.is_file() {
        if let Ok(manifest_src) = std::fs::read_to_string(&manifest_path) {
            parse_book_manifest(&manifest_src, &mut manifest_order, &mut manifest_title);
        }
    }

    if !manifest_order.is_empty() {
        let mut ordered_files = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for item in manifest_order {
            let item_path = args.input.join(&item);
            if item_path.is_file() {
                ordered_files.push(item_path.clone());
                seen.insert(item_path);
            }
        }
        for f in files {
            if !seen.contains(&f) {
                ordered_files.push(f);
            }
        }
        files = ordered_files;
    } else {
        files.sort_by(|a, b| {
            let rel_a = a.strip_prefix(&args.input).unwrap_or(a);
            let rel_b = b.strip_prefix(&args.input).unwrap_or(b);
            rel_a.cmp(rel_b)
        });
    }

    if files.is_empty() {
        return fail_json(
            66,
            "input_error",
            &format!("no Markdown files found in {}", args.input.display()),
            json,
        );
    }

    let mut inputs = Vec::with_capacity(files.len());
    let mut file_byte_counts = Vec::with_capacity(files.len());
    for file_path in &files {
        let rel = file_path
            .strip_prefix(&args.input)
            .unwrap_or(file_path)
            .to_string_lossy()
            .replace('\\', "/");
        let path_str = file_path.to_string_lossy();
        let raw = match read_input(Some(&path_str), None, args.max_input_bytes) {
            Ok(s) => s,
            Err(e) => {
                return fail_json(
                    66,
                    "input_error",
                    &format!("reading {}: {e}", file_path.display()),
                    json,
                );
            }
        };
        file_byte_counts.push(raw.len());
        let expanded = if crate::transclude::has_includes(&raw) {
            match expand_file_includes(&raw, &path_str, args.max_input_bytes) {
                Ok(s) => s,
                Err(e) => return fail_json(66, "include_error", &e, json),
            }
        } else {
            raw
        };
        inputs.push(crate::book::BookInput {
            path: rel,
            source: expanded,
        });
    }

    let book = match crate::book::build_book(&inputs) {
        Ok(b) => b,
        Err(e) => return fail_json(66, "book_error", &e.to_string(), json),
    };

    let dir_name = args
        .input
        .file_name()
        .map(|n| n.to_string_lossy())
        .filter(|s| !s.is_empty() && s != ".")
        .unwrap_or(std::borrow::Cow::Borrowed("book"));

    let (site_dir, pdf_path) = match &args.out_dir {
        Some(out) => {
            if args.to == Target::Pdf && out.extension().is_some_and(|e| e == "pdf") {
                (
                    out.parent()
                        .unwrap_or(Path::new(""))
                        .join(format!("{dir_name}-site")),
                    out.clone(),
                )
            } else {
                (out.clone(), out.join(format!("{dir_name}.pdf")))
            }
        }
        None => {
            let parent = args.input.parent().unwrap_or(Path::new(""));
            (
                parent.join(format!("{dir_name}-site")),
                parent.join(format!("{dir_name}.pdf")),
            )
        }
    };

    let theme = if no_config {
        Theme::default()
    } else {
        match load_config(false) {
            Ok(c) => c.to_theme(),
            Err(_) => Theme::default(),
        }
    };

    let known_pages: std::collections::BTreeSet<String> =
        book.chapters.iter().map(|c| c.out_name.clone()).collect();
    let mut outputs = Vec::new();
    let mut unresolved_count = 0;

    for chapter in &book.chapters {
        unresolved_count += count_unresolved_links(&chapter.doc, &known_pages);
    }
    if unresolved_count > 0 && !json {
        eprintln!("fmd: warning: found {unresolved_count} unresolved cross-file links");
    }

    if matches!(args.to, Target::Html | Target::Both) {
        if let Err(e) = std::fs::create_dir_all(&site_dir) {
            return fail_json(
                73,
                "output_error",
                &format!("creating site dir {}: {e}", site_dir.display()),
                json,
            );
        }
        let html_opts = HtmlOptions {
            theme: theme.clone(),
            ..HtmlOptions::default()
        };

        for chapter in &book.chapters {
            let mut doc = chapter.doc.clone();
            crate::book::rewrite_links_for_site(&mut doc, &known_pages);
            let rendered = match crate::render_html_document(&doc, &html_opts) {
                Ok(s) => s,
                Err(e) => return fail_json(70, "html_render_error", &e.to_string(), json),
            };
            let final_html = crate::book::inject_book_nav(&rendered, &book, &chapter.out_name);
            let out_file = site_dir.join(&chapter.out_name);
            if let Err(e) = std::fs::write(&out_file, final_html.as_bytes()) {
                return fail_json(
                    73,
                    "output_error",
                    &format!("writing {}: {e}", out_file.display()),
                    json,
                );
            }
            outputs.push(out_file.display().to_string());
        }

        if let Some(first) = book.chapters.first() {
            let escaped_url = crate::book::escape_attr_pub(&first.out_name);
            let escaped_title = crate::book::escape_text_pub(&first.title);
            let index_html = format!(
                "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0; url={escaped_url}\"><title>Redirecting to {escaped_title}</title></head><body><p>Redirecting to <a href=\"{escaped_url}\">{escaped_title}</a>...</p></body></html>\n"
            );
            let index_file = site_dir.join("index.html");
            if let Err(e) = std::fs::write(&index_file, index_html.as_bytes()) {
                return fail_json(
                    73,
                    "output_error",
                    &format!("writing {}: {e}", index_file.display()),
                    json,
                );
            }
            outputs.push(index_file.display().to_string());
        }
    }

    let mut pdf_pages = 0;
    if matches!(args.to, Target::Pdf | Target::Both) {
        let pdf_doc = crate::book::book_pdf_document(&book);
        let mut pdf_opts = PdfOptions {
            theme,
            toc: true,
            ..PdfOptions::default()
        };
        if let Some(title) = manifest_title.or_else(|| {
            book.chapters
                .first()
                .and_then(|c| c.frontmatter.as_ref())
                .and_then(|fm| fm.title.clone())
        }) {
            pdf_opts.title = Some(title);
        }
        if let Some(author) = book
            .chapters
            .first()
            .and_then(|c| c.frontmatter.as_ref())
            .and_then(|fm| fm.author.clone())
        {
            pdf_opts.author = Some(author);
        }
        let pdf_bytes = match crate::render_pdf_document(&pdf_doc, &pdf_opts) {
            Ok(b) => b,
            Err(e) => return fail_json(70, "pdf_render_error", &e.to_string(), json),
        };
        pdf_pages = count_pdf_pages(&pdf_bytes);
        if let Some(p) = pdf_path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if let Err(e) = std::fs::write(&pdf_path, &pdf_bytes) {
            return fail_json(
                73,
                "output_error",
                &format!("writing {}: {e}", pdf_path.display()),
                json,
            );
        }
        outputs.push(pdf_path.display().to_string());
    }

    if json {
        let mut files_json = Vec::new();
        for (i, ch) in book.chapters.iter().enumerate() {
            let bytes = file_byte_counts.get(i).copied().unwrap_or(0);
            files_json.push(format!(
                "{{\"path\":\"{}\",\"out_name\":\"{}\",\"title\":\"{}\",\"bytes\":{bytes}}}",
                json_escape(&ch.path),
                json_escape(&ch.out_name),
                json_escape(&ch.title),
            ));
        }
        let outputs_json: Vec<_> = outputs
            .iter()
            .map(|o| format!("\"{}\"", json_escape(o)))
            .collect();
        let receipt = format!(
            "{{\"ok\":true,\"tool\":\"fmd\",\"command\":\"book\",\"input\":\"{}\",\"chapters\":{},\"files\":[{}],\"unresolved_links\":{},\"pages\":{},\"outputs\":[{}]}}",
            json_escape(&args.input.display().to_string()),
            book.chapters.len(),
            files_json.join(","),
            unresolved_count,
            pdf_pages,
            outputs_json.join(",")
        );
        return emit_stdout(&receipt);
    }

    eprintln!("fmd: assembled book ({} chapters)", book.chapters.len());
    if matches!(args.to, Target::Html | Target::Both) {
        eprintln!("fmd: wrote HTML site -> {}", site_dir.display());
    }
    if matches!(args.to, Target::Pdf | Target::Both) {
        eprintln!("fmd: wrote PDF book -> {}", pdf_path.display());
    }
    ExitCode::SUCCESS
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, out)?;
        } else if path.is_file() && (name_str.ends_with(".md") || name_str.ends_with(".markdown")) {
            out.push(path);
        }
    }
    Ok(())
}

fn parse_book_manifest(src: &str, order: &mut Vec<String>, title: &mut Option<String>) {
    let mut in_order_array = false;
    for raw_line in src.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if in_order_array {
            let chunk = if let Some(idx) = line.find(']') {
                in_order_array = false;
                &line[..idx]
            } else {
                line
            };
            for item in chunk.split(',') {
                let s = item.trim().trim_matches('"').trim_matches('\'').trim();
                if !s.is_empty() {
                    order.push(s.to_string());
                }
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            if key == "title" {
                *title = Some(val.trim_matches('"').trim_matches('\'').to_string());
            } else if key == "order" || key == "chapters" {
                if let Some(open_idx) = val.find('[') {
                    let after_open = &val[open_idx + 1..];
                    let (chunk, closed) = if let Some(close_idx) = after_open.find(']') {
                        (&after_open[..close_idx], true)
                    } else {
                        (after_open, false)
                    };
                    for item in chunk.split(',') {
                        let s = item.trim().trim_matches('"').trim_matches('\'').trim();
                        if !s.is_empty() {
                            order.push(s.to_string());
                        }
                    }
                    if !closed {
                        in_order_array = true;
                    }
                }
            }
        }
    }
}

fn count_unresolved_links(doc: &Document, known: &std::collections::BTreeSet<String>) -> usize {
    let mut count = 0;
    count_block_unresolved(&doc.blocks, known, &mut count);
    count
}

fn count_block_unresolved(
    blocks: &[Block],
    known: &std::collections::BTreeSet<String>,
    count: &mut usize,
) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                count_inline_unresolved(inlines, known, count);
            }
            Block::BlockQuote(inner) => count_block_unresolved(inner, known, count),
            Block::List(list) => {
                for item in &list.items {
                    count_block_unresolved(&item.blocks, known, count);
                }
            }
            Block::Table(table) => {
                for cell in &table.head {
                    count_inline_unresolved(cell, known, count);
                }
                for row in &table.rows {
                    for cell in row {
                        count_inline_unresolved(cell, known, count);
                    }
                }
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for term in &item.terms {
                        count_inline_unresolved(term, known, count);
                    }
                    for def in &item.definitions {
                        count_inline_unresolved(def, known, count);
                    }
                }
            }
            Block::FootnoteDefinition { blocks, .. } => {
                count_block_unresolved(blocks, known, count)
            }
            _ => {}
        }
    }
}

fn count_inline_unresolved(
    inlines: &[Inline],
    known: &std::collections::BTreeSet<String>,
    count: &mut usize,
) {
    for inl in inlines {
        match inl {
            Inline::Link { dest, content, .. } => {
                if !dest.starts_with("http://")
                    && !dest.starts_with("https://")
                    && !dest.starts_with("//")
                    && !dest.starts_with("mailto:")
                    && !dest.starts_with("data:")
                {
                    let target = dest.split_once('#').map_or(dest.as_str(), |(p, _)| p);
                    let target_lower = target.to_ascii_lowercase();
                    if target_lower.ends_with(".md") || target_lower.ends_with(".markdown") {
                        let page_html = crate::book::out_name(target);
                        if !known.contains(&page_html) {
                            *count += 1;
                        }
                    }
                }
                count_inline_unresolved(content, known, count);
            }
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                count_inline_unresolved(c, known, count);
            }
            _ => {}
        }
    }
}

fn count_pdf_pages(bytes: &[u8]) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"/Type /Page") {
            let next = bytes.get(i + 11);
            if matches!(next, Some(b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')) {
                count += 1;
                i += 11;
                continue;
            }
        } else if bytes[i..].starts_with(b"/Type/Page") {
            let next = bytes.get(i + 10);
            if matches!(next, Some(b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')) {
                count += 1;
                i += 10;
                continue;
            }
        }
        i += 1;
    }
    count.max(1)
}

fn print_capabilities() -> ExitCode {
    emit_stdout(&format!(
        "{{\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"commands\":[{{\"name\":\"render\",\"examples\":[\"fmd README.md\",\"fmd - < README.md\",\"fmd --text '# Hello' --out hello.html\",\"fmd --text '# Hello' --out - > hello.html\",\"fmd render README.md --to both --out README.html\",\"fmd README.md --to pdf --out README.pdf\",\"fmd README.md --to pdf --pdf-line-numbers --out README.pdf\",\"fmd README.md --to pdf --microtype expansion --out README.pdf\",\"fmd README.md --to pdf --typography-homogeneous --out README.pdf\",\"fmd README.md --to pdf --typography-antiriver --out README.pdf\",\"fmd README.md --to pdf --pdf-optimal-pagination --out README.pdf\",\"fmd README.md --to pdf --typography-pareto --out README.pdf\",\"fmd README.md --to pdf --pdf-image images/chart.png=./chart.png --out README.pdf\",\"fmd README.md --to pdf --pdf-font body-regular=./Var.ttf --pdf-font-weight 650 --out README.pdf\",\"fmd README.md --to pdf --pdf-a 2b --out README.pdf\",\"fmd README.md --to pdf --title 'Quarterly Memo' --author 'FMD' --out README.pdf\",\"SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf\",\"fmd --max-input-bytes 1048576 README.md --out README.html\"]}},{{\"name\":\"diff\",\"examples\":[\"fmd diff v1.md v2.md\",\"fmd diff v1.md v2.md --out diff.html\",\"fmd diff v1.md v2.md --json\"]}},{{\"name\":\"stats\",\"examples\":[\"fmd stats README.md\",\"fmd stats README.md --json\",\"fmd stats --text '# Hello' --json\",\"fmd stats - < README.md\"]}},{{\"name\":\"book\",\"examples\":[\"fmd book ./docs --out-dir ./site\",\"fmd book ./docs --to pdf --out-dir ./dist\",\"fmd book ./docs --json\"]}},{{\"name\":\"config\",\"examples\":[\"fmd config show --json\",\"fmd config set font serif --json\",\"fmd --no-config README.md --out README.html\"]}},{{\"name\":\"capabilities\",\"examples\":[\"fmd capabilities --json\"]}},{{\"name\":\"robot-docs guide\",\"examples\":[\"fmd robot-docs guide\"]}},{{\"name\":\"doctor\",\"examples\":[\"fmd doctor --json\",\"fmd doctor fonts --corpus ./docs --json\"]}},{{\"name\":\"verify\",\"examples\":[\"fmd verify doc.md --json\",\"fmd verify doc.md --a11y\"]}},{{\"name\":\"watch\",\"examples\":[\"fmd watch README.md --out README.html\",\"fmd watch README.md --out README.html --serve\",\"fmd watch README.md --out README.html --serve --measure 21\",\"fmd watch README.md --to pdf --out README.pdf --interval 300\"]}},{{\"name\":\"--robot-triage\",\"examples\":[\"fmd --robot-triage\"]}}],\"outputs\":[\"html\",\"pdf\",\"both\",\"epub\",\"svg\"],\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"exit_codes\":{{\"0\":\"success\",\"64\":\"usage error\",\"66\":\"input error\",\"70\":\"render unavailable or failed\",\"73\":\"output file error\",\"74\":\"stdout/write error\"}},\"features\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"fit_to_pages\":\"available_binary_search_solver\",\"interactive_html\":\"available_self_hosting_single_file\",\"gfm_plus\":\"available\",\"definition_lists\":\"available\",\"raw_text\":\"available\",\"stdin\":\"available\",\"html_stdout_dash\":\"available\",\"pdf_stdout_dash\":\"refused_usage_error\",\"pdf_default_output_path\":\"available_derived_from_input_stem\",\"custom_css\":\"available\",\"native_config\":\"available\",\"no_config\":\"available\",\"input_size_limit\":\"available\",\"html_image_assets\":\"available_local_png_svg_data_uri\",\"pdf_image_assets\":\"available_png_svg_v0\",\"font_sans_serif_toggle\":\"available\",\"html_font_format\":\"available_ttf_woff1_default_woff1\",\"host_font_assets\":\"available\",\"variable_font_weight\":\"available\",\"pdf_a_2b\":\"available\",\"shared_theme_model\":\"structured_v1\",\"syntax_highlighting\":\"available\",\"pdf_code_line_numbers\":\"available\",\"pdf_metadata\":\"available\",\"source_date_epoch_pdf\":\"available\",\"tagged_pdf\":\"available_hierarchical_accessible\",\"font_subsetting_pdf\":\"available\",\"embedded_subset_fonts_pdf\":\"available\",\"gpos_kerning_pdf\":\"available_focused\",\"gsub_ligatures_pdf\":\"available_focused\",\"knuth_plass_pdf\":\"available\",\"hyphenation_pdf\":\"available_discretionary_body_paragraphs\",\"pdf_justification\":\"available_body_paragraphs\",\"page_builder_pdf\":\"available_v0_keep_widow\",\"stream_compression_pdf\":\"available\",\"robot_triage\":\"available\",\"microtype_pdf\":\"available_optin_protrusion_expansion\",\"optimal_pagination_pdf\":\"available_optin_plass_dp\",\"epub_output\":\"available_epub3_one_chapter\",\"search_index\":\"available_fmd-search-index-v1\",\"svg_output\":\"available_vector_glyphs_as_paths\",\"watch\":\"available_poll_hash_debounce_loopback_preview\",\"wasm_core\":\"no-default-features available\",\"wasm_browser_package\":\"available_published\",\"commonmark_spec\":\"0.31.2_ratcheted_min_567_of_652_normalized\"}}}}",
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
        "fmd agent guide\n\nCanonical commands:\n  fmd README.md --out README.html\n  fmd README.md --interactive-html --out README.html\n  fmd README.md --to pdf --out README.pdf\n  fmd README.md --font-scale lg --out README.html\n  fmd README.md --to pdf --font-scale 125% --out README.pdf\n  fmd README.md --to pdf --fit-to-pages 1 --out README.pdf\n  fmd diff v1.md v2.md --out diff.html\n  fmd stats README.md --json\n  fmd README.md --toc --out README.html\n  fmd README.md --to pdf --toc --toc-depth 2 --out README.pdf\n  fmd README.md --to pdf --pdf-line-numbers --out README.pdf\n  fmd README.md --to pdf --pdf-image images/chart.png=./chart.png --out README.pdf\n  fmd README.md --to pdf --pdf-font body-regular=./Var.ttf --pdf-font-weight 650 --out README.pdf\n  fmd README.md --to pdf --pdf-a 2b --out README.pdf\n  fmd README.md --to pdf --title 'Quarterly Memo' --author 'FMD' --out README.pdf\n  SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf\n  fmd --max-input-bytes 1048576 README.md --out README.html\n  fmd - --out stdin.html < README.md\n  fmd --text '# Hello' --out hello.html\n  fmd --text '# Hello' --out - > hello.html\n  fmd render README.md --to both --out README.html\n  fmd --allow-html trusted.md --out trusted.html\n  fmd --pdf-line-numbers README.md --to pdf --out README.pdf\n  fmd --max-pdf-image-bytes 1048576 README.md --to pdf --out README.pdf\n  fmd --no-remote-images README.md --to pdf --out README.pdf\n  fmd --max-input-bytes 1048576 README.md --out README.html\n  fmd watch README.md --out README.html --serve\n  fmd watch README.md --out README.html --serve --measure 21\n\nDiscovery:\n  fmd capabilities --json   # commands, examples, feature flags, theme, conformance number\n  fmd doctor --json          # subsystem availability, dependency posture, license\n  fmd doctor fonts --corpus ./docs --json\n                             # glyph coverage vs bundled faces + Noto math fallback.\n                             # stdout JSON: scripts/ranges/uncovered/hints.\n                             # exit 0 covered, 1 gaps, 64 usage, 66 input.\n  fmd diff <F1> <F2> --json  # semantic AST diff and change metrics\n  fmd stats [FILE] --json    # word counts, readability scores, outline, and health checks\n  fmd robot-docs guide       # this file\n  fmd --robot-triage         # one-shot JSON envelope: quick-ref + health + next actions\n\nConfig (native, ~/.config/fmd/config by default; --no-config disables):\n  font=sans|serif\n  dark_mode=auto|disabled\n  custom_css=/path/to/stylesheet (or 'none')\n  page_size=letter\n  margin_top_pt, margin_right_pt, margin_bottom_pt, margin_left_pt = non-negative points\n  emoji_strategy=warning|noto_subset|drawn (forward-compat hook; default = warning, render\n    falls back gracefully when a Noto Sans Symbols subset is not bundled; an undeclared key\n    is the v1 default and resolves to 'warning' until a curated Noto Sans Symbols subset\n    ships; set the key explicitly to declare intent).\n\nRules for agents:\n  stdout is document data for HTML-to-stdout and JSON data for capabilities/doctor/config/robot-triage/stats/diff.\n  `--out -` writes HTML document data to stdout only; PDF and --to both require a real output path.\n  diagnostics and write confirmations go to stderr.\n  use --json on render when you need machine-readable status events on stderr.\n  --max-input-bytes caps file/stdin/--text ingress before parsing; oversized input exits 66 with no document data on stdout.\n  File-input HTML and PDF renders auto-load relative local PNG/SVG/JPEG image destinations from the Markdown file's directory; HTML embeds them as data URIs and PDF draws supported assets directly. PDF renders also fetch remote http(s) image destinations at render time via the system curl/wget (per-image --remote-image-timeout-secs, --max-pdf-image-bytes cap); disable with --no-remote-images — failures degrade to alt text with a warning. Use --pdf-image to provide or override a PDF Markdown image destination as DEST=PATH; repeat it for multiple images. The core never fetches network images or reads files itself.\n  PDF output is available as a compact deterministic v0 with embedded per-document font subsets, real metrics, focused GPOS kerning, GSUB ligatures, Knuth-Plass paragraph layout, deterministic discretionary hyphenation and glue justification for body paragraphs, basic keep/widow page building, syntax-highlighted wrapped code blocks, optional --pdf-line-numbers, table of contents generation with dot leaders and bookmark alignment (--toc / [[_TOC_]]), local PNG/SVG/JPEG image assets via auto file-input loading, remote http(s) image fetching (opt-out --no-remote-images), or --pdf-image, PDF metadata via --title/--author/SOURCE_DATE_EPOCH, a hierarchical accessible tagged-PDF structure tree (Document root, per-cell tables with header column scope, nested lists, blockquotes, figures with alt/bbox, links referenced via /OBJR, decoration as /Artifact outside the logical tree), a Noto Sans Math symbol-fallback face for math/arrow glyphs, and an ASCII/SVG/JPEG asset path. deeper page-builder polish is still planned; specifics: full widow/orphan control, keep-with-next, footnotes layout, columns.\n  Use --css <file> for a full custom stylesheet replacement, --font serif for one render, config set font serif for a persistent native default, and --no-config for reproducible config-free runs.\n  Use --font-scale <xs|sm|md|lg|xl|2xl|FLOAT|PERCENT> (alias --type-size) for uniform, anti-aliased typographic scaling across HTML and PDF.\n  Use --fit-to-pages <N> (alias --target-pages) to automatically solve micro-typography and fit content to a page budget.\n  Use --interactive-html (alias --self-hosting) to render a self-hosting single-file HTML workspace with live editor, preview, and client-side PDF export.\n  Host TrueType faces: --pdf-font SLOT=PATH (repeatable; slots body-regular/body-bold/body-italic/body-bold-italic/mono-regular) and --pdf-font-weight WEIGHT or SLOT=WEIGHT (1..=1000). Variable wght faces instance at pin; static faces ignore it with warning font_weight_ignored_static. When body-bold is omitted and body-regular is variable, bold instances from that same file at 700. Flags apply to HTML and PDF.\n\nWarnings are non-fatal. Each surface to stderr (PDF) or a JSON envelope (--json).\n  missing_glyphs: {count, sample} — character(s) had no glyph in the bundled faces.\n  unresolved_image: image dest had no --pdf-image mapping; rendered as alt text.\n  unsupported_image: supplied asset could not be decoded; rendered as alt text.\n  pdf_size_budget: emitted PDF would have exceeded --max-pdf-image-bytes; aborted.\n  font_weight_ignored_static: a static face received --pdf-font-weight; ignored.\n\nVerify (yo83): 0 clean; 1 findings; 2 bad input; 66 usage error; 70 font load failure.\n  Default TTY output is a human caret report; pipes/--json force the JSON schema.\n\nWASM size budget (scripts/check-wasm-package.sh; bg.wasm after wasm-bindgen --target web):\n  tree     raw measured   raw budget   gzip measured  gzip budget  why\n  0.3.2    3,351,808      3,400,000    1,510,214      1,600,000    expanded vector-SVG/PDF\n  0.3.4    3,447,897      3,500,000    1,557,945      1,600,000    Noto math face + JPEG DCTDecode\n  0.3.5    4,019,715      4,200,000    1,798,217      1,850,000    fmd-math+hyphen langs+CJK+gvar+type knobs+page numbers (~+16 KiB Noto regen). Gate prints signed delta vs last ratchet.\n  0.4.1    4,162,426      4,300,000    1,854,075      1,900,000    table of contents + math + CJK fallbacks\n\nExit codes: 0 ok; 64 usage; 66 input; 70 render failed (font load, etc.); 73 write error; 74 stdout write error.",
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
        "verify",
        "watch",
        "config",
        "stats",
        "diff",
        "book",
        // Recognized even without the `batch` feature so it is never rewritten to
        // `render batch ...`; clap then reports a clean "unrecognized subcommand".
        "batch",
        "mcp",
        "help",
    ];
    let global_no_value = ["--json", "--no-color", "--no-config", "--robot-triage"];
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
#[cfg_attr(coverage_nightly, coverage(off))]
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

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod font_pin_warning_tests {
    use super::font_pin_failed_to_instance;
    use crate::{FontAssetSlot, FontAssets};

    #[test]
    fn pin_without_host_bytes_is_not_a_static_ignore() {
        let mut assets = FontAssets::default();
        assets
            .set_slot_weight(FontAssetSlot::BodyRegular, 650)
            .unwrap();
        assert!(
            !font_pin_failed_to_instance(&assets, FontAssetSlot::BodyRegular, 650),
            "no host face: the pin is unused, not ignored on a static face"
        );
    }

    #[test]
    fn static_host_face_is_a_failed_instance() {
        let bytes =
            crate::fonts::body_bytes(crate::FontFamily::Sans, crate::fonts::FontStyle::Regular)
                .to_vec();
        let assets = FontAssets::default()
            .with_slot(FontAssetSlot::BodyRegular, bytes)
            .unwrap()
            .with_slot_weight(FontAssetSlot::BodyRegular, 650)
            .unwrap();
        assert!(
            font_pin_failed_to_instance(&assets, FontAssetSlot::BodyRegular, 650),
            "bundled Plex is static; a 650 pin must not instance"
        );
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod helper_tests {
    use super::*;

    #[test]
    fn wget_https_urls_enable_https_only_to_block_file_redirects() {
        let args = wget_remote_image_args(
            "https://cdn.example/x.png",
            "--timeout=20",
            "--user-agent=fmd/test",
        );
        assert!(
            args.contains(&"--https-only"),
            "https fetches must refuse file:// redirects: {args:?}"
        );
        assert_eq!(args.last().copied(), Some("https://cdn.example/x.png"));
    }

    #[test]
    fn wget_http_urls_do_not_pass_https_only() {
        let args = wget_remote_image_args(
            "http://cdn.example/x.png",
            "--timeout=20",
            "--user-agent=fmd/test",
        );
        assert!(
            !args.contains(&"--https-only"),
            "http images must still fetch: {args:?}"
        );
    }

    /// Create a fresh, unique temp directory for one test. Process id plus a
    /// monotonic counter keeps concurrent tests from sharing a directory.
    fn fresh_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fmd-cli-helper-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fail_prints_to_stderr_and_maps_the_exit_code() {
        assert_eq!(fail(74, "writing help to stdout"), ExitCode::from(74));
    }

    #[test]
    fn fail_render_maps_every_render_error_to_exit_70() {
        let human = fail_render(RenderError::InvalidInput("bad input".to_string()), false);
        assert_eq!(human, ExitCode::from(70));
        let json = fail_render(RenderError::InvalidInput("bad \"input\"".to_string()), true);
        assert_eq!(json, ExitCode::from(70));
    }

    #[test]
    fn json_escape_replaces_bare_control_chars_with_a_space() {
        // \n \r \t get their own escapes; other control chars become a space so
        // the envelope stays valid single-line JSON.
        assert_eq!(json_escape("a\u{1}b"), "a b");
        assert_eq!(json_escape("plain"), "plain");
        assert_eq!(json_escape("q\"w\\e\nr\rt\ty"), "q\\\"w\\\\e\\nr\\rt\\ty");
    }

    #[test]
    fn auto_pdf_image_path_accepts_only_safe_relative_supported_destinations() {
        let base = Path::new("/base");
        // Accepted: relative, PNG/SVG extension, no escape components.
        assert_eq!(
            auto_pdf_image_path("img.png", base),
            Some(PathBuf::from("/base/img.png"))
        );
        assert_eq!(
            auto_pdf_image_path("./img.png", base),
            Some(base.join("./img.png"))
        );
        assert_eq!(
            auto_pdf_image_path("a/b.SVG", base),
            Some(PathBuf::from("/base/a/b.SVG"))
        );
        assert_eq!(
            auto_pdf_image_path("photo.jpg", base),
            Some(PathBuf::from("/base/photo.jpg"))
        );
        assert_eq!(
            auto_pdf_image_path("photo.JPEG", base),
            Some(PathBuf::from("/base/photo.JPEG"))
        );
        // Query and fragment suffixes are stripped before resolution.
        assert_eq!(
            auto_pdf_image_path("img.png?x=1#f", base),
            Some(PathBuf::from("/base/img.png"))
        );
        // Rejected shapes.
        for dest in [
            "",                   // empty
            "   ",                // blank
            "//cdn/x.png",        // protocol-relative
            "a\\b.png",           // backslash path
            "https://e/x.png",    // URI scheme
            "mailto:someone.png", // URI scheme, no slash
            "/abs/x.png",         // absolute
            "x.gif",              // unsupported extension
            "noext",              // no extension
            "../up.png",          // parent-dir escape
            "a/../up.png",        // embedded parent-dir escape
            ".",                  // no file component
        ] {
            assert_eq!(auto_pdf_image_path(dest, base), None, "dest: {dest:?}");
        }
    }

    #[test]
    fn remote_image_url_accepts_only_http_and_https_destinations() {
        assert_eq!(
            remote_image_url("https://cdn.example/x.png"),
            Some("https://cdn.example/x.png")
        );
        assert_eq!(
            remote_image_url("  HTTP://cdn.example/x  "),
            Some("HTTP://cdn.example/x")
        );
        for dest in [
            "",
            "https://",           // scheme only, no host
            "http://",            // scheme only, no host
            "ftp://cdn/x.png",    // non-HTTP scheme
            "file:///etc/passwd", // local scheme must never be fetched
            "//cdn/x.png",        // protocol-relative
            "images/local.png",   // relative local path
            "httpsx://e/x.png",   // near-miss scheme
        ] {
            assert_eq!(remote_image_url(dest), None, "dest: {dest:?}");
        }
    }

    #[test]
    fn has_uri_scheme_matches_scheme_shapes_only() {
        assert!(has_uri_scheme("https://e/x.png"));
        assert!(has_uri_scheme("mailto:x"));
        assert!(has_uri_scheme("a+b-c.d:rest"));
        assert!(!has_uri_scheme("1abc:/x")); // scheme must start alphabetic
        assert!(!has_uri_scheme(":x")); // empty scheme
        assert!(!has_uri_scheme("no-colon/path.png"));
        assert!(!has_uri_scheme("dir/with:colon.png")); // colon after separator
    }

    #[test]
    fn split_pdf_image_spec_prefers_hint_then_existing_path_then_first_split() {
        // A document-destination hint always wins, wherever its `=` sits.
        assert_eq!(split_pdf_image_spec("a=b=c", &["a=b"]), Some(("a=b", "c")));
        assert_eq!(split_pdf_image_spec("a=b=c", &["a"]), Some(("a", "b=c")));
        // Without hints, a split whose PATH names an existing file wins.
        let dir = fresh_dir("spec-existing");
        let file = dir.join("real.png");
        std::fs::write(&file, b"png").unwrap();
        let spec = format!("x=y={}", file.display());
        let (dest, path) = split_pdf_image_spec(&spec, &[]).unwrap();
        assert_eq!(dest, "x=y");
        assert_eq!(path, file.display().to_string());
        // Otherwise the first non-blank split wins.
        assert_eq!(
            split_pdf_image_spec("x=y=nothing-here", &[]),
            Some(("x", "y=nothing-here"))
        );
        // A blank destination is surfaced (and rejected) by the parser.
        let err = parse_pdf_image_spec("=path.png", &[]).unwrap_err();
        assert!(err.contains("MARKDOWN_DEST must not be blank"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_image_destinations_walks_nested_containers_in_document_order() {
        let doc = parse_markdown(
            "# H ![h](h.png)\n\n![p](p.png)\n\n> ![q](q.png)\n\n- ![l](l.png)\n\n\
             | ![t1](t1.png) |\n| --- |\n| ![t2](t2.png) |\n\n\
             *![e](e.png)* **![s](s.png)** ~~![k](k.png)~~ [![n](n.png)](https://x/)\n",
        );
        let mut dests = Vec::new();
        collect_image_destinations(&doc.blocks, &mut dests);
        assert_eq!(
            dests,
            vec![
                "h.png", "p.png", "q.png", "l.png", "t1.png", "t2.png", "e.png", "s.png", "k.png",
                "n.png"
            ]
        );
    }

    #[test]
    fn append_auto_image_assets_loads_safe_files_once_and_skips_the_rest() {
        let dir = fresh_dir("auto-assets");
        std::fs::write(dir.join("pic.png"), b"not-a-real-png-but-loaded").unwrap();
        std::fs::create_dir_all(dir.join("imgdir.png")).unwrap();
        let doc = parse_markdown(
            "![a](pic.png)\n\n![dup](pic.png)\n\n![u](https://e/x.png)\n\n\
             ![m](missing.png)\n\n![d](imgdir.png)\n\n![e]()\n",
        );

        let mut assets = Vec::new();
        append_auto_image_assets(&doc, &dir, &mut assets, 1024, "PDF").unwrap();
        assert_eq!(assets.len(), 1, "only pic.png resolves to a loadable file");
        assert_eq!(assets[0].destination, "pic.png");
        assert_eq!(assets[0].bytes, b"not-a-real-png-but-loaded");

        // A base directory that cannot be canonicalized disables auto-loading
        // without failing the render.
        let mut none = Vec::new();
        append_auto_image_assets(
            &doc,
            Path::new("/no/such/fmd/base/dir"),
            &mut none,
            1024,
            "PDF",
        )
        .unwrap();
        assert!(none.is_empty());

        // An over-limit file is an error naming the flag, not a silent skip.
        let mut over = Vec::new();
        let err = append_auto_image_assets(&doc, &dir, &mut over, 1, "PDF").unwrap_err();
        assert!(err.contains("exceeds --max-pdf-image-bytes 1"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Exit-code contract tests for `run_batch`. These live in-file (not in the
/// integration suite) because the batch feature's coverage pass runs `--lib`
/// only: the `fmd` binary spawned by integration tests is built without
/// `--features batch` there, so binary-spawn tests can never instrument this
/// code. Everything below is hermetic: `no_config` is always true (the
/// developer's real config file is never read) and all I/O stays inside a
/// unique per-test temp directory.
#[cfg(all(test, feature = "batch"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod run_batch_exit_code_tests {
    use super::*;

    /// Create a fresh, unique temp directory for one test.
    fn fresh_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "fmd-cli-run-batch-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Baseline batch args: HTML output, automatic sizing, human diagnostics.
    fn args(inputs: Vec<PathBuf>) -> BatchArgs {
        BatchArgs {
            inputs,
            to: Target::Html,
            out_dir: None,
            workers: None,
            batch_mode: BatchModeArg::Interactive,
            mem_budget: None,
            timeout: None,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_pdf_image_bytes: DEFAULT_MAX_PDF_IMAGE_BYTES,
            continue_on_error: false,
            font: None,
            css: None,
            json: false,
        }
    }

    #[test]
    fn workers_zero_is_a_usage_error() {
        let dir = fresh_dir("workers0");
        std::fs::write(dir.join("a.md"), "# A\n").unwrap();
        let mut a = args(vec![dir.join("a.md")]);
        a.workers = Some(0);
        assert_eq!(run_batch(a, false, true), ExitCode::from(64));
        assert!(!dir.join("a.html").exists(), "nothing may render");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn out_dir_dash_is_a_usage_error() {
        let dir = fresh_dir("outdir-dash");
        std::fs::write(dir.join("a.md"), "# A\n").unwrap();
        let mut a = args(vec![dir.join("a.md")]);
        a.out_dir = Some(PathBuf::from("-"));
        assert_eq!(run_batch(a, true, true), ExitCode::from(64));
        assert!(!dir.join("a.html").exists(), "nothing may render");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_stylesheet_is_an_input_error() {
        let dir = fresh_dir("missing-css");
        std::fs::write(dir.join("a.md"), "# A\n").unwrap();
        let mut a = args(vec![dir.join("a.md")]);
        a.css = Some(dir.join("missing.css"));
        assert_eq!(run_batch(a, false, true), ExitCode::from(66));
        assert!(!dir.join("a.html").exists(), "nothing may render");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_mode_aborts_on_an_unexpandable_input() {
        let dir = fresh_dir("strict-expand");
        let a = args(vec![dir.join("nope.md")]);
        assert_eq!(run_batch(a, false, true), ExitCode::from(66));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expanding_to_no_inputs_is_an_input_error() {
        // A directory with no Markdown files expands to nothing.
        let dir = fresh_dir("empty-expand");
        let a = args(vec![dir.clone()]);
        assert_eq!(run_batch(a, false, true), ExitCode::from(66));
        // With --continue-on-error and only unexpandable paths there is still
        // nothing to render (the "no readable Markdown inputs" message branch).
        let mut b = args(vec![dir.join("nope.md")]);
        b.continue_on_error = true;
        assert_eq!(run_batch(b, false, true), ExitCode::from(66));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_batch_renders_every_input_with_custom_css_and_font() {
        let dir = fresh_dir("html-ok");
        std::fs::write(dir.join("a.md"), "# Alpha\n\nBody.\n").unwrap();
        std::fs::write(dir.join("b.md"), "# Beta\n\nBody.\n").unwrap();
        let css = dir.join("custom.css");
        std::fs::write(&css, "body{background:#123456}").unwrap();
        let out_dir = dir.join("out");

        let mut a = args(vec![dir.join("a.md"), dir.join("b.md")]);
        a.out_dir = Some(out_dir.clone());
        a.workers = Some(2);
        a.batch_mode = BatchModeArg::Throughput;
        a.mem_budget = Some(1 << 30);
        a.timeout = Some(600);
        a.font = Some(FontArg::Serif);
        a.css = Some(css);

        assert_eq!(run_batch(a, false, true), ExitCode::SUCCESS);
        let a_html = std::fs::read_to_string(out_dir.join("a.html")).unwrap();
        assert!(a_html.contains("background:#123456"), "custom CSS applied");
        assert!(a_html.contains("Alpha"));
        assert!(out_dir.join("b.html").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pdf_batch_writes_outputs_alongside_inputs() {
        let dir = fresh_dir("pdf-ok");
        std::fs::write(dir.join("c.md"), "# C\n\nBody.\n").unwrap();
        let mut a = args(vec![dir.join("c.md")]);
        a.to = Target::Pdf;
        assert_eq!(run_batch(a, false, true), ExitCode::SUCCESS);
        let pdf = std::fs::read(dir.join("c.pdf")).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_batch_with_json_emits_the_receipt_and_succeeds() {
        let dir = fresh_dir("both-json");
        std::fs::write(dir.join("d.md"), "# D\n\nBody.\n").unwrap();
        let mut a = args(vec![dir.join("d.md")]);
        a.to = Target::Both;
        a.json = true; // receipt JSON goes to real stdout (data, not diagnostics)
        assert_eq!(run_batch(a, false, true), ExitCode::SUCCESS);
        assert!(dir.join("d.html").exists());
        let pdf = std::fs::read(dir.join("d.pdf")).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn continue_on_error_records_expansion_failures_and_still_succeeds() {
        let dir = fresh_dir("continue-expand");
        std::fs::write(dir.join("good.md"), "# Good\n\nBody.\n").unwrap();
        let mut a = args(vec![dir.join("good.md"), dir.join("nope.md")]);
        a.continue_on_error = true;
        assert_eq!(run_batch(a, false, true), ExitCode::SUCCESS);
        assert!(dir.join("good.html").exists(), "the valid input renders");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_oversized_input_exits_with_the_input_error_code() {
        let dir = fresh_dir("oversized");
        std::fs::write(dir.join("a.md"), "# A body larger than the tiny cap\n").unwrap();
        let mut a = args(vec![dir.join("a.md")]);
        a.max_input_bytes = 8;
        assert_eq!(run_batch(a, false, true), ExitCode::from(66));
        assert!(!dir.join("a.html").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unwritable_out_dir_exits_with_the_output_error_code() {
        let dir = fresh_dir("unwritable-out");
        std::fs::write(dir.join("a.md"), "# A\n\nBody.\n").unwrap();
        // A regular file where a directory component is needed makes every
        // output write fail with an output-kind error.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let mut a = args(vec![dir.join("a.md")]);
        a.out_dir = Some(blocker.join("sub"));
        assert_eq!(run_batch(a, false, true), ExitCode::from(73));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
