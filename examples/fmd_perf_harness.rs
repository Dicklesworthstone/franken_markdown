use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::layout::{
    FontSize, HyphenationOptions, Hyphenator, LayoutUnit, break_paragraph,
    paragraph_items_from_text,
};
use franken_markdown::{
    FontFamily, HtmlOptions, PdfOptions, Theme, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned_profiled, render_html, render_html_document, render_pdf,
    render_pdf_document, render_pdf_document_profiled,
};
use std::collections::BTreeMap;
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
struct Sample {
    scenario: &'static str,
    category: &'static str,
    iterations: usize,
    bytes: usize,
    output_bytes: usize,
    durations: Vec<Duration>,
    notes: String,
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
            "{{\"type\":\"perf_sample\",\"scenario\":\"{}\",\"category\":\"{}\",\"iterations\":{},\"input_bytes\":{},\"output_bytes\":{},\"min_ns\":{},\"mean_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{},\"max_ns\":{},\"notes\":\"{}\"}}",
            self.scenario,
            self.category,
            self.iterations,
            self.bytes,
            self.output_bytes,
            min,
            mean,
            p50,
            p95,
            p99,
            max,
            json_escape(&self.notes)
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(env::args().skip(1))?;
    if let Some(dir) = &args.out_dir {
        fs::create_dir_all(dir)?;
    }

    let mut samples = Vec::new();
    match args.scenario.as_str() {
        "all" => {
            samples.push(html_showcase(args.iterations, args.out_dir.as_deref())?);
            samples.push(pdf_showcase(args.iterations, args.out_dir.as_deref())?);
            samples.push(parser_large(args.iterations, args.out_dir.as_deref())?);
            samples.push(paragraph_1k(args.iterations, args.out_dir.as_deref())?);
            samples.push(hyphen_corpus(args.iterations, args.out_dir.as_deref())?);
            samples.push(font_subset(args.iterations, args.out_dir.as_deref())?);
            samples.push(pdf_large(args.iterations, args.out_dir.as_deref())?);
        }
        "html-showcase" => samples.push(html_showcase(args.iterations, args.out_dir.as_deref())?),
        "pdf-showcase" => samples.push(pdf_showcase(args.iterations, args.out_dir.as_deref())?),
        "parser-large" => samples.push(parser_large(args.iterations, args.out_dir.as_deref())?),
        "paragraph-1k" => samples.push(paragraph_1k(args.iterations, args.out_dir.as_deref())?),
        "hyphen-corpus" => samples.push(hyphen_corpus(args.iterations, args.out_dir.as_deref())?),
        "font-subset" => samples.push(font_subset(args.iterations, args.out_dir.as_deref())?),
        "pdf-large" => samples.push(pdf_large(args.iterations, args.out_dir.as_deref())?),
        _ => {
            return Err(format!(
                "unknown scenario '{}'; use all, html-showcase, pdf-showcase, parser-large, paragraph-1k, hyphen-corpus, font-subset, or pdf-large",
                args.scenario
            )
            .into());
        }
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
    let mut iterations = 100usize;
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
                    .map_err(|_| format!("--iters must be a positive integer, got '{}'", raw))?;
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
                    "Usage: cargo run --profile release-perf --example fmd_perf_harness -- --scenario all --iters 100 --out-dir tests/artifacts/perf/<run>/golden"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{}'", other).into()),
        }
    }

    Ok(Args {
        scenario,
        iterations,
        out_dir,
    })
}

fn html_showcase(
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let src = fs::read_to_string("examples/showcase.md")?;
    let opts = HtmlOptions::default();
    let golden = render_html(&src, &opts)?;
    write_golden(out_dir, "html-showcase.html", golden.as_bytes())?;
    let durations = measure(iterations, || {
        let html = render_html(&src, &opts).unwrap_or_default();
        black_box(html.len())
    });
    Ok(Sample {
        scenario: "html-showcase",
        category: "render-html",
        iterations,
        bytes: src.len(),
        output_bytes: golden.len(),
        durations,
        notes: String::from("parse + html render of examples/showcase.md"),
    })
}

