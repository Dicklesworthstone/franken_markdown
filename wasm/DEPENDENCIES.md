# WASM Package Dependency Evidence

Status: package adapter only. The render core remains dependency-free under:

```bash
cargo check --no-default-features --lib
cargo tree --no-default-features --edges normal
```

## Why `wasm-bindgen` is used

The preferred manual ABI would expose raw Wasm functions directly, but Rust 2024
requires unsafe attributes for stable symbol exports such as `no_mangle` and
`export_name`. This repository forbids unsafe code at the crate level, so a raw
pointer ABI would violate project policy.

`wasm-bindgen` is therefore feature-gated behind `--features wasm-bindgen` and
kept outside the default render path. It is used only to build a browser package
adapter over the dependency-free `crate::wasm` API.

## Cargo tree

Command:

```bash
cargo tree --target wasm32-unknown-unknown --no-default-features --features wasm-bindgen --edges normal --prefix none
```

Observed tree on 2026-06-28:

```text
franken_markdown v0.0.0 (/data/projects/franken_markdown)
wasm-bindgen v0.2.126
cfg-if v1.0.4
once_cell v1.21.4
wasm-bindgen-macro v0.2.126 (proc-macro)
quote v1.0.46
proc-macro2 v1.0.106
unicode-ident v1.0.24
wasm-bindgen-macro-support v0.2.126
bumpalo v3.20.3
proc-macro2 v1.0.106 (*)
quote v1.0.46 (*)
syn v2.0.118
proc-macro2 v1.0.106 (*)
quote v1.0.46 (*)
unicode-ident v1.0.24
wasm-bindgen-shared v0.2.126
unicode-ident v1.0.24
wasm-bindgen-shared v0.2.126
unicode-ident v1.0.24
```

## Policy checks

Required checks:

```bash
scripts/check-wasm-core.sh
cargo check --target wasm32-unknown-unknown --no-default-features --features wasm-bindgen --lib
cargo test --no-default-features --features wasm-bindgen --test wasm_package_test
```

Full browser package assembly:

```bash
cargo install wasm-bindgen-cli --version 0.2.126 --locked
scripts/check-wasm-package.sh
```
