//! Native book command and shared executable dispatcher.
//!
//! Both binary shims enter here. Book commands use a focused parser and the
//! bounded book pipeline; all other invocations delegate to the existing CLI
//! unchanged. Filesystem and process concerns remain behind the `cli` feature.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use super::render::site_search;
use crate::config::FmdConfig;
use crate::file_write::{OutputFile, write_outputs_staged};
use crate::{Block, Document, FontFamily, HtmlOptions, Inline, PdfOptions};

#[path = "../cli/book_manifest.rs"]
mod manifest;
#[path = "../cli/book_inputs.rs"]
mod inputs;
#[path = "native_check.rs"]
mod preflight;
#[path = "native/options.rs"]
mod options;

const MAX_STYLESHEET_BYTES: u64 = 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Parser)]
#[command(name = "fmd", version, about = "Render a Markdown book to HTML, PDF, or EPUB")]
struct BookCli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    no_color: bool,
    #[arg(long, global = true)]
    no_config: bool,
    #[arg(long, global = true)]
    robot_triage: bool,
    #[command(subcommand)]
    command: BookCommand,
}

#[derive(Subcommand)]
enum BookCommand {
    /// Assemble Markdown chapters into an HTML site, PDF book, or EPUB e-book.
    Book(BookArgs),
}

#[derive(Args)]
struct BookArgs {
    /// Input directory. book.toml sets metadata, chapter order and include_only sources.
    #[arg(value_name = "DIR")]
    input: PathBuf,
    /// Output directory; PDF/EPUB also accept a path ending in .pdf/.epub.
    #[arg(long, short)]
    out_dir: Option<PathBuf>,
    /// Explicit PDF/EPUB output file. Binary stdout is not supported.
    #[arg(long, conflicts_with = "out_dir")]
    out: Option<PathBuf>,
    /// HTML site, PDF, both, or one multi-chapter EPUB with packaged images.
    #[arg(long, value_enum, default_value_t = BookTarget::Both)]
    to: BookTarget,
    /// Check expanded HTML navigation only; write no publications. Exit 65 for findings.
    #[arg(long, conflicts_with_all = ["out", "out_dir", "to", "css", "font", "title", "author", "lang", "max_pdf_image_bytes", "deny_broken_links", "robot_triage", "pdf_fonts", "pdf_font_weights", "font_scale", "page_size", "margin_top_pt", "margin_right_pt", "margin_bottom_pt", "margin_left_pt", "pdf_line_numbers", "toc_depth"])]
    check_links: bool,
    /// Refuse publication on expanded-navigation findings before rendering or writes.
    #[arg(long, conflicts_with = "robot_triage")]
    deny_broken_links: bool,
    /// Override the book title from book.toml or the first chapter.
    #[arg(long)]
    title: Option<String>,
    /// PDF author metadata, overriding book.toml/frontmatter.
    #[arg(long)]
    author: Option<String>,
    /// Default language; chapter frontmatter may override HTML/EPUB language.
    #[arg(long)]
    lang: Option<String>,
    /// Complete replacement stylesheet for HTML/EPUB.
    #[arg(long)]
    css: Option<PathBuf>,
    /// Override the configured body font for HTML/PDF.
    #[arg(long, value_parser = ["sans", "serif"])]
    font: Option<String>,
    /// Host TrueType font as SLOT=PATH, applied to HTML, PDF and EPUB. Repeatable.
    /// Slots: body-regular, body-bold, body-italic, body-bold-italic, mono-regular.
    #[arg(long = "pdf-font", value_name = "SLOT=PATH")]
    pdf_fonts: Vec<String>,
    /// Host font weight: WEIGHT for body-regular, or SLOT=WEIGHT (1..=1000).
    /// Variable fonts are instanced; static fonts report an ignored-weight warning.
    #[arg(long = "pdf-font-weight", value_name = "WEIGHT|SLOT=WEIGHT")]
    pdf_font_weights: Vec<String>,
    /// Uniform typography scale for HTML/PDF/EPUB: lg, 125%, 1.2, 18px or 12pt.
    #[arg(long, visible_alias = "type-size", value_name = "SCALE|PRESET")]
    font_scale: Option<String>,
    /// PDF paper: letter, a4, a5, legal, tabloid, or WIDTHxHEIGHT in points.
    #[arg(long, value_name = "SIZE")]
    page_size: Option<String>,
    /// PDF top margin in points; overrides the configured value.
    #[arg(long)]
    margin_top_pt: Option<f64>,
    /// PDF right margin in points; overrides the configured value.
    #[arg(long)]
    margin_right_pt: Option<f64>,
    /// PDF bottom margin in points; overrides the configured value.
    #[arg(long)]
    margin_bottom_pt: Option<f64>,
    /// PDF left margin in points; overrides the configured value.
    #[arg(long)]
    margin_left_pt: Option<f64>,
    /// Render line numbers in the PDF book's fenced code blocks.
    #[arg(long)]
    pdf_line_numbers: bool,
    /// Maximum heading depth in the generated PDF contents (1..=6).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=6))]
    toc_depth: Option<u8>,
    /// Maximum bytes per Markdown file and expanded chapter (book cap: 64 MiB).
    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    max_input_bytes: u64,
    /// Maximum bytes per automatically loaded local image (ceiling: 32 MiB).
    #[arg(long, default_value_t = 32 * 1024 * 1024)]
    max_pdf_image_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BookTarget {
    Html,
    Pdf,
    Both,
    Epub,
}

impl BookTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
            Self::Both => "both",
            Self::Epub => "epub",
        }
    }
}

