//! Long-name binary (`franken_markdown`). Uses the same native book-aware
//! dispatcher as `fmd`; non-book commands delegate to the existing CLI.

fn main() -> std::process::ExitCode {
    franken_markdown::book::native::main()
}
