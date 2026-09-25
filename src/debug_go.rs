use crate::lang_go::lex_go_into;
use crate::highlight::{Span, Tok};

pub fn dump(code: &str) {
    let mut spans = Vec::new();
    lex_go_into(code, &mut spans);
    for s in &spans {
        println!("DBG {} {:?} [{}..{}] {:?}", s.end, s.kind, s.start, s.end, &code[s.start..s.end.min(code.len())]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dump_numbers() {
        crate::debug_go::dump("3i 1_000 0xdead_beef 1.5e-2 0b1010_1010");
    }
}
