//! Explicit, bounded native PDF compression scheduling.
//!
//! This module is compiled only by the opt-in `native-compression` feature.
//! The shared compressor and the default render path remain synchronous.

use super::{OwnedPdfStream, PageContent, PdfStream, own_page_stream, page_stream};
use crate::{RenderError, Result};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};

pub(super) const MIN_PARALLEL_BYTES: usize = 256 * 1024;

type WorkerResult = Result<(usize, OwnedPdfStream)>;
type Work = (usize, String);
type WorkerTask = Box<dyn FnOnce() + Send>;

struct WorkQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

#[derive(Default)]
struct QueueState {
    closed: bool,
    pending: VecDeque<Work>,
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(QueueState::default()),
            ready: Condvar::new(),
        }
    }

    fn push(&self, work: Work) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.pending.push_back(work);
        drop(state);
        self.ready.notify_one();
    }

    fn pop(&self) -> Option<Work> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(work) = state.pending.pop_front() {
                return Some(work);
            }
            if state.closed {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        drop(state);
        self.ready.notify_all();
    }
}

fn worker(queue: &WorkQueue, results: &mpsc::Sender<WorkerResult>) {
    let mut scratch = crate::compress::ZlibCompressScratch::new();
    while let Some((index, stream)) = queue.pop() {
        let compressed = own_page_stream(page_stream(&stream, &mut scratch));
        if results.send(Ok((index, compressed))).is_err() {
            return;
        }
    }
}

fn report_worker_failure(
    queue: &WorkQueue,
    sender: &mpsc::Sender<WorkerResult>,
    run: impl FnOnce(),
) {
    // Without this result, a panicked worker could lose one page while other
    // workers keep the channel open indefinitely.
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).is_err() {
        queue.close();
        let _ = sender.send(Err(RenderError::PdfGeneration(
            "native PDF compression worker panicked",
        )));
    }
}

/// At most one outstanding page per successfully spawned worker. All workers
/// are joined on success, on an error, and when a caller drops the pipeline.
/// Results are drained in source order, independent of completion order.
pub(super) struct Pipeline {
    queue: Arc<WorkQueue>,
    results: mpsc::Receiver<WorkerResult>,
    workers: Vec<JoinHandle<()>>,
    submitted: usize,
    drained: usize,
    done: BTreeMap<usize, OwnedPdfStream>,
}

impl Pipeline {
    pub(super) fn spawn(max_workers: usize, initial_page: usize) -> Option<Self> {
        Self::spawn_with(max_workers, initial_page, |task| {
            thread::Builder::new()
                .name("fmd-pdf-page-compress".to_string())
                .spawn(task)
        })
    }

    fn spawn_with(
        max_workers: usize,
        initial_page: usize,
        mut spawn: impl FnMut(WorkerTask) -> std::io::Result<JoinHandle<()>>,
    ) -> Option<Self> {
        let queue = Arc::new(WorkQueue::new());
        let (sender, results) = mpsc::channel();
        let mut workers = Vec::new();
        for _ in 0..max_workers {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            let task = Box::new(move || {
                report_worker_failure(&queue, &sender, || worker(&queue, &sender));
            });
            match spawn(task) {
                Ok(handle) => workers.push(handle),
                // A reduced pool is still correct. If all spawns fail, the
                // caller retains the page and compresses it synchronously.
                Err(_) => break,
            }
        }
        if workers.is_empty() {
            return None;
        }
        Some(Self {
            queue,
            results,
            workers,
            submitted: initial_page,
            drained: initial_page,
            done: BTreeMap::new(),
        })
    }

    pub(super) fn submit(
        &mut self,
        index: usize,
        stream: String,
        out: &mut Vec<OwnedPdfStream>,
    ) -> Result<()> {
        if index != self.submitted {
            return Err(RenderError::PdfGeneration(
                "native PDF compression pages must be submitted in source order",
            ));
        }
        if self.submitted.saturating_sub(self.drained) >= self.workers.len() {
            self.drain_next(out)?;
        }
        self.queue.push((index, stream));
        self.submitted += 1;
        Ok(())
    }

    pub(super) fn drain_ready(&mut self, out: &mut Vec<OwnedPdfStream>) -> Result<()> {
        while let Ok(result) = self.results.try_recv() {
            let (index, stream) = result?;
            self.done.insert(index, stream);
        }
        while let Some(stream) = self.done.remove(&self.drained) {
            out.push(stream);
            self.drained += 1;
        }
        Ok(())
    }

