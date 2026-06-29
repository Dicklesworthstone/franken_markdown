//! Native-only batch renderer (bead `zmd.1.3`).
//!
//! Renders many Markdown inputs under a bounded worker budget using Asupersync
//! for structured concurrency, cancellation, and budgets, and emits a
//! deterministic receipt. This module is compiled ONLY with the `batch` cargo
//! feature; the render core, `--no-default-features`, and wasm32 builds never
//! see it, so the engine stays dependency-free. See
//! `docs/BATCH_ORCHESTRATION.md` and `docs/BATCH_WORKER_BUDGET.md`.
//!
//! Scheduling: inputs are split round-robin into exactly `workers` shards, each
//! rendered serially by one Asupersync task on a `workers`-thread runtime. This
//! bounds concurrent renders (and therefore peak memory) to the worker budget
//! without a shared queue, and a per-file `checkpoint` makes cancellation
//! cooperative at file granularity. Receipts are assembled in deterministic
//! input order regardless of completion order.
#![cfg(feature = "batch")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use asupersync::prelude::*;
use asupersync::runtime::RuntimeBuilder;

use crate::{HtmlOptions, PdfOptions, render_html, render_pdf};

/// Conservative default per-job peak RSS estimate (bytes) for the memory
/// ceiling when the caller does not supply one.
const DEFAULT_PER_JOB_RSS: u64 = 64 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Worker-budget policy (bead zmd.1.1; docs/BATCH_WORKER_BUDGET.md)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchMode {
    Interactive,
    Throughput,
}

impl BatchMode {
    fn as_str(self) -> &'static str {
        match self {
            BatchMode::Interactive => "interactive",
            BatchMode::Throughput => "throughput",
        }
    }
}

/// Inputs to the deterministic worker-budget policy.
#[derive(Clone, Copy, Debug)]
pub struct BudgetInputs {
    /// Logical CPUs visible to the process.
    pub c_avail: usize,
    /// User worker cap (`0` = unset).
    pub c_cap: usize,
    pub mode: BatchMode,
    /// Soft memory ceiling in bytes (`0` = unbounded).
    pub mem_budget: u64,
    /// Per-job peak RSS estimate in bytes.
    pub per_job_rss: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerBudget {
    pub workers: usize,
    pub queue_depth: usize,
}

/// Deterministic, solver-free worker budget (the batch path of the policy).
pub fn worker_budget(input: &BudgetInputs) -> WorkerBudget {
    let c_avail = input.c_avail.max(1);

    // Desired concurrency from the user cap (clamped to CPUs) or all CPUs.
    let mut desired = if input.c_cap > 0 {
        input.c_cap.min(c_avail)
    } else {
        c_avail
    };
    // Interactive default reserves ~1/4 of cores so a blocking agent stays
    // responsive; an explicit cap opts out of the reservation.
    if input.mode == BatchMode::Interactive && input.c_cap == 0 {
        desired = (((c_avail as f64) * 0.75).floor() as usize).max(1);
    }

    // Memory ceiling: never run more concurrent jobs than the budget allows.
    let mem_workers = if input.mem_budget == 0 {
        usize::MAX
    } else {
        ((input.mem_budget / input.per_job_rss.max(1)).max(1)) as usize
    };

    let workers = desired.min(mem_workers).max(1);
    let queue_depth = (2 * workers).clamp(2, 4 * c_avail);
    WorkerBudget {
        workers,
        queue_depth,
    }
}

// ---------------------------------------------------------------------------
// Plan / options / receipt
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Html,
    Pdf,
    Both,
}

impl OutputFormat {
    fn as_str(self) -> &'static str {
        match self {
            OutputFormat::Html => "html",
            OutputFormat::Pdf => "pdf",
            OutputFormat::Both => "both",
        }
    }
    fn wants_html(self) -> bool {
        matches!(self, OutputFormat::Html | OutputFormat::Both)
    }
    fn wants_pdf(self) -> bool {
        matches!(self, OutputFormat::Pdf | OutputFormat::Both)
    }
}

/// A fully-expanded, deterministically-ordered batch plan.
pub struct BatchPlan {
    pub inputs: Vec<PathBuf>,
    pub format: OutputFormat,
    /// Output directory; `None` writes alongside each input.
    pub out_dir: Option<PathBuf>,
}