fn pdf_showcase(
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let src = fs::read_to_string("examples/showcase.md")?;
    let opts = PdfOptions::default();
    let golden = render_pdf(&src, &opts)?;
    write_golden(out_dir, "pdf-showcase.pdf", &golden)?;
    let durations = measure(iterations, || {
        let pdf = render_pdf(&src, &opts).unwrap_or_default();
        black_box(pdf.len())
    });
    Ok(Sample {
        scenario: "pdf-showcase",
        category: "render-pdf",
        iterations,
        bytes: src.len(),
        output_bytes: golden.len(),
        durations,
        notes: String::from("parse + embedded-font PDF render of examples/showcase.md"),
    })
}

fn parser_large(
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let src = generated_markdown_bytes(1_048_576);
    let doc = parse_markdown(&src);
    let html = render_html_document(&doc, &HtmlOptions::default())?;
    write_golden(out_dir, "parser-large.html", html.as_bytes())?;
    write_parser_large_stage_artifacts(out_dir, iterations, &src, &doc, &html)?;
    let durations = measure(iterations, || {
        let doc = parse_markdown(&src);
        black_box(doc.blocks.len())
    });
    Ok(Sample {
        scenario: "parser-large",
        category: "parse",
        iterations,
        bytes: src.len(),
        output_bytes: html.len(),
        durations,
        notes: String::from("parse generated 1 MiB CommonMark/GFM-like document"),
    })
}

fn paragraph_1k(
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let text = generated_words(1_000);
    let font = fonts::load_body(FontFamily::Sans, FontStyle::Regular)?;
    let size = FontSize::from_points(11);
    let width = LayoutUnit::from_points(468);
    let items = paragraph_items_from_text(&font, &text, size);
    let breaks = break_paragraph(&items, width);
    write_golden(
        out_dir,
        "paragraph-1k.breaks",
        line_break_ledger(&breaks).as_bytes(),
    )?;
    let durations = measure(iterations, || {
        let breaks = break_paragraph(&items, width);
        black_box(breaks.len())
    });
    Ok(Sample {
        scenario: "paragraph-1k",
        category: "line-break",
        iterations,
        bytes: text.len(),
        output_bytes: breaks.len(),
        durations,
        notes: String::from("Knuth-Plass baseline breaker over 1000 generated words"),
    })
}

fn hyphen_corpus(
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let words = generated_hyphen_words(50_000);
    let hyphenator = Hyphenator::english();
    let opts = HyphenationOptions::default();
    let ledger = hyphen_ledger(&hyphenator, &words[..words.len().min(256)], opts);
    write_golden(out_dir, "hyphen-corpus.points", ledger.as_bytes())?;
    let durations = measure(iterations, || {
        let mut total = 0usize;
        for word in &words {
            total += hyphenator.hyphenation_points(word, opts).len();
        }
        black_box(total)
    });
    Ok(Sample {
        scenario: "hyphen-corpus",
        category: "hyphenation",
        iterations,
        bytes: words.iter().map(String::len).sum(),
        output_bytes: words.len(),
        durations,
        notes: String::from("Liang/TeX hyphenation over 50k generated documentation words"),
    })
}

fn font_subset(
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let src = generated_markdown_bytes(80_000);
    let keep = unique_chars(&src);
    let font = fonts::load_body(FontFamily::Sans, FontStyle::Regular)?;
    let golden = font.subset(&keep).unwrap_or_default();
    write_golden(out_dir, "font-subset.ttf", &golden)?;
    let durations = measure(iterations, || {
        let subset = font.subset(&keep).unwrap_or_default();
        black_box(subset.len())
    });
    Ok(Sample {
        scenario: "font-subset",
        category: "font-subset",
        iterations,
        bytes: keep.len(),
        output_bytes: golden.len(),
        durations,
        notes: String::from("subset bundled IBM Plex Sans over generated document character set"),
    })
}

