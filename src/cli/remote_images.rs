//! Native image fetching shared by ordinary renders and CLI batch jobs.
//! The render core receives bytes only. Every child is reaped before returning,
//! including on cancellation, byte-limit failure and the wall-clock deadline.

use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_ERROR_BYTES: u64 = 4096;

pub(crate) enum FetchError {
    Cancelled,
    Failed(String),
}

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

pub(super) fn fetch_remote_image(
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
                    .take(MAX_ERROR_BYTES)
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
    })
}

pub(crate) fn warning_message(url: &str, reason: &str) -> String {
    format!(
        "fetching remote image '{url}': {reason}; rendered as alt text \
         (map it with --pdf-image '{url}=PATH' or silence fetching with --no-remote-images)"
    )
}
