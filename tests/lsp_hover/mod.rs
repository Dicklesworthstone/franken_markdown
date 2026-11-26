//! Exercise hover through the real framed stdio server and live document edits.
use super::*;

#[test]
fn hover_previews_track_open_targets_and_require_synchronized_source() -> TestResult {
    let (status, replies) = session(&[
        r###"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"textDocument":{"hover":{"contentFormat":["plaintext"]}}}}}"###,
        r###"{"jsonrpc":"2.0","method":"initialized","params":{}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///fmd-hover/start.md","languageId":"markdown","version":1,"text":"😀 [read](other.md#topic)\r\n\r\nNote[^n].\r\n\r\n[^n]: Footnote **body**.\r\n"}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///fmd-hover/other.md","languageId":"markdown","version":1,"text":"# Topic\r\n\r\nFirst target paragraph.\r\n\r\n# Next\r\n\r\nOther section."}}}"###,
        r###"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/start.md"},"position":{"line":0,"character":12}}}"###,
        r###"{"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/start.md"},"position":{"line":2,"character":6}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///fmd-hover/other.md","version":2},"contentChanges":[{"text":"# Topic\n\nUpdated target paragraph."}]}}"###,
        r###"{"jsonrpc":"2.0","id":4,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/start.md"},"position":{"line":0,"character":12}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///fmd-hover/other.md","version":3},"contentChanges":[{"range":{"start":{"line":999,"character":0},"end":{"line":999,"character":0}},"text":"bad"}]}}"###,
        r###"{"jsonrpc":"2.0","id":5,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/start.md"},"position":{"line":0,"character":12}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///fmd-hover/other.md","version":4},"contentChanges":[{"text":"# Topic\n\nRecovered target paragraph."}]}}"###,
        r###"{"jsonrpc":"2.0","id":6,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/start.md"},"position":{"line":0,"character":12}}}"###,
        r###"{"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":"file:///fmd-hover/other.md"}}}"###,
        r###"{"jsonrpc":"2.0","id":7,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/start.md"},"position":{"line":0,"character":12}}}"###,
        r###"{"jsonrpc":"2.0","id":8,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/start.md"},"position":{"line":0,"character":1}}}"###,
        r###"{"jsonrpc":"2.0","id":9,"method":"shutdown"}"###,
        r###"{"jsonrpc":"2.0","method":"exit"}"###,
    ])?;
    assert!(status.success());
    let reply = |id| {
        replies
            .iter()
            .find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id))
            .ok_or("missing hover reply")
    };
    assert_eq!(
        reply(1)?
            .get("result")
            .and_then(|result| result.get("capabilities"))
            .and_then(|caps| caps.get("hoverProvider"))
            .and_then(Json::as_bool),
        Some(true)
    );
    for (id, expected) in [
        (2, "First target paragraph."),
        (3, "Footnote **body**."),
        (4, "Updated target paragraph."),
        (6, "Recovered target paragraph."),
    ] {
        let result = reply(id)?.get("result").ok_or("missing hover result")?;
        assert!(result.get("range").is_none());
        let contents = result.get("contents").ok_or("missing hover contents")?;
        assert_eq!(contents.get("kind").and_then(Json::as_str), Some("plaintext"));
        let value = contents
            .get("value")
            .and_then(Json::as_str)
            .ok_or("missing hover text")?;
        assert!(value.contains(expected), "{value}");
        assert!(!value.contains("Other section."));
    }
    assert_eq!(reply(7)?.get("result"), Some(&Json::Null));
    for (id, code) in [(5, -32801.0), (8, -32602.0)] {
        assert_eq!(
            reply(id)?
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Json::as_f64),
            Some(code)
        );
    }
    Ok(())
}

#[test]
fn hover_negotiates_plaintext_markdown_legacy_and_unsupported_clients() -> TestResult {
    let source = "[read](#topic)\n\n# Topic\n\n\x60\x60\x60\x60\n<img src=\"https://example.test/private\">\n\x60\x60\x60\x60\n[Run](command:danger)\n";
    let encoded_source = Json::String(source.to_string()).to_json_string();
    let open = format!(
        r###"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///fmd-hover/markup.md","languageId":"markdown","version":1,"text":{encoded_source}}}}}}}"###
    );
    for (hover_capability, expected) in [
        (r###"{"contentFormat":["markdown"]}"###, "markdown"),
        (r###"{"contentFormat":["markdown","plaintext"]}"###, "markdown"),
        (r###"{"contentFormat":["plaintext","markdown"]}"###, "plaintext"),
        ("{}", "legacy"),
        (r###"{"contentFormat":[]}"###, "disabled"),
        (r###"{"contentFormat":["future-format"]}"###, "disabled"),
    ] {
        let initialize = format!(
            r###"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"capabilities":{{"textDocument":{{"hover":{hover_capability}}}}}}}}}"###
        );
        let (status, replies) = session(&[
            &initialize,
            &open,
            r###"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///fmd-hover/markup.md"},"position":{"line":0,"character":9}}}"###,
            r###"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"###,
            r###"{"jsonrpc":"2.0","method":"exit"}"###,
        ])?;
        assert!(status.success());
        let reply = |id| {
            replies
                .iter()
                .find(|reply| reply.get("id").and_then(Json::as_u64) == Some(id))
                .ok_or("missing negotiated hover reply")
        };
        assert_eq!(
            reply(1)?
                .get("result")
                .and_then(|result| result.get("capabilities"))
                .and_then(|caps| caps.get("hoverProvider"))
                .and_then(Json::as_bool),
            Some(expected != "disabled")
        );
        let result = reply(2)?.get("result").ok_or("missing negotiated hover result")?;
        if expected == "disabled" {
            assert_eq!(result, &Json::Null);
            continue;
        }
        let contents = result.get("contents").ok_or("missing negotiated contents")?;
        let value = contents
            .get("value")
            .and_then(Json::as_str)
            .ok_or("missing negotiated text")?;
        assert!(value.contains("<img src=\"https://example.test/private\">"));
        assert!(value.contains("[Run](command:danger)"));
        if expected == "legacy" {
            assert_eq!(contents.get("language").and_then(Json::as_str), Some("markdown"));
            assert!(contents.get("kind").is_none());
        } else {
            assert_eq!(contents.get("kind").and_then(Json::as_str), Some(expected));
        }
        if expected == "markdown" {
            let fence = "\x60".repeat(5);
            assert!(value.starts_with(&format!("{fence}\n# Topic")));
            assert!(value.ends_with(&format!("\n{fence}")));
            assert_eq!(value.matches(&fence).count(), 2);
            assert!(value.len() <= 24 * 1024);
        } else {
            assert!(value.starts_with("# Topic"));
            assert!(value.len() <= 8 * 1024);
        }
    }
    Ok(())
}
