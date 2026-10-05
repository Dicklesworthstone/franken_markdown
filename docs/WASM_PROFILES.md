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

Both existing package builders explicitly request wasm-full, preserving their
full-package smoke tests and size budgets. A manually built wasm-bindgen-only
package cannot create a flow session; the existing JavaScript facade refuses it
with UNSUPPORTED_WASM_PACKAGE instead of emulating an editor. Consumers needing
flow must build wasm-flow or wasm-full from matching source.

## Separate local packages

`scripts/dsr-wasm-profiles.sh` is the dedicated DSR-host entrypoint for building
both profiles from one clean committed checkout. It uses isolated Cargo target
directories, locked dependencies, native library checks/tests, and release WASM
builds. It retains both packages and a comparison report beneath a fresh
`tests/artifacts/wasm/profiles.XXXXXXXX/` directory. It does not delete prior
artifacts, modify the source manifest, invoke Actions, or publish to a registry.

The assembled packages are named with `-render` and `-full` suffixes and marked
`private: true`; these are local validation artifacts, not newly released npm
packages. The render package exports the main renderer and web component. The
full package retains all source-manifest exports, including flow/book/worker
entrypoints. Every generated binding/binary comes from its selected build,
never an old `wasm/pkg` directory. The existing full distribution remains intact.

The build-time `scripts/assemble-wasm-profile.mjs` can also assemble already
built artifacts:

```sh
node --experimental-vm-modules scripts/assemble-wasm-profile.mjs \
  wasm /absolute/path/to/generated-bindings /absolute/path/to/new-package render
```

The destination parent must exist and the destination must not exist. Static
JavaScript imports and re-exports are parsed using Node's V8 module parser and
recursively copied, including helper files absent from hand-maintained copy
lists. Input modules are never linked or evaluated during assembly. Cycles are
deduplicated. Dynamic imports, workers, type declarations and other resources
must still be declared in the source manifest; this is not a JavaScript bundler,
TypeScript compiler, or dependency discovery for arbitrary computed URLs.

Full-profile roots come from `files` and `exports`; render roots are the root
renderer/web-component plus their declarations and generated binding files.
Bare/remote static imports, escaping paths, symlinks, missing files and syntax
errors fail before output creation. Admission is bounded to 4,096 input files
and 256 MiB of payload. Files are captured before writing, and writes never
replace existing files. A filesystem write failure can leave a new incomplete
directory; the package manifest is written last and no success is returned.
Checkout/build directories are trusted inputs, not a hostile concurrent-filesystem
sandbox. Every copied payload has a SHA-256 inventory in `fmd-profile.json`.

## Executable profile comparison

`wasm/compare_profiles.mjs` initializes both real generated modules, checks raw
WASM and JavaScript ABI presence/absence, and exercises native flow creation,
display and editing in the full profile. It renders two HTML/PDF fixtures through
the generated bindings and another HTML/PDF fixture through each actual public
facade. All six corresponding outputs must be byte-identical between profiles.

```sh
node wasm/compare_profiles.mjs /path/to/render /path/to/full \
  /path/to/new-profile-report.json EXACT_40_HEX_SOURCE_COMMIT
```

The create-only JSON report records each binary's SHA-256, raw size and gzip-9
size, output fingerprints, and errors. A valid empty WASM file cannot masquerade
as a renderer. The runner requires both a raw-size and gzip-size improvement
against the full build from the same invocation; equality or a loss fails. Both
profiles must also stay under the existing full-package ceilings of 7,900,000
raw bytes and 3,400,000 gzip bytes. No smaller-profile ceiling has been invented
without a measured run. Failed comparisons retain their failure report.

This comparison is profile-vs-profile, not native-vs-WASM or browser/raster proof.
The original full-package, native-parity and visual gates remain independently
required. The DSR entrypoint checks the source commit/clean-tree fence around
builds and comparison; its exit status is part of the evidence, not just the
presence of a JSON file. The runner executes package code only from the trusted
build directories supplied by the host.

## Remaining scope and verification

This is not completion of the entire bundle-size issue. One-shot book and
interactive exports, and the reusable book adapter, are still in wasm-bindgen.
Separating those roots, publishing distinct npm distributions, measuring the
resulting raw/gzip sizes, and ratcheting budgets on actual evidence remain open.
No claim is made that this slice reaches the approximately 4.5 MB target.

```sh
node --experimental-vm-modules --test scripts/assemble-wasm-profile.test.mjs \
  wasm/flow_profile_probe.test.mjs wasm/compare_profiles.test.mjs
```

All 26 tests pass in the authoring environment: 13 real-filesystem assembler
tests, seven profile-probe tests with explicit binding doubles, and six
comparison/error/size-gate tests. Assembly fixtures use a real but empty WASM
module; the comparison runner correctly refuses it as a renderer. These tests
prove package assembly and gate behavior, not Rust rendering or a size win.
JavaScript syntax, shell syntax and the Cargo feature-graph checks also pass.

Rust fmt/check/clippy/test were attempted but Cargo and Rust were absent. The
DSR profile script exits 3 at its missing-Cargo preflight, before creating build
artifacts. Neither generated profile has been built or executed here. Rust,
linker elimination, actual size measurement, DSR, native parity and browser
validation therefore remain unverified. No release, registry publish, Actions
workflow or size-budget ratchet was made.