/// Render + scheduling options for a batch run.
pub struct BatchOptions {
    pub html: HtmlOptions,
    pub pdf: PdfOptions,
    pub mode: BatchMode,
    /// User worker cap (`None` = derive from CPUs).
    pub workers: Option<usize>,
    /// Memory ceiling in bytes (`None` = unbounded).
    pub mem_budget: Option<u64>,
    /// When true, per-file failures do not fail the whole run.
    pub continue_on_error: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Ok,
    Failed,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct OutputEntry {
    pub path: PathBuf,
    pub bytes: usize,
    /// FNV-1a-64 content fingerprint (deterministic; not cryptographic).
    pub fnv1a64: u64,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub input: PathBuf,
    pub status: FileStatus,
    pub outputs: Vec<OutputEntry>,
    pub error: Option<String>,
}

impl FileEntry {
    fn skipped(input: &Path) -> Self {
        FileEntry {
            input: input.to_path_buf(),
            status: FileStatus::Skipped,
            outputs: Vec::new(),
            error: None,
        }
    }
}

/// The deterministic outcome of a batch run. Golden fields are timing-free so
/// repeated runs over the same inputs produce byte-identical receipts.
pub struct BatchReceipt {
    pub format: OutputFormat,
    pub mode: BatchMode,
    pub workers: usize,
    pub queue_depth: usize,
    pub files: Vec<FileEntry>,
    pub cancelled: bool,
}

impl BatchReceipt {
    pub fn ok_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.status == FileStatus::Ok)
            .count()
    }
    pub fn failed_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.status == FileStatus::Failed)
            .count()
    }
    pub fn skipped_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.status == FileStatus::Skipped)
            .count()
    }

    /// Deterministic, hand-rolled JSON (no third-party serializer). Stable key
    /// order; no timestamps.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(256 + self.files.len() * 96);
        out.push_str("{\"schema\":\"fmd-batch-receipt-v1\"");
        out.push_str(&format!(",\"format\":\"{}\"", self.format.as_str()));
        out.push_str(&format!(",\"batch_mode\":\"{}\"", self.mode.as_str()));
        out.push_str(&format!(",\"workers\":{}", self.workers));
        out.push_str(&format!(",\"queue_depth\":{}", self.queue_depth));
        out.push_str(&format!(",\"inputs\":{}", self.files.len()));
        out.push_str(&format!(",\"ok\":{}", self.ok_count()));
        out.push_str(&format!(",\"failed\":{}", self.failed_count()));
        out.push_str(&format!(",\"skipped\":{}", self.skipped_count()));
        out.push_str(&format!(",\"cancelled\":{}", self.cancelled));
        out.push_str(",\"files\":[");
        for (i, f) in self.files.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let status = match f.status {
                FileStatus::Ok => "ok",
                FileStatus::Failed => "failed",
                FileStatus::Skipped => "skipped",
            };
            out.push_str("{\"input\":\"");
            json_escape_into(&mut out, &f.input.to_string_lossy());
            out.push_str(&format!("\",\"status\":\"{status}\""));
            if let Some(err) = &f.error {
                out.push_str(",\"error\":\"");
                json_escape_into(&mut out, err);
                out.push('"');
            }
            out.push_str(",\"outputs\":[");
            for (j, o) in f.outputs.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str("{\"path\":\"");
                json_escape_into(&mut out, &o.path.to_string_lossy());
                out.push_str(&format!(
                    "\",\"bytes\":{},\"fnv1a64\":\"{:016x}\"}}",
                    o.bytes, o.fnv1a64
                ));
            }
            out.push_str("]}");
        }
        out.push_str("]}");
        out
    }
}

fn json_escape_into(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ---------------------------------------------------------------------------
// Input expansion (native)
// ---------------------------------------------------------------------------

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md") | Some("markdown")
    )
}

