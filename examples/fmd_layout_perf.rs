use franken_markdown::ast::{Block, Document, Inline};
use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::layout::{
    FontSize, HyphenationOptions, Hyphenator, LayoutUnit, ParagraphItem, ParagraphLayoutScratch,
    break_paragraph_into, hyphenated_paragraph_items_from_text_into,
};
use franken_markdown::{FontFamily, parse_markdown};
use std::env;
use std::fs;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct Args {
    scenario: String,
    iterations: usize,
    out_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct Scenario {
    name: &'static str,
    measure_pt: i32,
    paragraphs: Vec<String>,
    source_bytes: usize,
    notes: &'static str,
}

#[derive(Debug, Clone, Default)]
struct LayoutMetrics {
    paragraph_count: usize,
    word_count: usize,
    hyphenation_count: usize,
    line_count: usize,
    badness_total: i64,
    final_demerits_total: i64,
    widow_orphan_count: usize,
    ledger_bytes: usize,
}

#[derive(Debug, Clone)]
struct Sample {
    scenario: &'static str,
    iterations: usize,
    input_bytes: usize,
    line_width_pt: i32,
    metrics: LayoutMetrics,
    durations: Vec<Duration>,
    notes: &'static str,
}

impl Sample {
    fn print_json(&mut self) {
        self.durations.sort_unstable();
        let p50 = percentile_ns(&self.durations, 50);
        let p95 = percentile_ns(&self.durations, 95);
        let p99 = percentile_ns(&self.durations, 99);
        let min = self.durations.first().map_or(0, |d| d.as_nanos());
        let max = self.durations.last().map_or(0, |d| d.as_nanos());
        let total: u128 = self.durations.iter().map(Duration::as_nanos).sum();
        let mean = if self.durations.is_empty() {
            0
        } else {
            total / self.durations.len() as u128
        };
        println!(
            "{{\"type\":\"perf_sample\",\"scenario\":\"{}\",\"category\":\"layout-linebreak\",\"iterations\":{},\"input_bytes\":{},\"output_bytes\":{},\"line_width_pt\":{},\"paragraph_count\":{},\"word_count\":{},\"hyphenation_count\":{},\"line_count\":{},\"badness_total\":{},\"demerits\":{},\"widow_orphan_count\":{},\"widow_orphan_mode\":\"page_builder_not_modelled_yet\",\"min_ns\":{},\"mean_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{},\"max_ns\":{},\"notes\":\"{}\"}}",
            self.scenario,
            self.iterations,
            self.input_bytes,
            self.metrics.ledger_bytes,
            self.line_width_pt,
            self.metrics.paragraph_count,
            self.metrics.word_count,
            self.metrics.hyphenation_count,
            self.metrics.line_count,
            self.metrics.badness_total,
            self.metrics.final_demerits_total,
            self.metrics.widow_orphan_count,
            min,
            mean,
            p50,
            p95,
            p99,
            max,
            json_escape(self.notes)
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(env::args().skip(1))?;
    if let Some(dir) = &args.out_dir {
        fs::create_dir_all(dir)?;
    }

    let scenarios = scenarios()?;
    let selected: Vec<Scenario> = if args.scenario == "all" {
        scenarios
    } else {
        scenarios
            .into_iter()
            .filter(|scenario| scenario.name == args.scenario)
            .collect()
    };
    if selected.is_empty() {
        return Err(format!(
            "unknown scenario '{}'; use all, paragraph-1000, unique-long-words, narrow-measure, wide-measure, punctuation-heavy, code-table-list-heavy, readme, or generated-large",
            args.scenario
        )
        .into());
    }

    let font = fonts::load_body(FontFamily::Sans, FontStyle::Regular)?;
    let size = FontSize::from_points(11);
    let hyphenator = Hyphenator::english();
    let hyphen_opts = HyphenationOptions::default();

    let mut samples = Vec::new();
    for scenario in &selected {
        samples.push(run_scenario(
            scenario,
            &font,
            size,
            &hyphenator,
            hyphen_opts,
            args.iterations,
            args.out_dir.as_deref(),
        )?);
    }

    for mut sample in samples {
        sample.print_json();
    }
    io::stdout().flush()?;
    Ok(())
}

fn parse_args<I>(mut args: I) -> Result<Args, Box<dyn std::error::Error>>
where
    I: Iterator<Item = String>,
{
    let mut scenario = String::from("all");
    let mut iterations = 25usize;
    let mut out_dir = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => {
                scenario = args
                    .next()
                    .ok_or_else(|| String::from("--scenario requires a value"))?;
            }
            "--iters" | "--iterations" => {
                let raw = args
                    .next()
                    .ok_or_else(|| String::from("--iters requires a value"))?;
                iterations = raw
                    .parse::<usize>()
                    .map_err(|_| format!("--iters must be a positive integer, got '{raw}'"))?;
                if iterations == 0 {
                    return Err("--iters must be greater than zero".into());
                }
            }
            "--out-dir" => {
                let raw = args
                    .next()
                    .ok_or_else(|| String::from("--out-dir requires a value"))?;
                out_dir = Some(PathBuf::from(raw));
            }
            "--help" | "-h" => {
                println!(
                    "Usage: cargo run --profile release-perf --example fmd_layout_perf -- --scenario all --iters 25 --out-dir tests/artifacts/perf/<run>/golden"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'").into()),
        }
    }

    Ok(Args {
        scenario,
        iterations,
        out_dir,
    })
}

fn run_scenario(
    scenario: &Scenario,
    font: &franken_markdown::text::Font,
    size: FontSize,
    hyphenator: &Hyphenator,
    hyphen_opts: HyphenationOptions,
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let (metrics, ledger) = analyze_scenario(scenario, font, size, hyphenator, hyphen_opts);
    write_golden(
        out_dir,
        &format!("{}.breaks.tsv", scenario.name),
        ledger.as_bytes(),
    )?;

    let durations = measure(iterations, || {
        let metrics = analyze_scenario_metrics(scenario, font, size, hyphenator, hyphen_opts);
        black_box(metrics.line_count)
    });

    Ok(Sample {
        scenario: scenario.name,
        iterations,
        input_bytes: scenario.source_bytes,
        line_width_pt: scenario.measure_pt,
        metrics: LayoutMetrics {
            ledger_bytes: ledger.len(),
            ..metrics
        },
        durations,
        notes: scenario.notes,
    })
}

fn analyze_scenario(
    scenario: &Scenario,
    font: &franken_markdown::text::Font,
    size: FontSize,
    hyphenator: &Hyphenator,
    hyphen_opts: HyphenationOptions,
) -> (LayoutMetrics, String) {
    analyze_scenario_inner(scenario, font, size, hyphenator, hyphen_opts, true)
}

fn analyze_scenario_metrics(
    scenario: &Scenario,
    font: &franken_markdown::text::Font,
    size: FontSize,
    hyphenator: &Hyphenator,
    hyphen_opts: HyphenationOptions,
) -> LayoutMetrics {
    analyze_scenario_inner(scenario, font, size, hyphenator, hyphen_opts, false).0
}

fn analyze_scenario_inner(
    scenario: &Scenario,
    font: &franken_markdown::text::Font,
    size: FontSize,
    hyphenator: &Hyphenator,
    hyphen_opts: HyphenationOptions,
    emit_ledger: bool,
) -> (LayoutMetrics, String) {
    let line_width = LayoutUnit::from_points(scenario.measure_pt);
    let mut metrics = LayoutMetrics::default();
    let mut scratch = ParagraphLayoutScratch::new();
    let mut items: Vec<ParagraphItem> = Vec::new();
    let mut breaks = Vec::new();
    let mut ledger = if emit_ledger {
        String::from(
            "paragraph\tline\tstart_item\tend_item\tnext_item\tnatural_width_mpt\tbadness\tdemerits\n",
        )
    } else {
        String::new()
    };

    for (paragraph_idx, paragraph) in scenario
        .paragraphs
        .iter()
        .filter(|paragraph| !paragraph.trim().is_empty())
        .enumerate()
    {
        metrics.paragraph_count += 1;
        metrics.word_count += paragraph.split_whitespace().count();
        metrics.hyphenation_count += count_hyphen_points(paragraph, hyphenator, hyphen_opts);

        hyphenated_paragraph_items_from_text_into(
            font,
            hyphenator,
            paragraph,
            size,
            &mut scratch,
            &mut items,
        );
        break_paragraph_into(&items, line_width, &mut scratch, &mut breaks);
        metrics.line_count += breaks.len();
        if let Some(last) = breaks.last() {
            metrics.final_demerits_total =
                metrics.final_demerits_total.saturating_add(last.demerits);
        }
        for (line_idx, br) in breaks.iter().enumerate() {
            metrics.badness_total = metrics.badness_total.saturating_add(i64::from(br.badness));
            if emit_ledger {
                ledger.push_str(&paragraph_idx.to_string());
                ledger.push('\t');
                ledger.push_str(&line_idx.to_string());
                ledger.push('\t');
                ledger.push_str(&br.start.to_string());
                ledger.push('\t');
                ledger.push_str(&br.end.to_string());
                ledger.push('\t');
                ledger.push_str(&br.next.to_string());
                ledger.push('\t');
                ledger.push_str(&br.natural_width.milli_points().to_string());
                ledger.push('\t');
                ledger.push_str(&br.badness.to_string());
                ledger.push('\t');
                ledger.push_str(&br.demerits.to_string());
                ledger.push('\n');
            }
        }
    }

    (metrics, ledger)
}

fn scenarios() -> Result<Vec<Scenario>, Box<dyn std::error::Error>> {
    let paragraph_1000 = generated_words(1_000);
    let unique_long_words = generated_unique_long_words(420);
    let balanced = balanced_prose(28);
    let punctuation = punctuation_heavy_prose(42);
    let code_table_list = code_table_list_heavy();
    let readme = fs::read_to_string("README.md")?;
    let generated_large = generated_large_markdown(160);

    Ok(vec![
        Scenario {
            name: "paragraph-1000",
            measure_pt: 468,
            source_bytes: paragraph_1000.len(),
            paragraphs: vec![paragraph_1000],
            notes: "1000-word repeated paragraph through hyphenated Knuth-Plass line breaking",
        },
        Scenario {
            name: "unique-long-words",
            measure_pt: 360,
            source_bytes: unique_long_words.len(),
            paragraphs: vec![unique_long_words],
            notes: "unique long documentation words stress hyphenation and badness decisions",
        },
        Scenario {
            name: "narrow-measure",
            measure_pt: 180,
            source_bytes: balanced.len(),
            paragraphs: split_markdown_paragraphs(&balanced),
            notes: "narrow measure prose should create many legal balanced breaks",
        },
        Scenario {
            name: "wide-measure",
            measure_pt: 540,
            source_bytes: balanced.len(),
            paragraphs: split_markdown_paragraphs(&balanced),
            notes: "wide measure prose should produce fewer lower-badness lines",
        },
        Scenario {
            name: "punctuation-heavy",
            measure_pt: 420,
            source_bytes: punctuation.len(),
            paragraphs: split_markdown_paragraphs(&punctuation),
            notes: "punctuation-heavy prose catches spacing, tokenization, and line-quality edges",
        },
        Scenario {
            name: "code-table-list-heavy",
            measure_pt: 396,
            source_bytes: code_table_list.len(),
            paragraphs: paragraphs_from_markdown(&code_table_list),
            notes: "Markdown document with lists, tables, and code projected into layout text",
        },
        Scenario {
            name: "readme",
            measure_pt: 468,
            source_bytes: readme.len(),
            paragraphs: paragraphs_from_markdown(&readme),
            notes: "real project README text projected through the layout engine",
        },
        Scenario {
            name: "generated-large",
            measure_pt: 432,
            source_bytes: generated_large.len(),
            paragraphs: paragraphs_from_markdown(&generated_large),
            notes: "generated large mixed Markdown document for layout regression pressure",
        },
    ])
}

fn paragraphs_from_markdown(markdown: &str) -> Vec<String> {
    let doc = parse_markdown(markdown);
    let mut out = Vec::new();
    collect_document_text(&doc, &mut out);
    if out.is_empty() {
        split_markdown_paragraphs(markdown)
    } else {
        out
    }
}

fn collect_document_text(doc: &Document, out: &mut Vec<String>) {
    collect_blocks_text(&doc.blocks, out);
}

fn collect_blocks_text(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                push_nonempty(out, inline_text(inlines));
            }
            Block::CodeBlock { code, .. } | Block::HtmlBlock(code) => {
                for line in code.lines() {
                    push_nonempty(out, line.to_string());
                }
            }
            Block::BlockQuote(inner) => collect_blocks_text(inner, out),
            Block::List(list) => {
                for item in &list.items {
                    collect_blocks_text(&item.blocks, out);
                }
            }
            Block::Table(table) => {
                push_nonempty(out, inline_cells_text(&table.head));
                for row in &table.rows {
                    push_nonempty(out, inline_cells_text(row));
                }
            }
            Block::ThematicBreak => {}
        }
    }
}