fn pdf_large(
    iterations: usize,
    out_dir: Option<&Path>,
) -> Result<Sample, Box<dyn std::error::Error>> {
    let src = generated_pdf_large();
    let doc = parse_markdown(&src);
    let opts = PdfOptions {
        theme: Theme::default(),
        title: Some(String::from("fmd large perf document")),
        allow_raw_html: false,
        ..PdfOptions::default()
    };
    let golden = render_pdf_document(&doc, &opts)?;
    write_golden(out_dir, "pdf-large.pdf", &golden)?;
    write_pdf_large_stage_artifacts(
        out_dir,
        iterations,
        src.len(),
        golden.len(),
        &doc,
        &opts,
        &golden,
    )?;
    let durations = measure(iterations, || {
        let pdf = render_pdf_document(&doc, &opts).unwrap_or_default();
        black_box(pdf.len())
    });
    Ok(Sample {
        scenario: "pdf-large",
        category: "render-pdf",
        iterations,
        bytes: src.len(),
        output_bytes: golden.len(),
        durations,
        notes: String::from("render pre-parsed large mixed Markdown document to PDF"),
    })
}

#[derive(Default)]
struct StageBucket {
    samples: Vec<u128>,
    work_count: usize,
    max_bytes: usize,
    total_bytes: usize,
    allocation_count: usize,
    total_ns: u128,
    notes: &'static str,
}

fn write_pdf_large_stage_artifacts(
    out_dir: Option<&Path>,
    iterations: usize,
    input_bytes: usize,
    output_bytes: usize,
    doc: &franken_markdown::Document,
    opts: &PdfOptions,
    golden: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = out_dir else {
        return Ok(());
    };

    let mut buckets: BTreeMap<&'static str, StageBucket> = BTreeMap::new();
    for _ in 0..iterations {
        let profile = render_pdf_document_profiled(doc, opts)?;
        if profile.bytes != golden {
            return Err("profiled pdf-large render bytes differed from normal render bytes".into());
        }
        for stage in profile.stages {
            let bucket = buckets.entry(stage.stage).or_default();
            bucket.samples.push(stage.elapsed_ns);
            bucket.work_count = bucket.work_count.saturating_add(stage.count);
            bucket.max_bytes = bucket.max_bytes.max(stage.bytes);
            bucket.total_bytes = bucket.total_bytes.saturating_add(stage.bytes);
            bucket.total_ns = bucket.total_ns.saturating_add(stage.elapsed_ns);
            bucket.notes = stage.notes;
        }
    }

    let stage_path = dir.join("pdf-large-stages.jsonl");
    let mut stage_jsonl = String::new();
    stage_jsonl.push_str(&format!(
        "{{\"type\":\"scenario_start\",\"scenario\":\"pdf-large\",\"category\":\"render-pdf\",\"input_bytes\":{},\"iterations\":{},\"notes\":\"{}\"}}\n",
        input_bytes,
        iterations,
        "stage attribution for pre-parsed large mixed Markdown document to PDF"
    ));

    let mut ranked: Vec<(&'static str, u128, u128)> = Vec::new();
    for (stage, bucket) in &mut buckets {
        bucket.samples.sort_unstable();
        let p50 = percentile_ns_u128(&bucket.samples, 50);
        let p95 = percentile_ns_u128(&bucket.samples, 95);
        let p99 = percentile_ns_u128(&bucket.samples, 99);
        let max = bucket.samples.last().copied().unwrap_or(0);
        ranked.push((*stage, p95, bucket.total_ns));
        stage_jsonl.push_str(&format!(
            "{{\"type\":\"stage_summary\",\"scenario\":\"pdf-large\",\"stage\":\"{}\",\"count\":{},\"work_count\":{},\"total_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{},\"max_ns\":{},\"output_bytes\":{},\"total_output_bytes\":{},\"unit\":\"ns\",\"notes\":\"{}\"}}\n",
            stage,
            bucket.samples.len(),
            bucket.work_count,
            bucket.total_ns,
            p50,
            p95,
            p99,
            max,
            bucket.max_bytes,
            bucket.total_bytes,
            json_escape(bucket.notes),
        ));
    }
    stage_jsonl.push_str(&format!(
        "{{\"type\":\"proof_obligation\",\"bead_id\":\"br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.1\",\"obligation\":\"profiled_bytes_equal_normal_pdf\",\"status\":\"pass\",\"evidence_path\":\"golden/pdf-large.pdf\",\"notes\":\"{}\"}}\n",
        json_escape(&format!(
            "{} profiled iteration(s) matched normal render bytes exactly; output_bytes={output_bytes}",
            iterations
        ))
    ));
    fs::write(&stage_path, stage_jsonl)?;

    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));
    let (top_stage, top_p95, top_total) = ranked
        .iter()
        .find(|(stage, _, _)| !stage.ends_with("_total"))
        .copied()
        .unwrap_or(("unknown", 0, 0));
    let recommended = recommended_pdf_perf_bead(top_stage);
    let recommendation_path = dir.join("pdf-large-recommendation.jsonl");
    fs::write(
        recommendation_path,
        format!(
            "{{\"type\":\"next_target_recommendation\",\"recommended_bead_id\":\"{}\",\"reason\":\"{}\",\"evidence_path\":\"{}\",\"confidence\":\"{}\",\"top_stage\":\"{}\",\"top_stage_p95_ns\":{},\"top_stage_total_ns\":{}}}\n",
            recommended.bead_id,
            json_escape(recommended.reason),
            "golden/pdf-large-stages.jsonl",
            recommended.confidence,
            top_stage,
            top_p95,
            top_total,
        ),
    )?;
    Ok(())
}

