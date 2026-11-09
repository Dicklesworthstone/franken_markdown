//! Real-process LSP integration tests. No mocked parser or transport.
#![cfg(feature = "lsp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Cursor, Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use franken_markdown::mcp::{JsonValue as Json, parse_json, read_frame, write_frame};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn session(messages: &[&str]) -> Result<(ExitStatus, Vec<Json>), Box<dyn std::error::Error>> {
    let mut input = Vec::new();
    for message in messages {
        write_frame(&mut input, message)?;
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmd-lsp"))
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
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
        if let Some(status) = child.try_wait()? {
            break status;
        }
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
    match value {
        Json::Array(values) => values,
        _ => &[],
    }
}

#[test]
fn real_file_rename_returns_old_uri_versioned_edits_and_reads_current_revisions() -> TestResult {
    let (status, replies) = session(&[
        r###"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"workspace":{"workspaceEdit":{"documentChanges":true},"fileOperations":{"willRename":true}}}}}"###,
        r###"{"jsonrpc":"2.0","method":"initialized","params":{}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///docs/README.md","languageId":"markdown","version":7,"text":"😀 [go](chapter.md#part)\r\n"}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///docs/chapter.md","languageId":"markdown","version":4,"text":"# Part\n\n![art](image.png)\n"}}}"###,
        r###"{"jsonrpc":"2.0","id":2,"method":"workspace/willRenameFiles","params":{"files":[{"oldUri":"file:///docs/chapter.md","newUri":"file:///manual/renamed.md"}]}}"###,
        r###"{"jsonrpc":"2.0","id":3,"method":"workspace/willRenameFiles","params":{"files":[{"oldUri":"file:///docs/chapter.md","newUri":"file:///manual/renamed.md"}]}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///docs/README.md","version":8},"contentChanges":[{"text":"😀 [go](chapter.md#next)\r\n"}]}}"###,
        r###"{"jsonrpc":"2.0","id":4,"method":"workspace/willRenameFiles","params":{"files":[{"oldUri":"file:///docs/chapter.md","newUri":"file:///manual/renamed.md"}]}}"###,
        r###"{"jsonrpc":"2.0","id":5,"method":"shutdown"}"###,
        r###"{"jsonrpc":"2.0","method":"exit"}"###,
    ])?;
    assert!(status.success());
    let result = |id| {
        replies
            .iter()
            .find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id))
            .and_then(|reply| reply.get("result"))
            .ok_or("missing result")
    };
    let operation = result(1)?
        .get("capabilities")
        .and_then(|caps| caps.get("workspace"))
        .and_then(|workspace| workspace.get("fileOperations"))
        .and_then(|operations| operations.get("willRename"))
        .ok_or("missing file operation capability")?;
    let filters = array(operation.get("filters").ok_or("missing filters")?);
    assert_eq!(filters.len(), 1);
    assert_eq!(
        filters[0].get("scheme").and_then(Json::as_str),
        Some("file")
    );
    assert_eq!(
        filters[0]
            .get("pattern")
            .and_then(|pattern| pattern.get("glob"))
            .and_then(Json::as_str),
        Some("**/*")
    );
    assert_eq!(
        result(2)?,
        result(3)?,
        "planning a move must not relocate or edit buffers"
    );
    for (id, version, fragment) in [(2, 7, "part"), (4, 8, "next")] {
        let changes = array(
            result(id)?
                .get("documentChanges")
                .ok_or("missing changes")?,
        );
        assert_eq!(changes.len(), 2);
        for (uri, version, line, start, end, replacement) in [
            (
                "file:///docs/README.md",
                version,
                0,
                8,
                23,
                format!("../manual/renamed.md#{fragment}"),
            ),
            (
                "file:///docs/chapter.md",
                4,
                2,
                7,
                16,
                "../docs/image.png".to_string(),
            ),
        ] {
            let change = changes
                .iter()
                .find(|change| {
                    change
                        .get("textDocument")
                        .and_then(|doc| doc.get("uri"))
                        .and_then(Json::as_str)
                        == Some(uri)
                })
                .ok_or("missing old URI")?;
            assert_eq!(
                change
                    .get("textDocument")
                    .and_then(|doc| doc.get("version"))
                    .and_then(Json::as_u64),
                Some(version)
            );
            let edits = array(change.get("edits").ok_or("missing edits")?);
            assert_eq!(edits.len(), 1);
            assert_eq!(
                edits[0].get("newText").and_then(Json::as_str),
                Some(replacement.as_str())
            );
            let range = edits[0].get("range").ok_or("missing range")?;
            for (side, column) in [("start", start), ("end", end)] {
                let position = range.get(side).ok_or("missing position")?;
                assert_eq!(position.get("line").and_then(Json::as_u64), Some(line));
                assert_eq!(
                    position.get("character").and_then(Json::as_u64),
                    Some(column)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn real_label_completion_returns_exact_edits_and_reads_live_document_revisions() -> TestResult {
    let (status, replies) = session(&[
        r###"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"###,
        r###"{"jsonrpc":"2.0","method":"initialized","params":{}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"untitled:labels","languageId":"markdown","version":7,"text":"😀 Claim [^remainder]\r\n\r\n[^research]: Evidence.\r\n\r\n[Guide][ma]\r\n\r\n[manual]: /guide\r\n"}}}"###,
        r###"{"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"untitled:labels"},"position":{"line":0,"character":13}}}"###,
        r###"{"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":"untitled:labels"},"position":{"line":4,"character":10}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"untitled:labels","version":8},"contentChanges":[{"text":"[Guide][ma]\n\n[map]: /current\n"}]}}"###,
        r###"{"jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{"textDocument":{"uri":"untitled:labels"},"position":{"line":0,"character":10}}}"###,
        r###"{"jsonrpc":"2.0","id":5,"method":"shutdown"}"###,
        r###"{"jsonrpc":"2.0","method":"exit"}"###,
    ])?;
    assert!(status.success());
    let result = |id| {
        replies
            .iter()
            .find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id))
            .and_then(|reply| reply.get("result"))
            .ok_or("missing completion result")
    };
    let triggers = result(1)?
        .get("capabilities")
        .and_then(|v| v.get("completionProvider"))
        .and_then(|v| v.get("triggerCharacters"))
        .ok_or("missing triggers")?;
    assert_eq!(
        array(triggers)
            .iter()
            .filter_map(Json::as_str)
            .collect::<Vec<_>>(),
        ["#", "[", "^"]
    );
    for (id, label, line, start, end, inserted) in [
        (2, "[^research]", 0, 11, 20, "research"),
        (3, "manual", 4, 8, 10, "manual"),
        (4, "map", 0, 8, 10, "map"),
    ] {
        let items = array(result(id)?.get("items").ok_or("missing items")?);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].get("label").and_then(Json::as_str), Some(label));
        let edit = items[0].get("textEdit").ok_or("missing edit")?;
        assert_eq!(edit.get("newText").and_then(Json::as_str), Some(inserted));
        let range = edit.get("range").ok_or("missing edit range")?;
        for (side, column) in [("start", start), ("end", end)] {
            let position = range.get(side).ok_or("missing edit position")?;
            assert_eq!(position.get("line").and_then(Json::as_u64), Some(line));
            assert_eq!(
                position.get("character").and_then(Json::as_u64),
                Some(column)
            );
        }
    }
    Ok(())
}

