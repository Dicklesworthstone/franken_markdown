use franken_markdown::ast::{Block, Document, Inline, List};
use franken_markdown::{HtmlOptions, parse_markdown, parse_markdown_spanned, render_html_document};
use std::env;
use std::fs;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BEAD_ID: &str = "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.5";

#[derive(Debug, Clone)]
struct Args {
    iterations: usize,
    out_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct Scenario {
    name: &'static str,
    source: String,
    notes: &'static str,
    min_diagnostics: usize,
}

#[derive(Debug, Clone, Copy)]
struct AstCounts {
    blocks: usize,
    inlines: usize,
}

#[derive(Debug, Clone)]
struct Sample {
    scenario: &'static str,
    iterations: usize,
    input_bytes: usize,
    output_bytes: usize,
    block_count: usize,
    inline_count: usize,
    diagnostic_count: usize,
    output_checksum: u64,
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
            "{{\"type\":\"perf_sample\",\"scenario\":\"{}\",\"category\":\"parse\",\"iterations\":{},\"input_bytes\":{},\"output_bytes\":{},\"block_count\":{},\"inline_count\":{},\"diagnostic_count\":{},\"output_checksum\":\"fnv1a64:{:016x}\",\"min_ns\":{},\"mean_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{},\"max_ns\":{},\"notes\":\"{}\"}}",
            self.scenario,
            self.iterations,
            self.input_bytes,
            self.output_bytes,
            self.block_count,
            self.inline_count,
            self.diagnostic_count,
            self.output_checksum,
            min,
            mean,
            p50,
            p95,
            p99,
            max,
            json_escape(self.notes),
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(env::args().skip(1))?;
    if let Some(dir) = &args.out_dir {
        fs::create_dir_all(dir)?;
    }

    for scenario in scenarios()? {
        println!(
            "{{\"type\":\"scenario_start\",\"scenario\":\"{}\",\"category\":\"parse\",\"input_bytes\":{},\"iterations\":{},\"notes\":\"{}\"}}",
            scenario.name,
            scenario.source.len(),
            args.iterations,
            json_escape(scenario.notes),
        );
        let mut sample = run_scenario(&scenario, args.iterations, args.out_dir.as_deref())?;
        sample.print_json();
    }
    println!(
        "{{\"type\":\"proof_obligation\",\"bead_id\":\"{}\",\"obligation\":\"parser_perf_harness_ast_and_html_equivalence\",\"status\":\"pass\",\"evidence_path\":\"inprocess.jsonl\",\"notes\":\"every scenario parsed normally, parsed through the spanned diagnostics path, converted back to the same AST, and rendered deterministic HTML\"}}",
        BEAD_ID
    );
    io::stdout().flush()?;
    Ok(())
}

fn parse_args<I>(mut args: I) -> Result<Args, Box<dyn std::error::Error>>
where
    I: Iterator<Item = String>,
{
    let mut iterations = 25usize;
    let mut out_dir = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
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
                    "Usage: cargo run --profile release-perf --example fmd_parser_perf -- --iters 25 --out-dir tests/artifacts/perf/<run>/golden"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'").into()),
        }
    }

    Ok(Args {
        iterations,
        out_dir,
    })
}

fn scenarios() -> Result<Vec<Scenario>, Box<dyn std::error::Error>> {
    Ok(vec![
        Scenario {
            name: "parser-large-1mib",
            source: generated_markdown_bytes(1_048_576),
            notes: "generated 1 MiB Markdown document mixing headings, paragraphs, tables, code fences, lists, and inline markup",
            min_diagnostics: 0,
        },
        Scenario {
            name: "readme",
            source: fs::read_to_string("README.md")?,
            notes: "repository README.md as a real-world project documentation input",
            min_diagnostics: 0,
        },
        Scenario {
            name: "showcase",
            source: fs::read_to_string("examples/showcase.md")?,
            notes: "examples/showcase.md visual smoke document",
            min_diagnostics: 0,
        },
        Scenario {
            name: "table-heavy",
            source: table_heavy_fixture(),
            notes: "generated table-heavy fixture stressing GFM pipe-table splitting and inline cells",
            min_diagnostics: 0,
        },
        Scenario {
            name: "reference-link-heavy",
            source: reference_link_heavy_fixture(),
            notes: "generated reference-link-heavy fixture stressing definition collection and inline resolution",
            min_diagnostics: 0,
        },
        Scenario {
            name: "code-fence-heavy",
            source: code_fence_heavy_fixture(),
            notes: "generated code-fence-heavy fixture stressing fenced-code block recognition and literal preservation",
            min_diagnostics: 0,
        },
        Scenario {
            name: "malformed-diagnostics",
            source: malformed_diagnostic_fixture(),
            notes: "malformed-but-recoverable fixture that must keep rendering while surfacing parser diagnostics",
            min_diagnostics: 1,
        },
    ])
}