/// Shared executable entrypoint. Non-book arguments preserve the original
/// command's implicit-render behavior, help, feature gates and exit codes.
#[must_use]
pub fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    if !handles_book(&args) {
        return crate::cli::main();
    }
    let json_requested = args.iter().skip(1).take_while(|arg| *arg != "--")
        .any(|arg| arg == "--json");
    let cli = match BookCli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(error.kind(), clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion) {
                return if error.print().is_ok() { ExitCode::SUCCESS } else { ExitCode::from(74) };
            }
            return failure(64, "usage_error", &error.to_string(), json_requested);
        }
    };
    if cli.robot_triage {
        return crate::cli::main();
    }
    let _ = cli.no_color;
    let BookCommand::Book(args) = cli.command;
    if args.check_links {
        return preflight::run(&args, cli.json);
    }
    match run(args, cli.no_config) {
        Ok(receipt) => {
            if cli.json {
                if stdout_line(&receipt.json).is_err() { return ExitCode::from(74); }
            } else {
                for warning in receipt.warnings { eprintln!("fmd: warning: {warning}"); }
                eprintln!("fmd: assembled book ({} chapters)", receipt.chapters);
                for path in receipt.outputs { eprintln!("fmd: wrote {}", path.display()); }
            }
            ExitCode::SUCCESS
        }
        Err(error) => failure(error.code, error.tag, &error.message, cli.json),
    }
}

fn handles_book(args: &[OsString]) -> bool {
    for arg in args.iter().skip(1) {
        match arg.to_str() {
            Some("--json" | "--no-color" | "--no-config") => {}
            Some("book") => return true,
            _ => return false,
        }
    }
    false
}

struct Failure {
    code: u8,
    tag: &'static str,
    message: String,
}

fn error(code: u8, tag: &'static str, message: impl ToString) -> Failure {
    Failure { code, tag, message: message.to_string() }
}

struct Receipt {
    json: String,
    chapters: usize,
    outputs: Vec<PathBuf>,
    warnings: Vec<String>,
}

