//! Native image fetching shared by ordinary renders and CLI batch jobs.
//! The render core receives bytes only. Every direct downloader child is reaped
//! before returning, including on cancellation, byte-limit failure and the
//! wall-clock deadline. On Unix, interruptible output handles also prevent a
//! descendant's inherited stdout/stderr from extending that deadline.

use std::io::{ErrorKind, Read};
#[cfg(unix)]
use std::os::{fd::OwnedFd, unix::net::UnixStream};
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(not(unix))]
use std::sync::mpsc;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_ERROR_BYTES: usize = 4096;

pub(crate) enum FetchError {
    Cancelled,
    Failed(String),
}

#[derive(Debug)]
enum FetchSpawn {
    NotFound,
    Cancelled,
    Failed(String),
}

/// Only literal HTTP(S) URLs authorize a fetch. Files, data URLs and other
/// schemes remain with their existing resource resolvers.
pub(crate) fn remote_image_url(destination: &str) -> Option<&str> {
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

pub(crate) fn fetch_remote_image(
    url: &str,
    max_bytes: u64,
    timeout_secs: u64,
) -> Result<Vec<u8>, String> {
    fetch_remote_image_with_cancel(url, max_bytes, timeout_secs, &|| false).map_err(|error| {
        match error {
            FetchError::Cancelled => "remote image fetch cancelled".to_string(),
            FetchError::Failed(message) => message,
        }
    })
}

/// The caller retains cancellation ownership; the fetch polls it while pipes
/// are drained. A cancelled fetch never becomes a missing-image fallback.
pub(crate) fn fetch_remote_image_with_cancel(
    url: &str,
    max_bytes: u64,
    timeout_secs: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, FetchError> {
    let started = Instant::now();
    let deadline = Duration::from_secs(timeout_secs.max(1));
    let timeout = timeout_secs.max(1).to_string();
    let user_agent = format!("fmd/{}", crate::VERSION);
    let max_bytes_s = max_bytes.to_string();
    let curl_args = curl_remote_image_args(url, &timeout, &max_bytes_s, &user_agent);
    let attempt = |tool, args: &[&str]| {
        run_capped_fetch(tool, args, url, max_bytes, started, deadline, cancelled)
    };
    match attempt("curl", &curl_args) {
        Ok(bytes) => return Ok(bytes),
        Err(FetchSpawn::NotFound) => {}
        Err(FetchSpawn::Cancelled) => return Err(FetchError::Cancelled),
        Err(FetchSpawn::Failed(reason)) => return Err(FetchError::Failed(reason)),
    }

    let timeout_flag = format!("--timeout={timeout}");
    let ua_flag = format!("--user-agent={user_agent}");
    let wget_args = wget_remote_image_args(url, &timeout_flag, &ua_flag);
    match attempt("wget", &wget_args) {
        Ok(bytes) => Ok(bytes),
        Err(FetchSpawn::NotFound) => Err(FetchError::Failed(
            "neither curl nor wget is available; pass --pdf-image 'URL=PATH' or use \
             --no-remote-images"
                .to_string(),
        )),
        Err(FetchSpawn::Cancelled) => Err(FetchError::Cancelled),
        Err(FetchSpawn::Failed(reason)) => Err(FetchError::Failed(reason)),
    }
}

/// Curl only suppresses its ambient config when `--disable` is the first
/// argument. Keep transport settings under this adapter's explicit policy;
/// personal config can otherwise enable helpers that outlive the downloader.
pub(super) fn curl_remote_image_args<'a>(
    url: &'a str,
    timeout: &'a str,
    max_bytes: &'a str,
    user_agent: &'a str,
) -> [&'a str; 21] {
    [
        "--disable",
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
        timeout,
        "--max-filesize",
        max_bytes,
        "--user-agent",
        user_agent,
        "--output",
        "-",
        "--",
        url,
    ]
}

/// Retain ordinary-render redirect policy. GNU wget has no `--proto` jail;
/// HTTPS requests use `--https-only`, while HTTP requests retain redirects.
pub(super) fn wget_remote_image_args<'a>(
    url: &'a str,
    timeout_flag: &'a str,
    user_agent_flag: &'a str,
) -> Vec<&'a str> {
    let mut args = vec![
        "--no-config",
        "--quiet",
        "--tries=1",
        timeout_flag,
        "--max-redirect=5",
        user_agent_flag,
        "--output-document=-",
    ];
    if url
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        args.push("--https-only");
    }
    args.push("--");
    args.push(url);
    args
}

struct FetchChild {
    child: Child,
    reaped: bool,
    #[cfg(unix)]
    stdout: UnixStream,
    #[cfg(unix)]
    stderr: UnixStream,
}

impl FetchChild {
    fn stop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            self.reaped = self.child.wait().is_ok();
        }
    }
}

