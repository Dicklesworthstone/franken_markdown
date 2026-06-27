//! Long-name binary (`franken_markdown`). A one-line shim over the shared CLI
//! entrypoint so no source is duplicated across build targets.

fn main() -> std::process::ExitCode {
    franken_markdown::cli::main()
}