fn run(args: BookArgs, no_config: bool) -> Result<Receipt, Failure> {
    if args.out.as_ref().is_some_and(|path| path.as_os_str() == "-")
        || args.out_dir.as_ref().is_some_and(|path| path.as_os_str() == "-")
    {
        return Err(error(64, "usage_error", "book output must be a real path; binary stdout is not supported"));
    }
    if args.out.is_some() && matches!(args.to, BookTarget::Html | BookTarget::Both) {
        return Err(error(64, "usage_error", "--out is for PDF/EPUB; use --out-dir for HTML or both"));
    }
    let config = if no_config { FmdConfig::default() } else {
        FmdConfig::load_default().map_err(|e| error(66, "config_error", e))?
    };
    let mut theme = config.to_theme();
    if let Some(font) = args.font.as_deref() {
        theme = theme.with_font(FontFamily::parse(font)
            .ok_or_else(|| error(64, "usage_error", "font must be sans or serif"))?);
    }
    let typography = options::prepare(&args, theme)?;
    let (mut loaded, link_check) = if args.deny_broken_links {
        let mut loaded = inputs::load_sources(&args.input, args.max_input_bytes)
            .map_err(|e| error(66, "book_error", e))?;
        let report = preflight::require_clean(&loaded.book)?;
        inputs::load_images(&mut loaded, args.max_pdf_image_bytes)
            .map_err(|e| error(66, "book_error", e))?;
        (loaded, format!(",\"link_check\":{report}"))
    } else {
        (inputs::load(&args.input, args.max_input_bytes, args.max_pdf_image_bytes)
            .map_err(|e| error(66, "book_error", e))?, String::new())
    };
    let first = loaded.book.chapters.first().ok_or_else(|| error(66, "book_error", "empty book"))?;
    let frontmatter = first.frontmatter.as_ref();
    let title = args.title.clone().or_else(|| loaded.manifest.title.clone())
        .or_else(|| frontmatter.and_then(|fm| fm.title.clone())).or_else(|| Some(first.title.clone()));
    let lang = args.lang.clone().or_else(|| loaded.manifest.lang.clone())
        .or_else(|| frontmatter.and_then(|fm| fm.lang.clone()));
    let author = args.author.clone().or_else(|| loaded.manifest.author.clone())
        .or_else(|| frontmatter.and_then(|fm| fm.author.clone()));
    let css = args.css.as_ref().or(config.custom_css.as_ref()).map(|path| {
        let bytes = read_stylesheet(path).map_err(|e| error(66, "stylesheet_error", e))?;
        if let Ok(canonical) = path.canonicalize() { loaded.protected_paths.insert(canonical); }
        String::from_utf8(bytes).map_err(|_| error(66, "stylesheet_error", "stylesheet must be UTF-8"))
    }).transpose()?;
    let image_count = loaded.images.len();
    let font_assets = typography.load_fonts(&mut loaded.protected_paths)?;
    if !matches!(args.to, BookTarget::Pdf | BookTarget::Both) {
        loaded.warnings.extend(options::font_warnings(&font_assets));
    }
    let mut html_options = HtmlOptions {
        theme: typography.theme,
        title,
        lang,
        custom_css: css,
        image_assets: std::mem::take(&mut loaded.images),
        font_assets,
        ..HtmlOptions::default()
    };
    let paths = output_paths(&args)?;
    let known: BTreeSet<String> = loaded.book.chapters.iter().map(|chapter| chapter.out_name.clone()).collect();
    let known_sources: BTreeSet<&str> = loaded.book.chapters.iter().map(|chapter| chapter.path.as_str()).collect();
    let unresolved = loaded.book.chapters.iter().map(|chapter| {
        unresolved_links(&chapter.doc, &chapter.path, &known_sources)
    }).sum::<usize>();
    if unresolved > 0 {
        loaded.warnings.push(format!("unresolved_links: {unresolved}"));
    }
    let mut rendered = Vec::new();
    let mut output_bytes = 0usize;
    let mut pages = 0u64;
    if matches!(args.to, BookTarget::Html | BookTarget::Both) {
        let book_lang = html_options.lang.clone();
        let book_title = html_options.title.clone();
        for chapter in &loaded.book.chapters {
            let mut doc = chapter.doc.clone();
            super::rewrite_links_for_site(&mut doc, &known);
            html_options.title = Some(chapter.title.clone());
            html_options.lang = chapter.frontmatter.as_ref().and_then(|fm| fm.lang.clone())
                .or_else(|| book_lang.clone());
            let html = crate::render_html_document(&doc, &html_options)
                .map_err(|e| error(70, "html_render_error", e))?;
            let html = super::inject_book_nav(&html, &loaded.book, &chapter.out_name);
            let html = site_search::inject_link(&html);
            add_output(&mut rendered, &mut output_bytes, paths.site.join(&chapter.out_name), html.into_bytes())?;
        }
        html_options.lang = book_lang;
        // Restore the book title before a PDF in a combined export.
        html_options.title = book_title;
        let search = site_search::index_json(&loaded.book)
            .map_err(|e| error(70, "book_search_error", e))?;
        let search_page = site_search::page(
            &search, html_options.title.as_deref().unwrap_or("Book"), html_options.lang.as_deref(),
        ).map_err(|e| error(70, "book_search_error", e))?;
        add_output(&mut rendered, &mut output_bytes, paths.site.join("search-index.json"), search.into_bytes())?;
        add_output(&mut rendered, &mut output_bytes, paths.site.join(site_search::PAGE_NAME), search_page.into_bytes())?;
        let first = &loaded.book.chapters[0];
        let name = super::escape_attr_pub(&first.out_name);
        let title = super::escape_text_pub(&first.title);
        let landing = format!("<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0; url={name}\"><title>{title}</title></head><body><p>Open <a href=\"{name}\">{title}</a>.</p></body></html>\n");
        add_output(&mut rendered, &mut output_bytes, paths.site.join("index.html"), landing.into_bytes())?;
    }
    if matches!(args.to, BookTarget::Pdf | BookTarget::Both) {
        let pdf_options = PdfOptions {
            theme: html_options.theme.clone(),
            title: html_options.title.clone(),
            author,
            lang: html_options.lang.clone(),
            toc: true,
            toc_depth: args.toc_depth,
            page_numbers: true,
            code_line_numbers: args.pdf_line_numbers,
            base_font_size: typography.font_scale.map(|scale| scale.pdf_base_pt()),
            metadata_epoch_seconds: metadata_epoch()?,
            image_assets: std::mem::take(&mut html_options.image_assets),
            font_assets: std::mem::take(&mut html_options.font_assets),
            ..PdfOptions::default()
        };
        let (bytes, emitted_pages, warnings) = super::render::render_book_pdf_counted(&loaded.book, &pdf_options)
            .map_err(|e| error(70, "pdf_render_error", e))?;
        loaded.warnings.extend(warnings.iter().map(options::warning_text));
        pages = emitted_pages;
        add_output(&mut rendered, &mut output_bytes, paths.pdf, bytes)?;
    }
    if args.to == BookTarget::Epub {
        let bytes = crate::epub::render_book_epub(&loaded.book, &html_options)
            .map_err(|e| error(70, "epub_render_error", e))?;
        add_output(&mut rendered, &mut output_bytes, paths.epub, bytes)?;
    }
    // No destination is touched until discovery, parsing and every render
    // have succeeded. The shared writer stages all files before replacement.
    for (path, _) in &rendered {
        if path.canonicalize().ok().is_some_and(|p| loaded.protected_paths.contains(&p)) {
            return Err(error(73, "output_error", format!("output would overwrite a book input: {}", path.display())));
        }
        if path.is_dir() {
            return Err(error(73, "output_error", format!("output is a directory: {}", path.display())));
        }
    }
    for (path, _) in &rendered {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .map_err(|e| error(73, "output_error", format!("creating {}: {e}", parent.display())))?;
        }
    }
    let writes: Vec<_> = rendered.iter().map(|(path, bytes)| OutputFile { path, bytes }).collect();
    write_outputs_staged(&writes)
        .map_err(|e| error(73, "output_error", format!("writing {}: {}", e.path.display(), e.source)))?;
    let outputs: Vec<PathBuf> = rendered.into_iter().map(|(path, _)| path).collect();
    let files: Vec<_> = loaded.book.chapters.iter().enumerate().map(|(index, chapter)| {
        let out_name = if args.to == BookTarget::Epub { format!("chapter-{}.xhtml", index + 1) } else { chapter.out_name.clone() };
        format!("{{\"path\":{},\"out_name\":{},\"title\":{},\"bytes\":{}}}",
            json_string(&chapter.path), json_string(&out_name), json_string(&chapter.title),
            loaded.source_bytes.get(index).copied().unwrap_or(0))
    }).collect();
    let output_json: Vec<_> = outputs.iter().map(|path| json_string(&path.display().to_string())).collect();
    let warnings: Vec<_> = loaded.warnings.iter().map(|warning| json_string(warning)).collect();
    let json = format!("{{\"ok\":true,\"tool\":\"fmd\",\"command\":\"book\",\"target\":{},\"input\":{},\"chapters\":{},\"files\":[{}],\"unresolved_links\":{unresolved},\"pages\":{pages},\"images\":{image_count},\"outputs\":[{}],\"warnings\":[{}]{link_check}}}",
        json_string(args.to.as_str()), json_string(&args.input.display().to_string()), loaded.book.chapters.len(),
        files.join(","), output_json.join(","), warnings.join(","));
    Ok(Receipt { json, chapters: loaded.book.chapters.len(), outputs, warnings: loaded.warnings })
}

