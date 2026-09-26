//! Native, opt-in Markdown language server. Stdout is protocol-only.
#![forbid(unsafe_code)]

mod lsp;

use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        println!("fmd-lsp [--stdio]\n\nMarkdown language server over Content-Length framed stdio.");
        return ExitCode::SUCCESS;
    }
    if args.len() == 1 && matches!(args[0].as_str(), "--version" | "-V") {
        println!("fmd-lsp {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if !(args.is_empty() || (args.len() == 1 && args[0] == "--stdio")) {
        eprintln!("usage: fmd-lsp [--stdio]");
        return ExitCode::FAILURE;
    }
    match lsp::run(&mut io::stdin().lock(), &mut io::stdout().lock()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("fmd-lsp: {error}");
            ExitCode::FAILURE
        }
    }
}