fn run_scenario(
    scenario: &Scenario,
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let doc = parse_markdown(&scenario.source);
    let spanned = parse_markdown_spanned(&scenario.source);
    let diagnostics = spanned.diagnostics.len();
    if diagnostics < scenario.min_diagnostics {
        return Err(format!(
            "scenario '{}' expected at least {} diagnostic(s), got {}",
            scenario.name, scenario.min_diagnostics, diagnostics
        )
        .into());
    }
    if spanned.to_document() != doc {
        return Err(format!(
            "scenario '{}' spanned parse converted to a different AST",
            scenario.name
        )
        .into());
    }

    let html = render_html_document(&doc, &HtmlOptions::default())?;
    let counts = count_document(&doc);
    write_golden(out_dir, &format!("{}.html", scenario.name), html.as_bytes())?;
    let output_checksum = fnv1a64(html.as_bytes());
    let durations = measure(iterations, || {
        let parsed = parse_markdown(&scenario.source);
        let block_count = parsed.blocks.len();
        black_box(parsed);
        block_count
    });

    Ok(Sample {
        scenario: scenario.name,
        iterations,
        input_bytes: scenario.source.len(),
        output_bytes: html.len(),
        block_count: counts.blocks,
        inline_count: counts.inlines,
        diagnostic_count: diagnostics,
        output_checksum,
        durations,
        notes: scenario.notes,
    })
}

fn count_document(doc: &Document) -> AstCounts {
    count_blocks(&doc.blocks)
}

fn count_blocks(blocks: &[Block]) -> AstCounts {
    let mut counts = AstCounts {
        blocks: 0,
        inlines: 0,
    };
    for block in blocks {
        counts.blocks = counts.blocks.saturating_add(1);
        let nested = count_block(block);
        counts.blocks = counts.blocks.saturating_add(nested.blocks);
        counts.inlines = counts.inlines.saturating_add(nested.inlines);
    }
    counts
}

fn count_block(block: &Block) -> AstCounts {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => AstCounts {
            blocks: 0,
            inlines: count_inlines(inlines),
        },
        Block::BlockQuote(blocks) => count_blocks(blocks),
        Block::List(list) => count_list(list),
        Block::Table(table) => {
            let mut inlines = table.head.iter().map(|cell| count_inlines(cell)).sum();
            for row in &table.rows {
                inlines += row.iter().map(|cell| count_inlines(cell)).sum::<usize>();
            }
            AstCounts { blocks: 0, inlines }
        }
        Block::CodeBlock { .. } | Block::ThematicBreak | Block::HtmlBlock(_) => AstCounts {
            blocks: 0,
            inlines: 0,
        },
    }
}

fn count_list(list: &List) -> AstCounts {
    let mut counts = AstCounts {
        blocks: 0,
        inlines: 0,
    };
    for item in &list.items {
        let nested = count_blocks(&item.blocks);
        counts.blocks = counts.blocks.saturating_add(nested.blocks);
        counts.inlines = counts.inlines.saturating_add(nested.inlines);
    }
    counts
}

fn count_inlines(inlines: &[Inline]) -> usize {
    let mut count = 0usize;
    for inline in inlines {
        count = count.saturating_add(1);
        count = count.saturating_add(match inline {
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children)
            | Inline::Link {
                content: children, ..
            } => count_inlines(children),
            Inline::Text(_)
            | Inline::Code(_)
            | Inline::Image { .. }
            | Inline::SoftBreak
            | Inline::HardBreak
            | Inline::Html(_) => 0,
        });
    }
    count
}

