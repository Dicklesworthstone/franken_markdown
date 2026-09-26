//! Real-process LSP integration tests. No mocked parser or transport.
#![cfg(feature = "lsp")]

use std::io::{Cursor, Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use franken_markdown::mcp::{JsonValue as Json, parse_json, read_frame, write_frame};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn session(messages: &[&str]) -> Result<(ExitStatus, Vec<Json>), Box<dyn std::error::Error>> {
    let mut input = Vec::new();
    for message in messages { write_frame(&mut input, message)?; }
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd-lsp"))
        .arg("--stdio").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
    let mut stdout = child.stdout.take().ok_or("missing server stdout")?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    // Fixtures are smaller than a pipe buffer; drain stdout concurrently so
    // a server response cannot deadlock the input writer.
    let mut stdin = child.stdin.take().ok_or("missing server stdin")?;
    if let Err(error) = stdin.write_all(&input) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.into());
    }
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait()? { break status; }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("language server did not terminate within the test deadline".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = reader.join().map_err(|_| "stdout reader panicked")??;
    let mut cursor = Cursor::new(output);
    let mut replies = Vec::new();
    while let Some(body) = read_frame(&mut cursor, 8 * 1024 * 1024)? {
        replies.push(parse_json(&body)?);
    }
    Ok((status, replies))
}

fn array(value: &Json) -> &[Json] {
    match value { Json::Array(values) => values, _ => &[] }
}

#[test]
fn real_editor_session_repairs_diagnostics_and_navigates_current_unicode_text() -> TestResult {
    let (status, replies) = session(&[
        r###"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"textDocument":{"documentSymbol":{"hierarchicalDocumentSymbolSupport":true},"foldingRange":{"rangeLimit":1}}}}}"###,
        r###"{"jsonrpc":"2.0","method":"initialized","params":{}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"untitled:editor","languageId":"markdown","version":1,"text":"# Root\n\nA😀B\n\n```rust\nlet x = 1;\n"}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"untitled:editor","version":2},"contentChanges":[{"range":{"start":{"line":2,"character":1},"end":{"line":2,"character":3}},"rangeLength":2,"text":"猫"},{"range":{"start":{"line":6,"character":0},"end":{"line":6,"character":0}},"text":"```\n"}]}}"###,
        r###"{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"untitled:editor"}}}"###,
        r###"{"jsonrpc":"2.0","id":3,"method":"textDocument/foldingRange","params":{"textDocument":{"uri":"untitled:editor"}}}"###,
        r###"{"jsonrpc":"2.0","id":4,"method":"textDocument/selectionRange","params":{"textDocument":{"uri":"untitled:editor"},"positions":[{"line":2,"character":2}]}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":"untitled:editor"}}}"###,
        r###"{"jsonrpc":"2.0","id":5,"method":"shutdown"}"###,
        r###"{"jsonrpc":"2.0","method":"exit"}"###,
    ])?;
    assert!(status.success());
    assert_eq!(replies.len(), 8);
    let diagnostics: Vec<_> = replies.iter()
        .filter(|reply| reply.get("method").and_then(Json::as_str) == Some("textDocument/publishDiagnostics"))
        .filter_map(|reply| reply.get("params")).collect();
    assert_eq!(diagnostics.len(), 3);
    assert!(!array(diagnostics[0].get("diagnostics").ok_or("missing initial findings")?).is_empty());
    assert_eq!(diagnostics[1].get("version").and_then(Json::as_u64), Some(2));
    assert!(array(diagnostics[1].get("diagnostics").ok_or("missing repaired findings")?).is_empty());
    assert!(array(diagnostics[2].get("diagnostics").ok_or("missing close findings")?).is_empty());
    let result = |id| replies.iter().find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id))
        .and_then(|reply| reply.get("result")).ok_or("missing request result");
    let symbols = array(result(2)?);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].get("name").and_then(Json::as_str), Some("Root"));
    assert!(symbols[0].get("children").is_some());
    assert_eq!(array(result(3)?).len(), 1);
    let selections = array(result(4)?);
    assert_eq!(selections.len(), 1);
    assert!(selections[0].get("parent").is_some());
    assert_eq!(result(5)?, &Json::Null);
    Ok(())
}

#[test]
fn real_process_rejects_exit_without_shutdown_and_abrupt_eof() -> TestResult {
    let (status, replies) = session(&[r###"{"jsonrpc":"2.0","method":"exit"}"###])?;
    assert!(!status.success());
    assert!(replies.is_empty());
    // Abrupt EOF, even after initialize, is not a clean shutdown.
    let (status, replies) = session(&[r###"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"###])?;
    assert!(!status.success());
    assert_eq!(replies.len(), 1);
    Ok(())
}