/// Expand files, directories (recursively collecting `*.md`/`*.markdown`), into
/// a deterministically-sorted, de-duplicated input list.
pub fn expand_inputs(paths: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    for p in paths {
        let meta = std::fs::metadata(p)?;
        if meta.is_dir() {
            collect_dir(p, &mut out)?;
        } else {
            out.push(p.clone());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn collect_dir(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    // Read the directory into a sorted vector first so recursion order is
    // deterministic regardless of filesystem enumeration order.
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            collect_dir(&entry, out)?;
        } else if is_markdown(&entry) {
            out.push(entry);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-file render (synchronous core)
// ---------------------------------------------------------------------------

fn output_path(input: &Path, out_dir: Option<&Path>, ext: &str) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default();
    let mut name = stem.to_os_string();
    name.push(".");
    name.push(ext);
    match out_dir {
        Some(dir) => dir.join(name),
        None => match input.parent() {
            Some(parent) => parent.join(name),
            None => PathBuf::from(name),
        },
    }
}

fn render_one(
    input: &Path,
    format: OutputFormat,
    out_dir: Option<&Path>,
    html: &HtmlOptions,
    pdf: &PdfOptions,
) -> FileEntry {
    let md = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => return failed(input, format!("read failed: {e}")),
    };
    if let Some(dir) = out_dir
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return failed(input, format!("create out-dir failed: {e}"));
    }

    let mut outputs = Vec::new();
    if format.wants_html() {
        match render_html(&md, html) {
            Ok(doc) => {
                let path = output_path(input, out_dir, "html");
                if let Err(e) = std::fs::write(&path, doc.as_bytes()) {
                    return failed(input, format!("write html failed: {e}"));
                }
                outputs.push(OutputEntry {
                    path,
                    bytes: doc.len(),
                    fnv1a64: fnv1a64(doc.as_bytes()),
                });
            }
            Err(e) => return failed(input, format!("render html failed: {e}")),
        }
    }
    if format.wants_pdf() {
        match render_pdf(&md, pdf) {
            Ok(bytes) => {
                let path = output_path(input, out_dir, "pdf");
                if let Err(e) = std::fs::write(&path, &bytes) {
                    return failed(input, format!("write pdf failed: {e}"));
                }
                outputs.push(OutputEntry {
                    path,
                    bytes: bytes.len(),
                    fnv1a64: fnv1a64(&bytes),
                });
            }
            Err(e) => return failed(input, format!("render pdf failed: {e}")),
        }
    }

    FileEntry {
        input: input.to_path_buf(),
        status: FileStatus::Ok,
        outputs,
        error: None,
    }
}

fn failed(input: &Path, message: String) -> FileEntry {
    FileEntry {
        input: input.to_path_buf(),
        status: FileStatus::Failed,
        outputs: Vec::new(),
        error: Some(message),
    }
}

// ---------------------------------------------------------------------------
// Async orchestration
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum BatchError {
    NoContext,
}

impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchError::NoContext => write!(f, "runtime did not provide a root context"),
        }
    }
}

/// Render every input under the worker `budget`, returning a deterministic
/// receipt. `&Cx` first; the `Outcome` is preserved for the CLI boundary.
pub async fn render_batch(
    cx: &Cx,
    plan: BatchPlan,
    opts: &BatchOptions,
    budget: WorkerBudget,
) -> Outcome<BatchReceipt, BatchError> {
    // Cooperative cancellation/budget observation at the batch boundary.
    if cx.checkpoint().is_err() {
        let files = plan
            .inputs
            .iter()
            .map(|input| FileEntry::skipped(input))
            .collect();
        return Outcome::Ok(BatchReceipt {
            format: plan.format,
            mode: opts.mode,
            workers: budget.workers.max(1),
            queue_depth: budget.queue_depth,
            files,
            cancelled: true,
        });
    }

    let workers = budget.workers.max(1);
    let inputs = Arc::new(plan.inputs);
    let cancelled = Arc::new(AtomicBool::new(false));

    // Spawn through the runtime handle (the documented `block_on` spawn path);
    // on a multi-thread runtime each worker task lands on its own worker thread,
    // so the blocking synchronous renders run in parallel.
    let handle = match Runtime::current_handle() {
        Some(h) => h,
        None => return Outcome::Err(BatchError::NoContext),
    };

    let mut tasks = Vec::with_capacity(workers);
    for w in 0..workers {
        let inputs = Arc::clone(&inputs);
        let cancelled = Arc::clone(&cancelled);
        let html = opts.html.clone();
        let pdf = opts.pdf.clone();
        let out_dir = plan.out_dir.clone();
        let format = plan.format;
        tasks.push(handle.spawn(async move {
            let mut entries: Vec<(usize, FileEntry)> = Vec::new();
            let mut stop = false;
            let mut i = w;
            while i < inputs.len() {
                let cancel_now = stop
                    || cancelled.load(Ordering::Relaxed)
                    || Cx::current().is_some_and(|tcx| tcx.checkpoint().is_err());
                if cancel_now {
                    stop = true;
                    cancelled.store(true, Ordering::Relaxed);
                    entries.push((i, FileEntry::skipped(&inputs[i])));
                } else {
                    let entry = render_one(&inputs[i], format, out_dir.as_deref(), &html, &pdf);
                    entries.push((i, entry));
                }
                i += workers;
            }
            entries
        }));
    }

    // Reassemble in deterministic input order; any index not produced (a panicked
    // or cancelled shard) is recorded as skipped so every input appears.
    let mut slots: Vec<Option<FileEntry>> = (0..inputs.len()).map(|_| None).collect();
    for task in tasks {
        for (idx, entry) in task.await {
            if let Some(slot) = slots.get_mut(idx) {
                *slot = Some(entry);
            }
        }
    }
    let files: Vec<FileEntry> = slots
        .into_iter()
        .enumerate()
        .map(|(idx, slot)| slot.unwrap_or_else(|| FileEntry::skipped(&inputs[idx])))
        .collect();

    Outcome::Ok(BatchReceipt {
        format: plan.format,
        mode: opts.mode,
        workers,
        queue_depth: budget.queue_depth,
        files,
        cancelled: cancelled.load(Ordering::Relaxed),
    })
}