#[test]
fn real_reference_rename_returns_versioned_edits_without_changing_the_open_buffer() -> TestResult {
    let (status, replies) = session(&[
        r###"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"workspace":{"workspaceEdit":{"documentChanges":true}}}}}"###,
        r###"{"jsonrpc":"2.0","method":"initialized","params":{}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"untitled:references","languageId":"markdown","version":7,"text":"[read][ref] [ref][] [ref]\n\n[ref]: /target \"Title\"\n"}}}"###,
        r###"{"jsonrpc":"2.0","id":2,"method":"textDocument/prepareRename","params":{"textDocument":{"uri":"untitled:references"},"position":{"line":0,"character":8}}}"###,
        r###"{"jsonrpc":"2.0","id":3,"method":"textDocument/rename","params":{"textDocument":{"uri":"untitled:references"},"position":{"line":0,"character":8},"newName":"manual"}}"###,
        r###"{"jsonrpc":"2.0","id":4,"method":"textDocument/prepareRename","params":{"textDocument":{"uri":"untitled:references"},"position":{"line":0,"character":8}}}"###,
        r###"{"jsonrpc":"2.0","id":5,"method":"shutdown"}"###,
        r###"{"jsonrpc":"2.0","method":"exit"}"###,
    ])?;
    assert!(status.success());
    let result = |id| {
        replies
            .iter()
            .find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id))
            .and_then(|reply| reply.get("result"))
            .ok_or("missing result")
    };
    assert_eq!(
        result(2)?.get("placeholder").and_then(Json::as_str),
        Some("ref")
    );
    assert_eq!(
        result(2)?,
        result(4)?,
        "rename must leave the live buffer unchanged"
    );
    let changes = array(
        result(3)?
            .get("documentChanges")
            .ok_or("missing documentChanges")?,
    );
    assert_eq!(changes.len(), 1);
    let document = changes[0].get("textDocument").ok_or("missing document")?;
    assert_eq!(
        document.get("uri").and_then(Json::as_str),
        Some("untitled:references")
    );
    assert_eq!(document.get("version").and_then(Json::as_u64), Some(7));
    let edits = array(changes[0].get("edits").ok_or("missing edits")?);
    assert_eq!(edits.len(), 4);
    assert_eq!(
        edits
            .iter()
            .map(|edit| edit.get("newText").and_then(Json::as_str))
            .collect::<Vec<_>>(),
        [
            Some("manual"),
            Some("[manual]"),
            Some("[manual]"),
            Some("manual")
        ]
    );
    for (edit, (line, start, end)) in
        edits
            .iter()
            .zip([(0, 7, 10), (0, 17, 19), (0, 25, 25), (2, 1, 4)])
    {
        let range = edit.get("range").ok_or("missing edit range")?;
        let a = range.get("start").ok_or("missing start")?;
        let b = range.get("end").ok_or("missing end")?;
        assert_eq!(a.get("line").and_then(Json::as_u64), Some(line));
        assert_eq!(b.get("line").and_then(Json::as_u64), Some(line));
        assert_eq!(a.get("character").and_then(Json::as_u64), Some(start));
        assert_eq!(b.get("character").and_then(Json::as_u64), Some(end));
    }
    Ok(())
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
    let diagnostics: Vec<_> = replies
        .iter()
        .filter(|reply| {
            reply.get("method").and_then(Json::as_str) == Some("textDocument/publishDiagnostics")
        })
        .filter_map(|reply| reply.get("params"))
        .collect();
    assert_eq!(diagnostics.len(), 3);
    assert!(
        !array(
            diagnostics[0]
                .get("diagnostics")
                .ok_or("missing initial findings")?
        )
        .is_empty()
    );
    assert_eq!(
        diagnostics[1].get("version").and_then(Json::as_u64),
        Some(2)
    );
    assert!(
        array(
            diagnostics[1]
                .get("diagnostics")
                .ok_or("missing repaired findings")?
        )
        .is_empty()
    );
    assert!(
        array(
            diagnostics[2]
                .get("diagnostics")
                .ok_or("missing close findings")?
        )
        .is_empty()
    );
    let result = |id| {
        replies
            .iter()
            .find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id))
            .and_then(|reply| reply.get("result"))
            .ok_or("missing request result")
    };
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
    let (status, replies) =
        session(&[r###"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"###])?;
    assert!(!status.success());
    assert_eq!(replies.len(), 1);
    Ok(())
}
