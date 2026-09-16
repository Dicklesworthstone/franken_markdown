//! `fmd` — short alias for the shared executable dispatcher. Book commands
//! use the bounded native book pipeline; all other commands retain the original
//! CLI dispatch. The long-name binary enters through the same function.

fn main() -> std::process::ExitCode {
    franken_markdown::book::native::main()
}
