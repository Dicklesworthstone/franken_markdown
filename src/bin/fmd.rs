//! `fmd` — the short alias agents and humans actually type. Identical entrypoint
//! to the long-name binary; a one-line shim so no source is shared across build
//! targets.

fn main() -> std::process::ExitCode {
    franken_markdown::cli::main()
}