fn write_parser_large_stage_artifacts(
    out_dir: Option<&Path>,
    iterations: usize,
    src: &str,
    normal_doc: &franken_markdown::Document,
    golden_html: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = out_dir else {
        return Ok(());
    };

    let mut buckets: BTreeMap<&'static str, StageBucket> = BTreeMap::new();
    for _ in 0..iterations {
        let profile = parse_markdown_profiled(src);
        if profile.document != *normal_doc {
            return Err("profiled parser-large document differed from normal parse".into());
        }
        let html = render_html_document(&profile.document, &HtmlOptions::default())?;
        if html != golden_html {
            return Err("profiled parser-large rendered HTML differed from golden HTML".into());
        }
        for stage in profile.stages {
            let bucket = buckets.entry(stage.stage).or_default();
            bucket.samples.push(stage.elapsed_ns);
            bucket.work_count = bucket.work_count.saturating_add(stage.count);
            bucket.max_bytes = bucket.max_bytes.max(stage.bytes);
            bucket.total_bytes = bucket.total_bytes.saturating_add(stage.bytes);
            bucket.allocation_count = bucket.allocation_count.saturating_add(stage.allocations);
            bucket.total_ns = bucket.total_ns.saturating_add(stage.elapsed_ns);
            bucket.notes = stage.notes;
        }
    }

    let stage_path = dir.join("parser-large-stages.jsonl");
    let ranked = write_stage_summary_jsonl(
        StageJsonlSpec {
            path: &stage_path,
            scenario: "parser-large",
            category: "parse",
            input_bytes: src.len(),
            iterations,
            notes: "stage attribution for generated 1 MiB CommonMark/GFM-like document",
            proof: Some(StageProof {
                bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.1",
                obligation: "profiled_parse_equals_normal_parse_and_html",
                evidence_path: "golden/parser-large.html",
                notes: format!(
                    "{} profiled parse iteration(s) matched normal AST and rendered HTML exactly; output_bytes={}",
                    iterations,
                    golden_html.len()
                ),
            }),
        },
        &mut buckets,
    )?;

    let spanned = parse_markdown_spanned_profiled(src);
    if spanned.document.to_document() != *normal_doc {
        return Err("profiled parser-large spanned document differed from normal parse".into());
    }
    let mut spanned_buckets: BTreeMap<&'static str, StageBucket> = BTreeMap::new();
    for stage in spanned.stages {
        let bucket = spanned_buckets.entry(stage.stage).or_default();
        bucket.samples.push(stage.elapsed_ns);
        bucket.work_count = bucket.work_count.saturating_add(stage.count);
        bucket.max_bytes = bucket.max_bytes.max(stage.bytes);
        bucket.total_bytes = bucket.total_bytes.saturating_add(stage.bytes);
        bucket.allocation_count = bucket.allocation_count.saturating_add(stage.allocations);
        bucket.total_ns = bucket.total_ns.saturating_add(stage.elapsed_ns);
        bucket.notes = stage.notes;
    }
    let spanned_stage_path = dir.join("parser-large-spanned-stages.jsonl");
    let _ = write_stage_summary_jsonl(
        StageJsonlSpec {
            path: &spanned_stage_path,
            scenario: "parser-large-spanned",
            category: "parse-spans-diagnostics",
            input_bytes: src.len(),
            iterations: 1,
            notes: "stage attribution for spanned parser diagnostics path on generated parser-large source",
            proof: Some(StageProof {
                bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.1",
                obligation: "profiled_spanned_parse_equals_normal_parse",
                evidence_path: "parser-large-spanned-stages.jsonl",
                notes: String::from(
                    "profiled spanned parse converted back to the normal AST exactly",
                ),
            }),
        },
        &mut spanned_buckets,
    )?;

    let (top_stage, top_p95, top_total, top_allocations) = ranked
        .iter()
        .copied()
        .next()
        .unwrap_or(("unknown", 0, 0, 0));
    let recommended = recommended_parser_perf_bead(top_stage, top_allocations);
    fs::write(
        dir.join("parser-large-recommendation.jsonl"),
        format!(
            "{{\"type\":\"next_target_recommendation\",\"recommended_bead_id\":\"{}\",\"reason\":\"{}\",\"evidence_path\":\"{}\",\"confidence\":\"{}\",\"top_stage\":\"{}\",\"top_stage_p95_ns\":{},\"top_stage_total_ns\":{},\"top_stage_allocations\":{}}}\n",
            recommended.bead_id,
            json_escape(recommended.reason),
            "golden/parser-large-stages.jsonl",
            recommended.confidence,
            top_stage,
            top_p95,
            top_total,
            top_allocations,
        ),
    )?;
    Ok(())
}