fn inline_cells_text(cells: &[Vec<Inline>]) -> String {
    cells
        .iter()
        .map(|cell| inline_text(cell))
        .collect::<Vec<_>>()
        .join(" ")
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) | Inline::Code(text) | Inline::Html(text) => out.push_str(text),
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                out.push_str(&inline_text(inner));
            }
            Inline::Link { content, .. } => out.push_str(&inline_text(content)),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        }
    }
    out
}

fn push_nonempty(out: &mut Vec<String>, text: String) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
}

fn split_markdown_paragraphs(src: &str) -> Vec<String> {
    src.split("\n\n")
        .map(|chunk| chunk.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

fn count_hyphen_points(
    text: &str,
    hyphenator: &Hyphenator,
    hyphen_opts: HyphenationOptions,
) -> usize {
    let mut total = 0usize;
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            word.push(ch);
        } else if !word.is_empty() {
            total += hyphenator.hyphenation_points(&word, hyphen_opts).len();
            word.clear();
        }
    }
    if !word.is_empty() {
        total += hyphenator.hyphenation_points(&word, hyphen_opts).len();
    }
    total
}

fn generated_words(count: usize) -> String {
    const WORDS: &[&str] = &[
        "typography",
        "rendering",
        "deterministic",
        "paragraph",
        "hyphenation",
        "microtype",
        "baseline",
        "ligature",
        "kerning",
        "markdown",
        "performance",
        "document",
    ];
    let mut out = String::new();
    for i in 0..count {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(WORDS[i % WORDS.len()]);
    }
    out
}