impl Drop for FetchChild {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Anonymous local sockets let Unix/macOS output reads be nonblocking without
/// unsafe code or platform dependencies. Only the parent's endpoint is set to
/// nonblocking mode; curl/wget retain ordinary blocking stdout/stderr writes.
#[cfg(unix)]
fn fetch_output_pipe() -> std::io::Result<(UnixStream, UnixStream)> {
    let (reader, writer) = UnixStream::pair()?;
    reader.set_nonblocking(true)?;
    Ok((reader, writer))
}

#[cfg(unix)]
fn run_capped_fetch(
    tool: &str,
    args: &[&str],
    url: &str,
    max_bytes: u64,
    started: Instant,
    deadline: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, FetchSpawn> {
    if cancelled() {
        return Err(FetchSpawn::Cancelled);
    }
    let (stdout, stdout_writer) = fetch_output_pipe()
        .map_err(|error| FetchSpawn::Failed(format!("starting {tool} output pipe: {error}")))?;
    let (stderr, stderr_writer) = fetch_output_pipe()
        .map_err(|error| FetchSpawn::Failed(format!("starting {tool} diagnostic pipe: {error}")))?;
    // Drop the Command immediately after spawning: retaining its writer handles
    // would prevent EOF even after the downloader closes its own copies.
    let child = match Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(stdout_writer)))
        .stderr(Stdio::from(OwnedFd::from(stderr_writer)))
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(FetchSpawn::NotFound),
        Err(error) => return Err(FetchSpawn::Failed(format!("running {tool}: {error}"))),
    };
    let mut process = FetchChild {
        child,
        reaped: false,
        stdout,
        stderr,
    };
    collect_fetch_output(
        &mut process,
        tool,
        url,
        max_bytes,
        started,
        deadline,
        cancelled,
    )
}