    fn drain_next(&mut self, out: &mut Vec<OwnedPdfStream>) -> Result<()> {
        loop {
            if let Some(stream) = self.done.remove(&self.drained) {
                out.push(stream);
                self.drained += 1;
                return Ok(());
            }
            let (index, stream) = self.results.recv().map_err(|_| {
                RenderError::PdfGeneration(
                    "native PDF compression workers exited before all pages completed",
                )
            })??;
            self.done.insert(index, stream);
        }
    }

    pub(super) fn finish(&mut self, out: &mut Vec<OwnedPdfStream>) -> Result<()> {
        self.queue.close();
        let drained = (|| {
            while self.submitted > self.drained {
                self.drain_next(out)?;
            }
            Ok(())
        })();
        let joined = self.join_workers();
        drained.and(joined)
    }

    fn join_workers(&mut self) -> Result<()> {
        let mut failed = false;
        for handle in self.workers.drain(..) {
            failed |= handle.join().is_err();
        }
        if failed {
            Err(RenderError::PdfGeneration(
                "native PDF compression worker panicked during shutdown",
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.queue.close();
        let _ = self.join_workers();
    }
}

fn compress_slice(pages: &[PageContent]) -> Vec<PdfStream<'_>> {
    let mut scratch = crate::compress::ZlibCompressScratch::new();
    pages
        .iter()
        .map(|page| page_stream(&page.stream, &mut scratch))
        .collect()
}

pub(super) fn compress_pages(
    pages: &[PageContent],
    max_workers: usize,
) -> Result<Vec<PdfStream<'_>>> {
    compress_pages_with_spawn_policy(pages, max_workers, true)
}

fn compress_pages_with_spawn_policy(
    pages: &[PageContent],
    max_workers: usize,
    allow_spawn: bool,
) -> Result<Vec<PdfStream<'_>>> {
    let workers = max_workers.min(pages.len()).max(1);
    let total = pages.iter().map(|page| page.stream.len()).sum::<usize>();
    if workers == 1 || total < MIN_PARALLEL_BYTES {
        return Ok(compress_slice(pages));
    }
    let chunk_size = pages.len().div_ceil(workers);
    thread::scope(|scope| {
        let mut batches = Vec::new();
        for chunk in pages.chunks(chunk_size) {
            let result = if allow_spawn {
                thread::Builder::new()
                    .name("fmd-pdf-batch-compress".to_string())
                    .spawn_scoped(scope, move || compress_slice(chunk))
            } else {
                Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
            };
            batches.push(match result {
                Ok(handle) => (Some(handle), None),
                Err(_) => (None, Some(compress_slice(chunk))),
            });
        }
        let mut output = Vec::with_capacity(pages.len());
        let mut failed = false;
        for (handle, inline) in batches {
            match handle {
                Some(handle) => match handle.join() {
                    Ok(streams) => output.extend(streams),
                    Err(_) => failed = true,
                },
                None => output.extend(inline.into_iter().flatten()),
            }
        }
        if failed {
            Err(RenderError::PdfGeneration(
                "native PDF compression worker panicked",
            ))
        } else {
            Ok(output)
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn pages() -> Vec<PageContent> {
        (0..8)
            .map(|index| {
                let mut page = super::super::empty_page_content();
                page.stream = format!("% page {index}\n")
                    + &"BT /F0 11 Tf 1 0 0 1 72 720 Tm (bounded worker) Tj ET\n".repeat(1024);
                page
            })
            .collect()
    }

    fn streams_match(actual: &[OwnedPdfStream], expected: &[PdfStream<'_>]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.bytes, expected.bytes.as_ref());
            assert_eq!(actual.decoded_len, expected.decoded_len);
            assert_eq!(actual.flate_decode, expected.flate_decode);
        }
    }

    #[test]
    fn denied_pipeline_spawn_leaves_inline_fallback_available() {
        let pool = Pipeline::spawn_with(4, 0, |_| {
            Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
        });
        assert!(pool.is_none());
    }

    #[test]
    fn reduced_pipeline_preserves_order_bounds_and_joins_workers() {
        let input = pages();
        let expected = compress_slice(&input);
        let exited = Arc::new(AtomicUsize::new(0));
        let mut attempts = 0;
        let mut pool = Pipeline::spawn_with(4, 0, |task| {
            attempts += 1;
            if attempts > 2 {
                return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
            }
            let exited = Arc::clone(&exited);
            thread::Builder::new().spawn(move || {
                task();
                exited.fetch_add(1, Ordering::SeqCst);
            })
        })
        .expect("two workers available");
        assert_eq!(pool.workers.len(), 2);
        let mut out = Vec::new();
        for (index, page) in input.iter().enumerate() {
            pool.submit(index, page.stream.clone(), &mut out).unwrap();
            assert!(pool.submitted - pool.drained <= 2);
            pool.drain_ready(&mut out).unwrap();
        }
        pool.finish(&mut out).unwrap();
        assert!(pool.workers.is_empty());
        assert_eq!(exited.load(Ordering::SeqCst), 2);
        streams_match(&out, &expected);
    }

    #[test]
    fn early_drop_joins_every_started_worker() {
        let exited = Arc::new(AtomicUsize::new(0));
        let mut pool = Pipeline::spawn_with(2, 0, |task| {
            let exited = Arc::clone(&exited);
            thread::Builder::new().spawn(move || {
                task();
                exited.fetch_add(1, Ordering::SeqCst);
            })
        })
        .unwrap();
        let mut out = Vec::new();
        pool.submit(0, "pending page".repeat(2048), &mut out)
            .unwrap();
        drop(pool);
        assert_eq!(exited.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn lost_worker_returns_error_and_drop_still_joins() {
        let exited = Arc::new(AtomicUsize::new(0));
        let mut pool = Pipeline::spawn_with(1, 0, |task| {
            drop(task);
            let exited = Arc::clone(&exited);
            thread::Builder::new().spawn(move || {
                exited.fetch_add(1, Ordering::SeqCst);
            })
        })
        .unwrap();
        let mut out = Vec::new();
        pool.submit(0, "unprocessed page".to_string(), &mut out)
            .unwrap();
        let error = pool.finish(&mut out).unwrap_err();
        assert!(error.to_string().contains("before all pages completed"));
        assert!(pool.workers.is_empty(), "finish joins even on an error");
        drop(pool);
        assert_eq!(exited.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_panicked_worker_reports_its_lost_page_with_another_worker_alive() {
        let queue = Arc::new(WorkQueue::new());
        queue.push((0, "page consumed before failure".into()));
        let (sender, results) = mpsc::channel();
        let (taken_tx, taken_rx) = mpsc::channel();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let exited = Arc::new(AtomicUsize::new(0));
        let failing = {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            let barrier = Arc::clone(&barrier);
            let exited = Arc::clone(&exited);
            thread::spawn(move || {
                report_worker_failure(&queue, &sender, || {
                    let _lost_page = queue.pop().unwrap();
                    taken_tx.send(()).unwrap();
                    barrier.wait();
                    panic!("injected compression failure");
                });
                exited.fetch_add(1, Ordering::SeqCst);
            })
        };
        taken_rx.recv().unwrap();
        let surviving = {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            let exited = Arc::clone(&exited);
            thread::spawn(move || {
                report_worker_failure(&queue, &sender, || worker(&queue, &sender));
                exited.fetch_add(1, Ordering::SeqCst);
            })
        };
        drop(sender);
        let mut pool = Pipeline {
            queue,
            results,
            workers: vec![failing, surviving],
            submitted: 1,
            drained: 0,
            done: BTreeMap::new(),
        };
        barrier.wait();
        // Exercise draining before finish closes the queue: a surviving
        // waiting worker must not keep the failed page pending forever.
        let error = pool.drain_next(&mut Vec::new()).unwrap_err();
        assert!(error.to_string().contains("worker panicked"));
        drop(pool);
        assert_eq!(exited.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn monolithic_workers_and_denied_spawns_match_sequential() {
        let input = pages();
        assert!(input.iter().map(|page| page.stream.len()).sum::<usize>() > MIN_PARALLEL_BYTES);
        let expected = compress_slice(&input);
        for workers in [1, 2, 4] {
            for allow_spawn in [false, true] {
                let output = compress_pages_with_spawn_policy(&input, workers, allow_spawn)
                    .unwrap()
                    .into_iter()
                    .map(own_page_stream)
                    .collect::<Vec<_>>();
                streams_match(&output, &expected);
            }
        }
    }
}