struct StageJsonlSpec<'a> {
    path: &'a Path,
    scenario: &'a str,
    category: &'a str,
    input_bytes: usize,
    iterations: usize,
    notes: &'a str,
    proof: Option<StageProof<'a>>,
}

struct StageProof<'a> {
    bead_id: &'a str,
    obligation: &'a str,
    evidence_path: &'a str,
    notes: String,
}

fn write_stage_summary_jsonl(
    spec: StageJsonlSpec<'_>,
    buckets: &mut BTreeMap<&'static str, StageBucket>,
) -> io::Result<Vec<(&'static str, u128, u128, usize)>> {
    let mut jsonl = String::new();
    jsonl.push_str(&format!(
        "{{\"type\":\"scenario_start\",\"scenario\":\"{}\",\"category\":\"{}\",\"input_bytes\":{},\"iterations\":{},\"notes\":\"{}\"}}\n",
        spec.scenario,
        spec.category,
        spec.input_bytes,
        spec.iterations,
        json_escape(spec.notes),
    ));

    let mut ranked = Vec::new();
    for (stage, bucket) in buckets {
        bucket.samples.sort_unstable();
        let p50 = percentile_ns_u128(&bucket.samples, 50);
        let p95 = percentile_ns_u128(&bucket.samples, 95);
        let p99 = percentile_ns_u128(&bucket.samples, 99);
        let max = bucket.samples.last().copied().unwrap_or(0);
        ranked.push((*stage, p95, bucket.total_ns, bucket.allocation_count));
        jsonl.push_str(&format!(
            "{{\"type\":\"stage_summary\",\"scenario\":\"{}\",\"stage\":\"{}\",\"count\":{},\"work_count\":{},\"allocation_count\":{},\"total_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{},\"max_ns\":{},\"input_bytes\":{},\"total_input_bytes\":{},\"unit\":\"ns\",\"notes\":\"{}\"}}\n",
            spec.scenario,
            stage,
            bucket.samples.len(),
            bucket.work_count,
            bucket.allocation_count,
            bucket.total_ns,
            p50,
            p95,
            p99,
            max,
            bucket.max_bytes,
            bucket.total_bytes,
            json_escape(bucket.notes),
        ));
    }
    if let Some(proof) = spec.proof {
        jsonl.push_str(&format!(
            "{{\"type\":\"proof_obligation\",\"bead_id\":\"{}\",\"obligation\":\"{}\",\"status\":\"pass\",\"evidence_path\":\"{}\",\"notes\":\"{}\"}}\n",
            proof.bead_id,
            proof.obligation,
            proof.evidence_path,
            json_escape(&proof.notes),
        ));
    }
    fs::write(spec.path, jsonl)?;
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));
    Ok(ranked)
}