#[cfg(unix)]
fn collect_fetch_output(
    process: &mut FetchChild,
    tool: &str,
    url: &str,
    max_bytes: u64,
    started: Instant,
    deadline: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, FetchSpawn> {
    let mut bytes = Vec::new();
    let mut errors = Vec::new();
    let mut stdout_closed = false;
    let mut status = None;
    let mut chunk = [0u8; 8192];
    let result = loop {
        if cancelled() {
            break Err(FetchSpawn::Cancelled);
        }
        if started.elapsed() >= deadline {
            break Err(FetchSpawn::Failed(format!(
                "remote image fetch exceeded the {} second timeout",
                deadline.as_secs()
            )));
        }

        // Read at most one chunk per stream before checking cancellation and
        // the deadline again. Continuous body/diagnostic output cannot starve
        // either check, and draining stderr still prevents downloader stalls.
        let mut progress = false;
        if !stdout_closed {
            let remaining = max_bytes.saturating_sub(bytes.len() as u64);
            let capacity = usize::try_from(remaining.saturating_add(1))
                .unwrap_or(usize::MAX)
                .min(chunk.len());
            match process.stdout.read(&mut chunk[..capacity]) {
                Ok(0) => {
                    stdout_closed = true;
                    progress = true;
                }
                Ok(count) => {
                    bytes.extend_from_slice(&chunk[..count]);
                    if bytes.len() as u64 > max_bytes {
                        break Err(FetchSpawn::Failed(
                            super::size_limit_error(
                                &format!("remote image {url}"),
                                bytes.len() as u64,
                                max_bytes,
                                "--max-pdf-image-bytes",
                            )
                            .to_string(),
                        ));
                    }
                    progress = true;
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(error) => {
                    break Err(FetchSpawn::Failed(format!(
                        "reading {tool} output: {error}"
                    )));
                }
            }
        }
        let diagnostics_pending = match process.stderr.read(&mut chunk) {
            Ok(0) => false,
            Ok(count) => {
                let retained = count.min(MAX_ERROR_BYTES.saturating_sub(errors.len()));
                errors.extend_from_slice(&chunk[..retained]);
                progress = true;
                true
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => false,
            Err(error) if error.kind() == ErrorKind::Interrupted => true,
            Err(error) => {
                break Err(FetchSpawn::Failed(format!(
                    "reading {tool} diagnostics: {error}"
                )));
            }
        };

        if status.is_none() {
            match process.child.try_wait() {
                Ok(Some(exit_status)) => {
                    process.reaped = true;
                    status = Some(exit_status);
                    // Poll once more after observing exit, so diagnostics
                    // written between the read and try_wait are retained.
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    break Err(FetchSpawn::Failed(format!("waiting for {tool}: {error}")));
                }
            }
        }
        if stdout_closed
            && (!diagnostics_pending || errors.len() == MAX_ERROR_BYTES)
            && let Some(status) = status
        {
            // Once the body is complete and the downloader has exited, keep
            // the available diagnostic prefix without waiting for EOF from
            // an unrelated descendant that inherited only stderr.
            break Ok(status);
        }
        if !progress {
            std::thread::sleep(POLL_INTERVAL.min(deadline.saturating_sub(started.elapsed())));
        }
    };
    if result.is_err() {
        process.stop();
    }
    finish_fetch(tool, bytes, result?, &errors)
}

// Other native platforms retain the scoped-pipe adapter. The nonblocking local
// socket adapter above is Unix-specific; it does not claim process-tree cleanup
// or interruptible Windows pipe support.
#[cfg(not(unix))]
fn run_capped_fetch(
    tool: &str,
    args: &[&str],
    url: &str,
    max_bytes: u64,
    started: Instant,
    deadline: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, FetchSpawn> {
    if cancelled() {
        return Err(FetchSpawn::Cancelled);
    }
    let child = match Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(FetchSpawn::NotFound),
        Err(error) => return Err(FetchSpawn::Failed(format!("running {tool}: {error}"))),
    };

    std::thread::scope(|scope| {
        // This guard lives inside the scope: unwinding also closes the child
        // before the scoped readers are joined.
        let mut process = FetchChild {
            child,
            reaped: false,
        };
        let stdout = process
            .child
            .stdout
            .take()
            .ok_or_else(|| FetchSpawn::Failed(format!("{tool} produced no stdout pipe")))?;
        let stderr = process
            .child
            .stderr
            .take()
            .ok_or_else(|| FetchSpawn::Failed(format!("{tool} produced no stderr pipe")))?;
        let label = format!("remote image {url}");
        let (send, receive) = mpsc::channel();
        std::thread::Builder::new()
            .spawn_scoped(scope, move || {
                let result = super::read_limited_with_flag(
                    stdout,
                    max_bytes,
                    &label,
                    "--max-pdf-image-bytes",
                );
                let _ = send.send(result);
            })
            .map_err(|error| {
                FetchSpawn::Failed(format!("starting {tool} output reader: {error}"))
            })?;
        let errors = std::thread::Builder::new()
            .spawn_scoped(scope, move || {
                let mut stderr = stderr;
                let mut bytes = Vec::new();
                let _ = stderr
                    .by_ref()
                    .take(MAX_ERROR_BYTES as u64)
                    .read_to_end(&mut bytes);
                // Drain any remaining diagnostics without retaining them, so a
                // full stderr pipe cannot block the child or evade its deadline.
                let _ = std::io::copy(&mut stderr, &mut std::io::sink());
                bytes
            })
            .map_err(|error| {
                FetchSpawn::Failed(format!("starting {tool} diagnostic reader: {error}"))
            })?;
        let mut body = None;
        let result = loop {
            if cancelled() {
                break Err(FetchSpawn::Cancelled);
            }
            if started.elapsed() >= deadline {
                break Err(FetchSpawn::Failed(format!(
                    "remote image fetch exceeded the {} second timeout",
                    deadline.as_secs()
                )));
            }
            if body.is_none() {
                match receive.recv_timeout(POLL_INTERVAL) {
                    Ok(Ok(bytes)) => body = Some(bytes),
                    Ok(Err(error)) => break Err(FetchSpawn::Failed(error.to_string())),
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break Err(FetchSpawn::Failed(format!("reading {tool} output failed")));
                    }
                }
            }
            match process.child.try_wait() {
                Ok(Some(status)) => {
                    process.reaped = true;
                    break Ok((body.take().unwrap_or_default(), status));
                }
                Ok(None) => std::thread::sleep(POLL_INTERVAL),
                Err(error) => {
                    break Err(FetchSpawn::Failed(format!("waiting for {tool}: {error}")));
                }
            }
        };
        if result.is_err() {
            process.stop();
        }
        let stderr = errors.join().unwrap_or_default();
        let (bytes, status) = result?;
        finish_fetch(tool, bytes, status, &stderr)
    })
}

fn finish_fetch(
    tool: &str,
    bytes: Vec<u8>,
    status: ExitStatus,
    stderr: &[u8],
) -> Result<Vec<u8>, FetchSpawn> {
    if !status.success() {
        let detail = String::from_utf8_lossy(stderr)
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

pub(crate) fn warning_message(url: &str, reason: &str) -> String {
    format!(
        "fetching remote image '{url}': {reason}; image was not embedded \
         (map it with --pdf-image '{url}=PATH'; to disable fetching, omit \
         --embed-remote-images and use --no-remote-images)"
    )
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::sync::mpsc;

    /// Retain copies of the real child output handles to model inheritance by
    /// a descendant, without creating unowned grandchildren in the test suite.
    /// The holder releases on Drop, or after three seconds so a regression
    /// fails its timing assertion instead of hanging the entire test process.
    struct FetchFixture {
        process: FetchChild,
        release: mpsc::Sender<()>,
        holder: Option<std::thread::JoinHandle<()>>,
    }

    impl FetchFixture {
        fn new(script: &str, hold_stdout: bool, hold_stderr: bool) -> Self {
            let (stdout, stdout_writer) = fetch_output_pipe().unwrap();
            let (stderr, stderr_writer) = fetch_output_pipe().unwrap();
            let child = Command::new("/bin/sh")
                .args(["-c", script])
                .stdin(Stdio::null())
                .stdout(Stdio::from(OwnedFd::from(
                    stdout_writer.try_clone().unwrap(),
                )))
                .stderr(Stdio::from(OwnedFd::from(
                    stderr_writer.try_clone().unwrap(),
                )))
                .spawn()
                .unwrap();
            let process = FetchChild {
                child,
                reaped: false,
                stdout,
                stderr,
            };
            let held_outputs = (
                hold_stdout.then_some(stdout_writer),
                hold_stderr.then_some(stderr_writer),
            );
            let (release, receive) = mpsc::channel();
            let holder = std::thread::spawn(move || {
                let _ = receive.recv_timeout(Duration::from_secs(3));
                drop(held_outputs);
            });
            Self {
                process,
                release,
                holder: Some(holder),
            }
        }

        fn collect(
            &mut self,
            max_bytes: u64,
            deadline: Duration,
            cancelled: &dyn Fn() -> bool,
        ) -> Result<Vec<u8>, FetchSpawn> {
            collect_fetch_output(
                &mut self.process,
                "sh",
                "https://example.invalid/image.png",
                max_bytes,
                Instant::now(),
                deadline,
                cancelled,
            )
        }

        fn assert_reaped(&self) {
            assert!(self.process.reaped, "fetch returned with an unreaped child");
            #[cfg(target_os = "linux")]
            assert!(
                !std::path::Path::new("/proc")
                    .join(self.process.child.id().to_string())
                    .exists(),
                "downloader still exists after fetch returned"
            );
        }
    }

    impl Drop for FetchFixture {
        fn drop(&mut self) {
            let _ = self.release.send(());
            if let Some(holder) = self.holder.take() {
                let _ = holder.join();
            }
        }
    }

    #[test]
    fn held_stdout_obeys_deadline_after_downloader_exits() {
        let mut fixture = FetchFixture::new("printf image", true, true);
        let started = Instant::now();
        let result = fixture.collect(1024, Duration::from_millis(100), &|| false);
        assert!(
            matches!(result, Err(FetchSpawn::Failed(ref message)) if message.contains("timeout")),
            "{result:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        fixture.assert_reaped();
    }

    #[test]
    fn successful_exact_limit_body_does_not_wait_for_inherited_stderr() {
        let mut fixture = FetchFixture::new("printf image", false, true);
        let started = Instant::now();
        let bytes = fixture
            .collect(5, Duration::from_secs(1), &|| false)
            .unwrap();
        assert_eq!(bytes, b"image");
        assert!(started.elapsed() < Duration::from_secs(2));
        fixture.assert_reaped();
    }

    #[test]
    fn cancellation_reaps_downloader_despite_inherited_outputs_and_diagnostic_flood() {
        let mut fixture = FetchFixture::new(
            "while :; do printf 'downloader diagnostic\\n' >&2; done",
            true,
            true,
        );
        let started = Instant::now();
        let result = fixture.collect(1024, Duration::from_secs(5), &|| {
            started.elapsed() >= Duration::from_millis(50)
        });
        assert!(matches!(result, Err(FetchSpawn::Cancelled)), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(2));
        fixture.assert_reaped();
    }

    #[test]
    fn byte_limit_reaps_downloader_without_waiting_for_inherited_handles() {
        let mut fixture = FetchFixture::new("printf excess; while :; do :; done", true, true);
        let started = Instant::now();
        let result = fixture.collect(3, Duration::from_secs(5), &|| false);
        assert!(
            matches!(result, Err(FetchSpawn::Failed(ref message))
                if message.contains("is 4 bytes; exceeds --max-pdf-image-bytes 3")),
            "{result:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        fixture.assert_reaped();
    }

    #[test]
    fn diagnostics_are_drained_but_only_the_bounded_prefix_is_retained() {
        let script = format!(
            "printf body; i=0; while [ \"$i\" -lt 2048 ]; do printf '%s' '{}' >&2; \
             i=$((i + 1)); done; exit 17",
            "x".repeat(256)
        );
        let mut fixture = FetchFixture::new(&script, false, true);
        let started = Instant::now();
        let result = fixture.collect(1024, Duration::from_secs(5), &|| false);
        let expected = format!("sh: {}", "x".repeat(MAX_ERROR_BYTES));
        assert!(
            matches!(result, Err(FetchSpawn::Failed(ref message)) if message == &expected),
            "{result:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        fixture.assert_reaped();
    }
}