/// Native entry point: size a multi-thread runtime to the worker budget, drive
/// the batch, and return the receipt. Used by the `fmd batch` CLI command.
pub fn run_batch_blocking(
    plan: BatchPlan,
    opts: &BatchOptions,
) -> Result<BatchReceipt, BatchError> {
    let c_avail = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut budget = worker_budget(&BudgetInputs {
        c_avail,
        c_cap: opts.workers.unwrap_or(0),
        mode: opts.mode,
        mem_budget: opts.mem_budget.unwrap_or(0),
        per_job_rss: DEFAULT_PER_JOB_RSS,
    });
    // Never spawn more workers (or runtime threads) than there are inputs.
    budget.workers = budget.workers.min(plan.inputs.len().max(1)).max(1);

    let runtime = RuntimeBuilder::multi_thread()
        .worker_threads(budget.workers)
        .build()
        .map_err(|_| BatchError::NoContext)?;

    let outcome = runtime.block_on(async move {
        match Cx::current() {
            Some(cx) => render_batch(&cx, plan, opts, budget).await,
            None => Outcome::Err(BatchError::NoContext),
        }
    });

    match outcome {
        Outcome::Ok(receipt) => Ok(receipt),
        Outcome::Err(e) => Err(e),
        _ => Err(BatchError::NoContext),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn inputs(c_avail: usize, c_cap: usize, mode: BatchMode, mem: u64, rss: u64) -> BudgetInputs {
        BudgetInputs {
            c_avail,
            c_cap,
            mode,
            mem_budget: mem,
            per_job_rss: rss,
        }
    }

    #[test]
    fn worker_budget_matches_documented_examples() {
        // c=1 → single worker, queue floor 2.
        let b = worker_budget(&inputs(1, 0, BatchMode::Throughput, 0, DEFAULT_PER_JOB_RSS));
        assert_eq!(
            b,
            WorkerBudget {
                workers: 1,
                queue_depth: 2
            }
        );
        // throughput uses all cores.
        assert_eq!(
            worker_budget(&inputs(8, 0, BatchMode::Throughput, 0, DEFAULT_PER_JOB_RSS)).workers,
            8
        );
        // interactive default reserves ~1/4.
        assert_eq!(
            worker_budget(&inputs(
                8,
                0,
                BatchMode::Interactive,
                0,
                DEFAULT_PER_JOB_RSS
            ))
            .workers,
            6
        );
        // cap below cores respected.
        assert_eq!(
            worker_budget(&inputs(
                8,
                3,
                BatchMode::Interactive,
                0,
                DEFAULT_PER_JOB_RSS
            ))
            .workers,
            3
        );
        // cap above cores clamps to cores.
        assert_eq!(
            worker_budget(&inputs(
                4,
                16,
                BatchMode::Throughput,
                0,
                DEFAULT_PER_JOB_RSS
            ))
            .workers,
            4
        );
        // memory ceiling dominates.
        assert_eq!(
            worker_budget(&inputs(
                8,
                0,
                BatchMode::Throughput,
                2 * 1024 * 1024 * 1024,
                512 * 1024 * 1024
            ))
            .workers,
            4
        );
        // tight memory still yields at least one worker.
        assert_eq!(
            worker_budget(&inputs(
                8,
                0,
                BatchMode::Throughput,
                100 * 1024 * 1024,
                64 * 1024 * 1024
            ))
            .workers,
            1
        );
    }

    #[test]
    fn output_path_derivation() {
        assert_eq!(
            output_path(Path::new("/a/b/doc.md"), None, "html"),
            PathBuf::from("/a/b/doc.html")
        );
        assert_eq!(
            output_path(Path::new("/a/b/doc.md"), Some(Path::new("/out")), "pdf"),
            PathBuf::from("/out/doc.pdf")
        );
    }

    #[test]
    fn fnv1a64_is_deterministic_and_sensitive() {
        assert_eq!(fnv1a64(b"hello"), fnv1a64(b"hello"));
        assert_ne!(fnv1a64(b"hello"), fnv1a64(b"hellp"));
    }

    #[test]
    fn receipt_json_is_deterministic_and_well_formed() {
        let receipt = BatchReceipt {
            format: OutputFormat::Both,
            mode: BatchMode::Interactive,
            workers: 2,
            queue_depth: 4,
            files: vec![
                FileEntry {
                    input: PathBuf::from("a.md"),
                    status: FileStatus::Ok,
                    outputs: vec![OutputEntry {
                        path: PathBuf::from("a.html"),
                        bytes: 10,
                        fnv1a64: 0x1234,
                    }],
                    error: None,
                },
                FileEntry {
                    input: PathBuf::from("bad.md"),
                    status: FileStatus::Failed,
                    outputs: vec![],
                    error: Some("render html failed: x".to_string()),
                },
            ],
            cancelled: false,
        };
        let a = receipt.to_json();
        let b = receipt.to_json();
        assert_eq!(a, b);
        assert!(a.contains("\"schema\":\"fmd-batch-receipt-v1\""));
        assert!(a.contains("\"ok\":1"));
        assert!(a.contains("\"failed\":1"));
        assert!(a.contains("\"fnv1a64\":\"0000000000001234\""));
        assert_eq!(receipt.ok_count(), 1);
        assert_eq!(receipt.failed_count(), 1);
    }

    #[test]
    fn render_batch_e2e_is_parallel_and_deterministic() {
        // Unique temp dir with a handful of Markdown inputs.
        let dir = std::env::temp_dir().join(format!("fmd-batch-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixtures = [
            ("a.md", "# Alpha\n\nA short paragraph."),
            ("b.md", "- one\n- two\n  - nested"),
            ("c.md", "| H |\n|---|\n| 1 |"),
            ("d.md", "> quoted\n\n```rust\nfn main() {}\n```"),
            ("e.md", "Plain text with a [link](https://example.com)."),
        ];
        for (name, body) in fixtures {
            std::fs::write(dir.join(name), body).unwrap();
        }

        let inputs = expand_inputs(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(inputs.len(), 5, "directory expansion collected every .md");

        let out_dir = dir.join("out");
        let make_plan = || BatchPlan {
            inputs: inputs.clone(),
            format: OutputFormat::Both,
            out_dir: Some(out_dir.clone()),
        };
        let opts = BatchOptions {
            html: HtmlOptions::default(),
            pdf: PdfOptions::default(),
            mode: BatchMode::Throughput,
            workers: Some(3), // exercise the multi-thread scheduler
            mem_budget: None,
            continue_on_error: true,
        };

        let r1 = run_batch_blocking(make_plan(), &opts).unwrap();
        let r2 = run_batch_blocking(make_plan(), &opts).unwrap();

        assert_eq!(r1.ok_count(), 5);
        assert_eq!(r1.failed_count(), 0);
        assert_eq!(r1.skipped_count(), 0);
        assert!(!r1.cancelled);
        assert_eq!(r1.workers, 3, "bounded to the worker budget");

        // The receipt is timing-free, so two runs over the same inputs are
        // byte-identical regardless of worker scheduling order.
        assert_eq!(r1.to_json(), r2.to_json());

        // Files are reported in deterministic (sorted) input order.
        let order: Vec<String> = r1
            .files
            .iter()
            .filter_map(|f| {
                f.input
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .collect();
        assert_eq!(order, ["a.md", "b.md", "c.md", "d.md", "e.md"]);

        // Both outputs were actually written for each input.
        for (name, _) in fixtures {
            let stem = name.trim_end_matches(".md");
            assert!(out_dir.join(format!("{stem}.html")).exists());
            assert!(out_dir.join(format!("{stem}.pdf")).exists());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