fn generated_unique_long_words(count: usize) -> String {
    const STEMS: &[&str] = &[
        "internationalization",
        "characteristically",
        "interoperability",
        "institutionalization",
        "hyperoptimization",
        "microtypographical",
        "documentation",
        "deterministically",
        "accessibility",
        "reproducibility",
        "compositionality",
        "parameterization",
    ];
    let mut out = String::new();
    for i in 0..count {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(STEMS[i % STEMS.len()]);
        out.push_str(&alpha_suffix(i));
    }
    out
}

fn alpha_suffix(n: usize) -> String {
    const SUFFIXES: &[&str] = &[
        "able", "ation", "istic", "izing", "ness", "ally", "ward", "ments", "ology", "ative",
    ];
    let mut value = n / SUFFIXES.len();
    let mut letters = Vec::new();
    loop {
        let offset = (value % 26) as u8;
        letters.push(char::from(b'a' + offset));
        value /= 26;
        if value == 0 {
            break;
        }
    }
    letters.reverse();

    let mut suffix = String::from(SUFFIXES[n % SUFFIXES.len()]);
    for ch in letters {
        suffix.push(ch);
    }
    suffix
}

fn balanced_prose(repeats: usize) -> String {
    let paragraph = "A polished document needs calm line lengths, consistent rhythm, real kerning, and breakpoints that avoid distracting rivers of whitespace. The layout engine should prefer balanced rows while preserving deterministic output for native and browser callers.";
    repeat_paragraph(paragraph, repeats)
}