fn measure<F>(iterations: usize, mut f: F) -> Vec<Duration>
where
    F: FnMut() -> usize,
{
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let value = f();
        let elapsed = start.elapsed();
        black_box(value);
        durations.push(elapsed);
    }
    durations
}

fn percentile_ns(durations: &[Duration], percentile: usize) -> u128 {
    if durations.is_empty() {
        return 0;
    }
    let p = percentile.min(100);
    let idx = ((durations.len() - 1) * p).div_ceil(100);
    durations[idx].as_nanos()
}

fn write_golden(out_dir: Option<&Path>, name: &str, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = out_dir {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}

fn generated_markdown_bytes(target: usize) -> String {
    let seed = fs::read_to_string("examples/showcase.md").unwrap_or_else(|_| {
        String::from("# fallback\n\nA paragraph with **bold** text and `code`.\n")
    });
    let mut out = String::with_capacity(target + seed.len());
    let mut section = 0usize;
    while out.len() < target {
        out.push_str("\n\n## Generated Section ");
        out.push_str(&section.to_string());
        out.push_str("\n\n");
        out.push_str(&seed);
        out.push_str(
            "\n\n| alpha | beta | gamma |\n|---:|:---:|---|\n| 123 | middle | trailing |\n",
        );
        out.push_str("\n```rust\nfn generated(value: usize) -> usize { value + 1 }\n```\n");
        section = section.saturating_add(1);
    }
    truncate_to_char_boundary(&mut out, target);
    out
}

fn truncate_to_char_boundary(s: &mut String, max_len: usize) {
    if s.len() <= max_len {
        return;
    }

    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
}

fn table_heavy_fixture() -> String {
    let mut out = String::from("# Table Heavy Fixture\n\n");
    for section in 0..120 {
        out.push_str("## Table Section ");
        out.push_str(&section.to_string());
        out.push_str("\n\n| Metric | Value | Trend | Notes |\n|---|---:|:---:|---|\n");
        for row in 0..16 {
            out.push_str("| Parser ");
            out.push_str(&row.to_string());
            out.push_str(" | ");
            out.push_str(&(section * 100 + row).to_string());
            out.push_str(" | up | `inline` **strong** text |\n");
        }
        out.push('\n');
    }
    out
}

fn reference_link_heavy_fixture() -> String {
    let mut out = String::from("# Reference Link Heavy Fixture\n\n");
    for i in 0..800 {
        out.push_str("This paragraph links to [Reference ");
        out.push_str(&i.to_string());
        out.push_str("][ref-");
        out.push_str(&i.to_string());
        out.push_str("] and repeats [shortcut-");
        out.push_str(&i.to_string());
        out.push_str("].\n\n");
    }
    for i in 0..800 {
        out.push_str("[ref-");
        out.push_str(&i.to_string());
        out.push_str("]: https://example.test/reference/");
        out.push_str(&i.to_string());
        out.push_str(" \"Reference ");
        out.push_str(&i.to_string());
        out.push_str("\"\n");
        out.push_str("[shortcut-");
        out.push_str(&i.to_string());
        out.push_str("]: https://example.test/shortcut/");
        out.push_str(&i.to_string());
        out.push('\n');
    }
    out
}

fn code_fence_heavy_fixture() -> String {
    let mut out = String::from("# Code Fence Heavy Fixture\n\n");
    for i in 0..500 {
        out.push_str("## Fence ");
        out.push_str(&i.to_string());
        out.push_str("\n\n```rust\n");
        out.push_str("fn hot_path(input: &str) -> usize {\n");
        out.push_str("    input.bytes().filter(|b| *b == b'|').count()\n");
        out.push_str("}\n");
        out.push_str("```\n\n");
    }
    out
}

fn malformed_diagnostic_fixture() -> String {
    String::from(
        "# Recoverable Diagnostics\n\n[bad]:\n\nThis paragraph still renders after a malformed reference definition.\n\n```rust\nfn unclosed() {}\n",
    )
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
