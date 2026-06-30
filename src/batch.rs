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
    // Saturating: this is a `pub` function, so guard against overflow on
    // pathological (near-`usize::MAX`) inputs.
    let queue_depth = workers
        .saturating_mul(2)
        .clamp(2, c_avail.saturating_mul(4));
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
        // Never follow symlinks during the recursive walk: a directory symlink
        // that forms a cycle would otherwise recurse without bound and overflow
        // the stack. (A symlink passed explicitly as a top-level input is still
        // honored by `expand_inputs`.)
        if entry.is_symlink() {
            continue;
        }
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

/// Extension-independent output identity: two inputs collide (overwrite each
/// other's outputs) exactly when this key matches, since outputs differ only by
/// extension. Used to detect silent overwrites before rendering.
fn output_key(input: &Path, out_dir: Option<&Path>) -> PathBuf {
    output_path(input, out_dir, "")
}

/// Map each input to `Some(first_index)` when an earlier input already claims its
/// output key (a flat-name collision under a shared `--out-dir`, e.g. `a/doc.md`
/// and `b/doc.md` both targeting `out/doc.*`). Deterministic in sorted input
/// order; the earliest input keeps the output and later ones are failed rather
/// than silently overwritten.
fn detect_output_collisions(inputs: &[PathBuf], out_dir: Option<&Path>) -> Vec<Option<usize>> {
    let mut seen: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();
    let mut collisions = vec![None; inputs.len()];
    for (i, input) in inputs.iter().enumerate() {
        let key = output_key(input, out_dir);
        match seen.get(&key) {
            Some(&first) => collisions[i] = Some(first),
            None => {
                seen.insert(key, i);
            }
        }
    }
    collisions
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
    let collisions = Arc::new(detect_output_collisions(
        &plan.inputs,
        plan.out_dir.as_deref(),
    ));
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
        let collisions = Arc::clone(&collisions);
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
                let entry = if cancel_now {
                    stop = true;
                    cancelled.store(true, Ordering::Relaxed);
                    FileEntry::skipped(&inputs[i])
                } else if let Some(first) = collisions[i] {
                    // Refuse to silently overwrite an earlier input's output.
                    failed(
                        &inputs[i],
                        format!(
                            "output path collides with earlier input {} (same name under --out-dir); rename or use distinct output directories",
                            inputs[first].display()
                        ),
                    )
                } else {
                    // Isolate a renderer panic to this one file (in unwind builds;
                    // release uses panic=abort, where a panic ends the process the
                    // same as the single-file render path). The engine is designed
                    // panic-free, so this is defense-in-depth.
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        render_one(&inputs[i], format, out_dir.as_deref(), &html, &pdf)
                    }))
                    .unwrap_or_else(|_| failed(&inputs[i], "render panicked".to_string()))
                };
                entries.push((i, entry));
                i += workers;
            }
            entries
        }));
    }

    // Reassemble in deterministic input order. Workers always push an entry for
    // every index they own, so the `None` fallback below is only a defensive
    // guard (e.g. against a force-cancelled shard at runtime shutdown).
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

    /// Create a fresh, unique temp directory for a test. The per-test `tag` plus
    /// the process id plus a monotonic counter keep concurrently-running tests
    /// from sharing a directory.
    fn fresh_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::AtomicU64;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fmd-batch-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Default single-worker throughput options with the bundled fonts (valid).
    fn throughput_opts() -> BatchOptions {
        BatchOptions {
            html: HtmlOptions::default(),
            pdf: PdfOptions::default(),
            mode: BatchMode::Throughput,
            workers: Some(1),
            mem_budget: None,
            continue_on_error: true,
        }
    }

    /// Markdown bytes that are not a valid TrueType font; supplying them in a
    /// font slot makes `render_html`/`render_pdf` fail during font validation.
    fn bad_font() -> crate::FontAssets {
        crate::FontAssets {
            body_regular: Some(vec![0u8, 1, 2, 3]),
            ..crate::FontAssets::default()
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

    #[test]
    fn output_collisions_are_detected_not_silently_overwritten() {
        // Pure detection: two distinct inputs whose flat output names collide
        // under one --out-dir. The earliest (sorted) keeps the output; the later
        // is flagged, never silently overwritten.
        let out = Some(Path::new("/out"));
        let inputs = vec![
            PathBuf::from("/a/doc.md"),
            PathBuf::from("/b/doc.md"),
            PathBuf::from("/a/other.md"),
        ];
        let collisions = detect_output_collisions(&inputs, out);
        assert_eq!(collisions[0], None, "first claimant keeps the output");
        assert_eq!(collisions[1], Some(0), "second collides with input 0");
        assert_eq!(collisions[2], None, "distinct stem does not collide");

        // No collisions when writing alongside each input (distinct parents).
        let alongside = detect_output_collisions(&inputs, None);
        assert!(alongside.iter().all(Option::is_none));
    }

    #[test]
    fn render_batch_cancelled_at_boundary_skips_all_and_leaks_no_output() {
        // Deterministic cancellation injection (zmd.1.4): cancelling the context
        // before `render_batch` observes its boundary checkpoint makes the
        // outcome deterministic (unlike a mid-run cancel, whose skipped set would
        // depend on scheduling). The same `checkpoint()` path also surfaces
        // runtime budget exhaustion (CancelKind::PollQuota / CostBudget), so this
        // covers the budget-refusal accounting too. We assert that every input is
        // accounted as Skipped, the receipt is marked cancelled, and — crucially
        // for "no leaks" — nothing was rendered, so no output file was written and
        // no worker region was left running (render_batch returns before spawning).
        use asupersync::prelude::{CancelKind, Cx, Outcome, RuntimeBuilder};

        let dir = std::env::temp_dir().join(format!("fmd-batch-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a.md", "b.md", "c.md"] {
            std::fs::write(dir.join(name), "# Title\n\nbody").unwrap();
        }
        let inputs = expand_inputs(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(inputs.len(), 3);
        let out_dir = dir.join("out");
        let plan = BatchPlan {
            inputs,
            format: OutputFormat::Both,
            out_dir: Some(out_dir.clone()),
        };
        let opts = BatchOptions {
            html: HtmlOptions::default(),
            pdf: PdfOptions::default(),
            mode: BatchMode::Throughput,
            workers: Some(2),
            mem_budget: None,
            continue_on_error: true,
        };
        let budget = WorkerBudget {
            workers: 2,
            queue_depth: 4,
        };

        let runtime = RuntimeBuilder::current_thread().build().unwrap();
        let receipt = runtime.block_on(async move {
            let cx = Cx::current().expect("block_on installs a root Cx");
            cx.cancel_with(CancelKind::User, Some("zmd.1.4 cancel test"));
            match render_batch(&cx, plan, &opts, budget).await {
                Outcome::Ok(r) => r,
                _ => panic!("render_batch should return Ok(receipt) even when cancelled"),
            }
        });

        assert!(
            receipt.cancelled,
            "a cancelled context must mark the receipt cancelled"
        );
        assert_eq!(
            receipt.skipped_count(),
            3,
            "every input is accounted as skipped"
        );
        assert_eq!(receipt.ok_count(), 0);
        assert_eq!(receipt.failed_count(), 0);

        // No leaks: nothing was rendered, so the output directory was never
        // created (or is empty if it pre-existed).
        let leaked = out_dir.exists()
            && std::fs::read_dir(&out_dir)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false);
        assert!(
            !leaked,
            "a cancelled-before-start batch must not write any output"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_budget_memory_and_zero_cpu_edges() {
        // Memory-constrained: floor(256 MiB / 64 MiB) = 4 dominates 8 desired.
        assert_eq!(
            worker_budget(&inputs(
                8,
                0,
                BatchMode::Throughput,
                256 * 1024 * 1024,
                64 * 1024 * 1024
            ))
            .workers,
            4
        );
        // c_avail = 0 is clamped up to one core (never zero workers).
        let z = worker_budget(&inputs(
            0,
            0,
            BatchMode::Interactive,
            0,
            DEFAULT_PER_JOB_RSS,
        ));
        assert_eq!(
            z,
            WorkerBudget {
                workers: 1,
                queue_depth: 2
            }
        );
        // An explicit cap opts out of the interactive reservation: cap below
        // cores is honored exactly even in interactive mode.
        assert_eq!(
            worker_budget(&inputs(
                16,
                5,
                BatchMode::Interactive,
                0,
                DEFAULT_PER_JOB_RSS
            ))
            .workers,
            5
        );
        // queue_depth tracks 2x workers (one in-flight + one queued per worker).
        assert_eq!(
            worker_budget(&inputs(8, 0, BatchMode::Throughput, 0, DEFAULT_PER_JOB_RSS)).queue_depth,
            16
        );
    }

    #[test]
    fn json_escape_into_covers_every_escape_path() {
        // Exercises each arm of `json_escape_into`: quote, backslash, the three
        // named whitespace escapes, a sub-0x20 control char (\uXXXX), and a
        // passthrough character.
        let mut out = String::new();
        json_escape_into(&mut out, "\"\\\n\r\t\u{0001}z");
        assert_eq!(out, "\\\"\\\\\\n\\r\\t\\u0001z");
    }

    #[test]
    fn batch_error_display_names_the_failure() {
        assert_eq!(
            format!("{}", BatchError::NoContext),
            "runtime did not provide a root context"
        );
    }

    #[test]
    fn receipt_json_covers_html_pdf_formats_and_skipped_status() {
        // Html-format receipt carrying a Skipped file: covers the "html" format
        // string and the "skipped" file-status string in `to_json`.
        let html = BatchReceipt {
            format: OutputFormat::Html,
            mode: BatchMode::Throughput,
            workers: 1,
            queue_depth: 2,
            files: vec![FileEntry::skipped(Path::new("x.md"))],
            cancelled: true,
        };
        let j = html.to_json();
        assert!(j.contains("\"format\":\"html\""));
        assert!(j.contains("\"status\":\"skipped\""));
        assert!(j.contains("\"skipped\":1"));
        assert_eq!(html.skipped_count(), 1);

        // Pdf-format receipt: covers the "pdf" format string.
        let pdf = BatchReceipt {
            format: OutputFormat::Pdf,
            mode: BatchMode::Throughput,
            workers: 1,
            queue_depth: 2,
            files: Vec::new(),
            cancelled: false,
        };
        assert!(pdf.to_json().contains("\"format\":\"pdf\""));
    }

    #[test]
    fn output_path_without_parent_yields_bare_name() {
        // The filesystem root has no parent and no file stem, so the `None`
        // parent arm builds a bare `.<ext>` name rather than panicking.
        assert_eq!(
            output_path(Path::new("/"), None, "html"),
            PathBuf::from(".html")
        );
    }

    #[test]
    fn expand_inputs_accepts_files_and_propagates_missing_paths() {
        let dir = fresh_dir("file-direct");
        let f = dir.join("solo.md");
        std::fs::write(&f, "# solo").unwrap();

        // A plain file path is accepted directly (the non-directory branch of
        // `expand_inputs`) and the same file given twice is de-duplicated.
        let got = expand_inputs(&[f.clone(), f.clone()]).unwrap();
        assert_eq!(got, vec![f.clone()]);

        // A path that does not exist surfaces the io error from `metadata`.
        let missing = dir.join("nope.md");
        assert!(expand_inputs(std::slice::from_ref(&missing)).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expand_inputs_recurses_filters_and_orders() {
        let dir = fresh_dir("expand-tree");
        std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
        // Included: .md and .markdown at multiple depths.
        std::fs::write(dir.join("b.md"), "b").unwrap();
        std::fs::write(dir.join("a.markdown"), "a").unwrap();
        std::fs::write(dir.join("sub/c.md"), "c").unwrap();
        std::fs::write(dir.join("sub/deep/d.markdown"), "d").unwrap();
        // Excluded: wrong extension and no extension.
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        std::fs::write(dir.join("README"), "ignored").unwrap();
        std::fs::write(dir.join("sub/skip.rst"), "ignored").unwrap();

        let got = expand_inputs(std::slice::from_ref(&dir)).unwrap();
        let names: Vec<String> = got
            .iter()
            .map(|p| {
                p.strip_prefix(&dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            names,
            vec!["a.markdown", "b.md", "sub/c.md", "sub/deep/d.markdown"],
            "recursion collects only markdown, in deterministic sorted order"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn expand_inputs_skips_directory_symlinks_during_walk() {
        let dir = fresh_dir("expand-symlink");
        std::fs::write(dir.join("real.md"), "x").unwrap();
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        // A directory symlink pointing back at the root would recurse without
        // bound if followed; the walk must skip it.
        std::os::unix::fs::symlink(&dir, dir.join("nested/loop")).unwrap();

        let got = expand_inputs(std::slice::from_ref(&dir)).unwrap();
        let names: Vec<String> = got
            .iter()
            .map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["real.md"],
            "the directory symlink is skipped, so the walk terminates"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_records_read_failure_for_missing_input() {
        let dir = fresh_dir("read-fail");
        let missing = dir.join("ghost.md");
        // `out_dir: None` exercises the "write alongside input" branch (no
        // create-dir step) and the read of a non-existent file fails per-file.
        let plan = BatchPlan {
            inputs: vec![missing.clone()],
            format: OutputFormat::Html,
            out_dir: None,
        };
        let r = run_batch_blocking(plan, &throughput_opts()).unwrap();
        assert_eq!(r.failed_count(), 1);
        assert_eq!(r.ok_count(), 0);
        let f = &r.files[0];
        assert_eq!(f.status, FileStatus::Failed);
        assert!(f.outputs.is_empty());
        assert!(
            f.error.as_deref().unwrap().starts_with("read failed:"),
            "got {:?}",
            f.error
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_records_create_out_dir_failure() {
        let dir = fresh_dir("outdir-fail");
        let input = dir.join("a.md");
        std::fs::write(&input, "# A").unwrap();
        // A regular file used as the output directory makes `create_dir_all` fail.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "i am a file, not a dir").unwrap();

        let plan = BatchPlan {
            inputs: vec![input.clone()],
            format: OutputFormat::Html,
            out_dir: Some(blocker.clone()),
        };
        let r = run_batch_blocking(plan, &throughput_opts()).unwrap();
        assert_eq!(r.failed_count(), 1);
        assert!(
            r.files[0]
                .error
                .as_deref()
                .unwrap()
                .starts_with("create out-dir failed:"),
            "got {:?}",
            r.files[0].error
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_records_render_html_failure_on_bad_font() {
        let dir = fresh_dir("html-render-fail");
        let input = dir.join("a.md");
        std::fs::write(&input, "# A").unwrap();

        let opts = BatchOptions {
            html: HtmlOptions {
                font_assets: bad_font(),
                ..HtmlOptions::default()
            },
            ..throughput_opts()
        };
        // Html-only format: covers the wants_html render-error arm and skips PDF.
        let plan = BatchPlan {
            inputs: vec![input.clone()],
            format: OutputFormat::Html,
            out_dir: Some(dir.join("out")),
        };
        let r = run_batch_blocking(plan, &opts).unwrap();
        assert_eq!(r.failed_count(), 1);
        assert!(
            r.files[0]
                .error
                .as_deref()
                .unwrap()
                .starts_with("render html failed:"),
            "got {:?}",
            r.files[0].error
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_records_render_pdf_failure_on_bad_font() {
        let dir = fresh_dir("pdf-render-fail");
        let input = dir.join("a.md");
        std::fs::write(&input, "# A").unwrap();

        let opts = BatchOptions {
            pdf: PdfOptions {
                font_assets: bad_font(),
                ..PdfOptions::default()
            },
            ..throughput_opts()
        };
        // Pdf-only format: covers the wants_pdf render-error arm and skips HTML.
        let plan = BatchPlan {
            inputs: vec![input.clone()],
            format: OutputFormat::Pdf,
            out_dir: Some(dir.join("out")),
        };
        let r = run_batch_blocking(plan, &opts).unwrap();
        assert_eq!(r.failed_count(), 1);
        assert!(
            r.files[0]
                .error
                .as_deref()
                .unwrap()
                .starts_with("render pdf failed:"),
            "got {:?}",
            r.files[0].error
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_records_write_html_failure_when_output_is_a_directory() {
        let dir = fresh_dir("html-write-fail");
        let input = dir.join("doc.md");
        std::fs::write(&input, "# Doc").unwrap();
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        // Pre-create the would-be output file as a directory so the write fails
        // after a successful render.
        std::fs::create_dir_all(out.join("doc.html")).unwrap();

        let plan = BatchPlan {
            inputs: vec![input.clone()],
            format: OutputFormat::Html,
            out_dir: Some(out.clone()),
        };
        let r = run_batch_blocking(plan, &throughput_opts()).unwrap();
        assert_eq!(r.failed_count(), 1);
        assert!(
            r.files[0]
                .error
                .as_deref()
                .unwrap()
                .starts_with("write html failed:"),
            "got {:?}",
            r.files[0].error
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_records_write_pdf_failure_when_output_is_a_directory() {
        let dir = fresh_dir("pdf-write-fail");
        let input = dir.join("doc.md");
        std::fs::write(&input, "# Doc").unwrap();
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(out.join("doc.pdf")).unwrap();

        let plan = BatchPlan {
            inputs: vec![input.clone()],
            format: OutputFormat::Pdf,
            out_dir: Some(out.clone()),
        };
        let r = run_batch_blocking(plan, &throughput_opts()).unwrap();
        assert_eq!(r.failed_count(), 1);
        assert!(
            r.files[0]
                .error
                .as_deref()
                .unwrap()
                .starts_with("write pdf failed:"),
            "got {:?}",
            r.files[0].error
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_fails_later_input_on_output_collision() {
        // Two inputs in distinct subdirs share a flat output name under one
        // `--out-dir`. With a single worker both indices land on the same shard,
        // so the in-worker collision-refusal arm runs: the earliest input keeps
        // the output and the later one is failed, never silently overwritten.
        let dir = fresh_dir("collision");
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::write(dir.join("a/doc.md"), "# from a").unwrap();
        std::fs::write(dir.join("b/doc.md"), "# from b").unwrap();

        let inputs = expand_inputs(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(inputs.len(), 2);
        let out = dir.join("out");
        let plan = BatchPlan {
            inputs: inputs.clone(),
            format: OutputFormat::Html,
            out_dir: Some(out.clone()),
        };
        let r = run_batch_blocking(plan, &throughput_opts()).unwrap();

        assert_eq!(r.ok_count(), 1, "earliest input keeps the output");
        assert_eq!(
            r.failed_count(),
            1,
            "later collision refused, not overwritten"
        );
        assert_eq!(r.files[0].status, FileStatus::Ok);
        assert_eq!(r.files[1].status, FileStatus::Failed);
        assert!(
            r.files[1]
                .error
                .as_deref()
                .unwrap()
                .contains("collides with earlier input"),
            "got {:?}",
            r.files[1].error
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