fn punctuation_heavy_prose(repeats: usize) -> String {
    let paragraph = "Quotes, commas, semicolons, ellipses, and parenthetical asides all stress spacing: \"measure twice, break once,\" as the proof ledger says; meanwhile, code-like tokens such as render_pdf(), --out, and SOURCE_DATE_EPOCH should not destabilize paragraph quality.";
    repeat_paragraph(paragraph, repeats)
}

fn repeat_paragraph(paragraph: &str, repeats: usize) -> String {
    let mut out = String::new();
    for i in 0..repeats {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(paragraph);
        out.push(' ');
        out.push_str(&generated_words(18));
    }
    out
}

fn code_table_list_heavy() -> String {
    let mut out = String::from("# Layout Mixed Document\n\n");
    for i in 0..36 {
        out.push_str("## Workstream ");
        out.push_str(&i.to_string());
        out.push_str("\n\n");
        out.push_str("- [x] Measure deterministic line breaks for tables and lists.\n");
        out.push_str("- [ ] Preserve code readability while collecting layout ledgers.\n\n");
        out.push_str("| Area | Goal | Detail |\n|---|---|---|\n");
        out.push_str("| parser | stable | reference links and table cells remain measurable |\n");
        out.push_str("| pdf | polished | blockquotes, code, and lists keep readable rhythm |\n\n");
        out.push_str("```rust\n");
        out.push_str("fn layout_metric(value: usize) -> usize { value.wrapping_mul(17) }\n");
        out.push_str("```\n\n");
    }
    out
}

fn generated_large_markdown(sections: usize) -> String {
    let mut out = String::from("# Generated Layout Corpus\n\n");
    for i in 0..sections {
        out.push_str("## Generated Section ");
        out.push_str(&i.to_string());
        out.push_str("\n\n");
        out.push_str("This generated section combines prose, structured Markdown, punctuation, and long documentation words so the line-breaking harness can detect badness and demerit drift across a larger document. ");
        out.push_str(&generated_words(44));
        out.push_str("\n\n");
        out.push_str("| Metric | Value | Notes |\n|---:|:---:|---|\n");
        out.push_str("| ");
        out.push_str(&i.to_string());
        out.push_str(" | green | measured-column table text for layout proof |\n\n");
    }
    out
}

fn measure<F>(iterations: usize, mut f: F) -> Vec<Duration>
where
    F: FnMut() -> usize,
{
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let value = f();
        black_box(value);
        durations.push(start.elapsed());
    }
    durations
}

fn percentile_ns(durations: &[Duration], pct: usize) -> u128 {
    if durations.is_empty() {
        return 0;
    }
    let idx = ((durations.len() - 1) * pct).div_ceil(100);
    durations[idx].as_nanos()
}

fn write_golden(
    out_dir: Option<&Path>,
    name: &str,
    bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dir) = out_dir {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