struct OutputPaths { site: PathBuf, pdf: PathBuf, epub: PathBuf }

fn output_paths(args: &BookArgs) -> Result<OutputPaths, Failure> {
    let input = args.input.canonicalize().map_err(|e| error(66, "input_error", e))?;
    let name = input.file_name().and_then(|name| name.to_str()).unwrap_or("book");
    let parent = args.input.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let site = args.out_dir.clone().unwrap_or_else(|| parent.join(format!("{name}-site")));
    let single = |extension: &str| {
        if let Some(out) = &args.out { return out.clone(); }
        match &args.out_dir {
            Some(out) if out.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(extension))
                && matches!(args.to, BookTarget::Pdf | BookTarget::Epub) => out.clone(),
            Some(out) => out.join(format!("{name}.{extension}")),
            None => parent.join(format!("{name}.{extension}")),
        }
    };
    Ok(OutputPaths { site, pdf: single("pdf"), epub: single("epub") })
}

fn add_output(outputs: &mut Vec<(PathBuf, Vec<u8>)>, total: &mut usize, path: PathBuf, bytes: Vec<u8>) -> Result<(), Failure> {
    *total = total.checked_add(bytes.len()).ok_or_else(|| error(70, "output_budget", "book output size overflow"))?;
    if *total > MAX_OUTPUT_BYTES {
        return Err(error(70, "output_budget", "rendered book outputs exceed 256 MiB"));
    }
    outputs.push((path, bytes));
    Ok(())
}