struct PdfPerfRecommendation {
    bead_id: &'static str,
    reason: &'static str,
    confidence: &'static str,
}

fn recommended_parser_perf_bead(stage: &str, allocations: usize) -> PdfPerfRecommendation {
    match stage {
        "line_split" | "reference_collection" | "block_parse_total" => PdfPerfRecommendation {
            bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.3",
            reason: "line/reference/block classification dominates; implement scalar special-byte classification scan after fixture gate",
            confidence: "medium",
        },
        "inline_parse" | "paragraph_block" | "heading_block" | "setext_heading_block" => {
            let confidence = if allocations > 0 { "high" } else { "medium" };
            PdfPerfRecommendation {
                bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.4",
                reason: "inline or paragraph parsing dominates with parser-owned object churn; reduce allocation churn with borrowed spans",
                confidence,
            }
        }
        "table_block" | "list_block" | "fenced_code_block" | "blockquote_block" => {
            PdfPerfRecommendation {
                bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.2",
                reason: "semantic block parser dominates; expand scanner-sensitive differential fixtures before rewriting behavior-sensitive code",
                confidence: "medium",
            }
        }
        _ => PdfPerfRecommendation {
            bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.2",
            reason: "parser attribution did not map cleanly; add fixture corpus before parser rewrites",
            confidence: "low",
        },
    }
}

fn recommended_pdf_perf_bead(stage: &str) -> PdfPerfRecommendation {
    match stage {
        "glyph_collection_and_shaping" => PdfPerfRecommendation {
            bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.4",
            reason: "glyph collection and shaping dominates; cache shaped segment glyph streams within one render",
            confidence: "high",
        },
        "font_subsetting" => PdfPerfRecommendation {
            bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.5",
            reason: "font subsetting dominates; compact deterministic subset map lookups and seed handling",
            confidence: "high",
        },
        "tounicode_generation" | "widths_array_generation" => PdfPerfRecommendation {
            bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.6",
            reason: "font metadata generation dominates; optimize ToUnicode and width-table generation",
            confidence: "high",
        },
        "page_stream_compression" | "font_stream_compression" | "xref_trailer_writing" => {
            PdfPerfRecommendation {
                bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.2",
                reason: "PDF serialization/compression dominates; implement deterministic append-only PDF writers",
                confidence: "medium",
            }
        }
        "layout" | "pagination" | "page_content_stream_generation" => PdfPerfRecommendation {
            bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.3",
            reason: "layout or page-content assembly dominates; pre-size buffers and introduce render scratch",
            confidence: "medium",
        },
        _ => PdfPerfRecommendation {
            bead_id: "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.2",
            reason: "stage attribution did not map cleanly; start with deterministic append-only PDF writers",
            confidence: "low",
        },
    }
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

fn percentile_ns_u128(durations: &[u128], percentile: usize) -> u128 {
    if durations.is_empty() {
        return 0;
    }
    let p = percentile.min(100);
    let idx = ((durations.len() - 1) * p).div_ceil(100);
    durations[idx]
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
        section += 1;
    }
    out.truncate(target);
    out
}

