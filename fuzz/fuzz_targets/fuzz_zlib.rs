//! Coverage-guided fuzz target: bounded zlib inflate (m7fs.1).
#![no_main]

use franken_markdown::zlib_decompress;
use libfuzzer_sys::fuzz_target;

const MAX_OUT: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let _ = zlib_decompress(data, MAX_OUT);
});