fn read_stylesheet(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_STYLESHEET_BYTES {
        return Err("stylesheet must be a regular file of at most 1 MiB".to_string());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).and_then(|file| file.take(MAX_STYLESHEET_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("reading {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_STYLESHEET_BYTES { return Err("stylesheet exceeds 1 MiB".into()); }
    Ok(bytes)
}

fn metadata_epoch() -> Result<Option<u64>, Failure> {
    match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(value) => value.parse().map(Some).map_err(|_| error(64, "usage_error", "SOURCE_DATE_EPOCH must be a nonnegative integer")),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(error(64, "usage_error", "SOURCE_DATE_EPOCH must be UTF-8")),
    }
}

fn unresolved_links(doc: &Document, source: &str, known: &BTreeSet<&str>) -> usize {
    fn inlines(values: &[Inline], source: &str, known: &BTreeSet<&str>) -> usize {
        values.iter().map(|inline| match inline {
            Inline::Link { dest, content, .. } => {
                let missing = super::resolve_book_destination(source, dest).is_some_and(|(target, _)| {
                    let lower = target.to_ascii_lowercase();
                    (lower.ends_with(".md") || lower.ends_with(".markdown")) && !known.contains(target.as_str())
                });
                usize::from(missing) + inlines(content, source, known)
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => inlines(inner, source, known),
            _ => 0,
        }).sum()
    }
    fn blocks(values: &[Block], source: &str, known: &BTreeSet<&str>) -> usize {
        values.iter().map(|block| match block {
            Block::Paragraph(values) | Block::Heading { inlines: values, .. } => inlines(values, source, known),
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => blocks(inner, source, known),
            Block::List(list) => list.items.iter().map(|item| blocks(&item.blocks, source, known)).sum(),
            Block::Table(table) => table.head.iter().chain(table.rows.iter().flatten()).map(|cell| inlines(cell, source, known)).sum(),
            Block::DefinitionList(items) => items.iter().map(|item| item.terms.iter().chain(&item.definitions).map(|values| inlines(values, source, known)).sum::<usize>()).sum(),
            _ => 0,
        }).sum()
    }
    blocks(&doc.blocks, source, known)
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"), '\r' => out.push_str("\\r"), '\t' => out.push_str("\\t"),
            ch if ch < ' ' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn stdout_line(text: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    match writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

fn failure(code: u8, tag: &str, message: &str, json: bool) -> ExitCode {
    if json {
        let text = format!("{{\"ok\":false,\"error\":{{\"code\":{},\"message\":{}}},\"exit_code\":{code}}}", json_string(tag), json_string(message));
        if writeln!(std::io::stderr().lock(), "{text}").is_err() { return ExitCode::from(74); }
    } else {
        eprintln!("fmd: {message}");
    }
    ExitCode::from(code)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn dispatcher_preserves_implicit_render_and_global_commands() {
        for args in [vec!["fmd", "book", "docs"], vec!["fmd", "--json", "--no-config", "book", "docs"]] {
            assert!(handles_book(&args.into_iter().map(OsString::from).collect::<Vec<_>>()));
        }
        for args in [vec!["fmd"], vec!["fmd", "--text", "book"], vec!["fmd", "render", "book"], vec!["fmd", "--", "book"], vec!["fmd", "--help"], vec!["fmd", "capabilities", "--json"]] {
            assert!(!handles_book(&args.into_iter().map(OsString::from).collect::<Vec<_>>()));
        }
    }

    #[test]
    fn book_parser_accepts_epub_and_global_flags_on_either_side() {
        let cli = BookCli::try_parse_from(["fmd", "--no-config", "book", "docs", "--to", "epub", "--out", "manual.epub", "--json"]).unwrap();
        let BookCommand::Book(args) = cli.command;
        assert!(cli.json && cli.no_config);
        assert_eq!(args.to, BookTarget::Epub);
        assert_eq!(args.out, Some(PathBuf::from("manual.epub")));
        assert!(BookCli::try_parse_from(["fmd", "book", "docs", "--out", "x.epub", "--out-dir", "dist"]).is_err());
    }

    #[test]
    fn usage_errors_are_detected_without_loading_any_input() {
        let cli = BookCli::try_parse_from(["fmd", "book", "missing", "--to", "epub", "--out", "-"]).unwrap();
        let BookCommand::Book(args) = cli.command;
        assert_eq!(run(args, true).err().unwrap().code, 64);
    }

    struct TestDirectory(PathBuf);
    impl Drop for TestDirectory {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    fn fixture() -> TestDirectory {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "fmd-native-publish-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let dir = TestDirectory(path);
                    let input = dir.0.join("chapters");
                    std::fs::create_dir(&input).unwrap();
                    std::fs::write(input.join("first.md"), "# First\n\n[Next](second.md#target)\n").unwrap();
                    std::fs::write(input.join("second.md"), "# Target\n\nSearchable second chapter.\n").unwrap();
                    return dir;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("creating test directory: {error}"),
            }
        }
        panic!("could not allocate a test directory");
    }

    #[test]
    fn native_html_publishes_the_offline_search_page_and_machine_index() {
        let dir = fixture();
        let input = dir.0.join("chapters");
        let output = dir.0.join("site");
        let cli = BookCli::try_parse_from([
            "fmd", "book", input.to_str().unwrap(), "--to", "html", "--title", "Test book",
            "--out-dir", output.to_str().unwrap(),
        ]).unwrap();
        let BookCommand::Book(args) = cli.command;
        let receipt = run(args, true).unwrap_or_else(|e| panic!("{}", e.message));
        let search = std::fs::read_to_string(output.join(site_search::PAGE_NAME)).unwrap();
        assert!(search.contains("<title>Search — Test book</title>"));
        assert!(search.contains("id=\"search-data\""));
        let index = std::fs::read_to_string(output.join("search-index.json")).unwrap();
        assert!(index.contains("\"source\":\"second.md\",\"page\":\"second.html\""));
        for name in ["first.html", "second.html"] {
            let page = std::fs::read_to_string(output.join(name)).unwrap();
            assert!(page.contains("href=\"./~fmd-search.html\""));
        }
        assert_eq!(receipt.outputs.len(), 5);
        assert!(receipt.json.contains("~fmd-search.html"));
    }

    #[test]
    fn native_pdf_uses_bound_book_navigation_and_actual_emitted_page_count() {
        let dir = fixture();
        let input = dir.0.join("chapters");
        let output = dir.0.join("book.pdf");
        let loaded = inputs::load(&input, 64 * 1024 * 1024, 32 * 1024 * 1024)
            .unwrap_or_else(|e| panic!("{e}"));
        let options = PdfOptions {
            theme: FmdConfig::default().to_theme(),
            title: Some("Test book".into()),
            toc: true,
            page_numbers: true,
            metadata_epoch_seconds: metadata_epoch().unwrap_or_else(|e| panic!("{}", e.message)),
            image_assets: loaded.images.clone(),
            ..PdfOptions::default()
        };
        let (expected, page_count, _) = super::super::render::render_book_pdf_counted(&loaded.book, &options).unwrap();
        let cli = BookCli::try_parse_from([
            "fmd", "book", input.to_str().unwrap(), "--to", "pdf", "--title", "Test book",
            "--out", output.to_str().unwrap(),
        ]).unwrap();
        let BookCommand::Book(args) = cli.command;
        let receipt = run(args, true).unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(std::fs::read(output).unwrap(), expected);
        assert!(page_count >= 2);
        assert!(receipt.json.contains(&format!("\"pages\":{page_count},")));
        assert_eq!(receipt.outputs.len(), 1);
    }

    #[test]
    fn check_parser_accepts_defaults_but_refuses_explicit_publication_options() {
        let cli = BookCli::try_parse_from([
            "fmd", "--json", "book", "missing", "--check-links", "--max-input-bytes", "128",
        ]).unwrap();
        let BookCommand::Book(args) = cli.command;
        assert!(args.check_links && !args.deny_broken_links);
        assert_eq!(args.max_input_bytes, 128);
        for extra in [
            vec!["--out", "x.epub"], vec!["--out-dir", "site"], vec!["--to", "both"],
            vec!["--css", "missing.css"], vec!["--font", "serif"], vec!["--title", "Title"],
            vec!["--author", "Ada"], vec!["--lang", "fr"], vec!["--max-pdf-image-bytes", "1"],
            vec!["--deny-broken-links"], vec!["--robot-triage"],
            vec!["--pdf-font", "body-regular=missing.ttf"], vec!["--pdf-font-weight", "500"],
            vec!["--font-scale", "lg"], vec!["--page-size", "a4"],
            vec!["--margin-top-pt", "36"], vec!["--margin-right-pt", "36"],
            vec!["--margin-bottom-pt", "36"], vec!["--margin-left-pt", "36"],
            vec!["--pdf-line-numbers"], vec!["--toc-depth", "2"],
        ] {
            let mut args = vec!["fmd", "book", "missing", "--check-links"];
            args.extend(extra);
            assert!(BookCli::try_parse_from(args.clone()).is_err(), "accepted {args:?}");
        }
    }
}