fn generated_pdf_large() -> String {
    let mut out = String::with_capacity(220_000);
    out.push_str("# fmd large PDF perf document\n\n");
    for i in 0..420 {
        out.push_str("## Section ");
        out.push_str(&i.to_string());
        out.push_str("\n\n");
        out.push_str("Typography performance depends on repeated shaping, kerning, ligature formation, table layout, code rendering, and deterministic PDF object serialization. ");
        out.push_str("This paragraph intentionally repeats documentation words such as optimization, deterministic, representation, typography, hyphenation, and pagination so the text pipeline has real work to do.\n\n");
        if i % 7 == 0 {
            out.push_str("| Item | Status | Notes |\n|---|:---:|---:|\n| parser | ready | 10 |\n| layout | building | 20 |\n\n");
        }
        if i % 11 == 0 {
            out.push_str("```rust\nfn hot_path(input: &str) -> usize { input.len() }\n```\n\n");
        }
    }
    out
}

fn generated_words(count: usize) -> String {
    let base = [
        "deterministic",
        "typography",
        "optimization",
        "representation",
        "hyphenation",
        "pagination",
        "markdown",
        "rendering",
        "ligature",
        "kerning",
        "paragraph",
        "document",
    ];
    let mut out = String::new();
    for i in 0..count {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(base[i % base.len()]);
    }
    out
}

fn generated_hyphen_words(count: usize) -> Vec<String> {
    let base = [
        "deterministic",
        "documentation",
        "hyphenation",
        "implementation",
        "optimization",
        "pagination",
        "representation",
        "serialization",
        "typography",
        "visualization",
        "configuration",
        "internationalization",
    ];
    (0..count)
        .map(|i| base[i % base.len()].to_string())
        .collect()
}

fn unique_chars(src: &str) -> Vec<char> {
    let mut chars: Vec<char> = src
        .chars()
        .filter(|ch| !ch.is_control())
        .chain(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789- .,;:()[]{}#`*/_"
                .chars(),
        )
        .collect();
    chars.sort_unstable();
    chars.dedup();
    chars
}

fn line_break_ledger(breaks: &[franken_markdown::layout::LineBreak]) -> String {
    let mut out = String::new();
    for (idx, br) in breaks.iter().enumerate() {
        out.push_str(&idx.to_string());
        out.push('\t');
        out.push_str(&br.start.to_string());
        out.push('\t');
        out.push_str(&br.end.to_string());
        out.push('\t');
        out.push_str(&br.next.to_string());
        out.push('\t');
        out.push_str(&br.natural_width.milli_points().to_string());
        out.push('\t');
        out.push_str(&br.badness.to_string());
        out.push('\t');
        out.push_str(&br.demerits.to_string());
        out.push('\n');
    }
    out
}

fn hyphen_ledger(hyphenator: &Hyphenator, words: &[String], opts: HyphenationOptions) -> String {
    let mut out = String::new();
    for word in words {
        out.push_str(word);
        out.push('\t');
        let points = hyphenator.hyphenation_points(word, opts);
        for (idx, point) in points.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            out.push_str(&point.to_string());
        }
        out.push('\n');
    }
    out
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
