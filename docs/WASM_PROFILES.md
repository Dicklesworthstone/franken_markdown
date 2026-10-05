# Browser build profiles: persistent editor split

The first implementation slice of issue #16 separates the persistent flow
editor's wasm-bindgen ABI from document rendering. Native BrowserFlowSession,
flow layout, and Rust editor APIs remain available without these browser flags.

- `--no-default-features --features wasm-bindgen`: document render exports,
  without FmdFlowSession or its exported editor entrypoints.
- `--no-default-features --features wasm-flow`: document rendering plus the
  persistent editor, measured display, asset transactions and reading APIs.
- `--no-default-features --features wasm-full`: the full browser distribution;
  currently includes wasm-flow and its wasm-bindgen dependency.

Both existing package builders now explicitly request wasm-full, preserving
all their full-package smoke tests and size budgets. A manually built
wasm-bindgen-only package cannot create a flow session; the existing JavaScript
facade refuses it with UNSUPPORTED_WASM_PACKAGE instead of emulating an editor.
Consumers needing flow must build wasm-flow or wasm-full from matching source.

This is not completion of the entire bundle-size issue. One-shot book and
interactive exports, and the reusable book adapter, are still in wasm-bindgen.
Separating those roots, publishing distinct npm distributions, measuring the
resulting raw/gzip sizes, and ratcheting budgets on actual evidence remain open.
No claim is made that this slice reaches the approximately 4.5 MB target.

`wasm/flow_profile_probe.mjs` is an executable generated-binding contract: it
requires the editor ABI to be absent from render-only builds and functional in
full builds, renders two HTML/PDF fixtures, owns/frees native results, and can
compare the four output byte arrays across profiles. The seven probe unit tests
pass with explicit binding doubles, including negative cases for feature leaks,
non-editing stubs and changed bytes. They do not prove Rust compilation, linker
elimination, actual WASM rendering or size improvement.

Rust fmt/check/clippy/test were attempted in the authoring environment but Cargo
and Rust were absent. Shell syntax and Cargo feature-graph checks passed. DSR,
full-package execution and generated-artifact proof are still required; no
Actions workflow, release, npm publish or size-budget change was made.
