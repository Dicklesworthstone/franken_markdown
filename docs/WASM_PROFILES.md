# Composable browser build profiles

The browser ABI now separates three optional product surfaces. All feature
selections below use `--no-default-features`; `wasm-full` preserves the complete
browser distribution, and native Rust renderer/editor APIs are unchanged.

| Cargo feature | Additional JavaScript exports |
| --- | --- |
| `wasm-bindgen` | Single-document HTML/PDF/SVG/EPUB and document intelligence |
| `wasm-flow` | `FmdFlowSession`: persistent editing, layout, assets and reading |
| `wasm-book` | `FmdBook`, legacy/configured book PDF, legacy/canonical book sites |
| `wasm-workspace` | `renderInteractiveHtmlConfigured`: self-hosting HTML workspace |
| `wasm-full` | All three optional surfaces |

Each optional feature enables `wasm-bindgen`, but not another optional surface.
Features can be combined, for example `--features wasm-flow,wasm-book`.
`BookRenderer`, `BookWorkspace` and `BrowserFlowSession` remain native Rust APIs.
One-shot book/workspace adapter functions also remain callable from Rust: only
their `wasm_bindgen` export attributes are conditional. This removes their
JavaScript export roots without maintaining a second rendering implementation.
The configured book PDF function is re-exported from `wasm_abi` for Rust callers.

## JavaScript compatibility

The root facade uses namespace lookup, not mandatory named imports, for the
optional book/workspace bindings. A render-only binary can therefore load the
same facade and render ordinary documents. Unsupported book/workspace calls
reject with `UNSUPPORTED_WASM_PACKAGE` and the required Cargo feature before
WASM initialization or reading/copying request payloads. The facade does not
fetch another binary, emulate a missing renderer, or fall back to a different
format. `createRenderer()` retains its methods with the same refusal behavior.

Supported legacy book packages keep their existing narrow routes and warnings.
Configured/canonical-only bindings are accepted where their ABI matches the
request. A configured PDF binding is never called with legacy arguments, nor
are advanced options silently dropped onto a legacy renderer. Workspace source
and settings are captured before asynchronous initialization, like book exports.

## Local packages and verification

Both existing full-package builders explicitly request `wasm-full`; their smoke
corpora and size budgets are retained. `scripts/dsr-wasm-profiles.sh` is the
additional DSR-host entrypoint for isolated render/full builds, create-only local
package assembly and comparison. It does not publish or delete prior artifacts.

`scripts/assemble-wasm-profile.mjs` recursively closes static JavaScript imports
and re-exports without evaluating them. The render package exports the root
facade/web component; full retains the source manifest's subpaths. Dynamic
imports, workers, declarations and other resources must be declared explicitly.
Packages are private local artifacts with `-render`/`-full` name suffixes and a
SHA-256 inventory, not newly published npm releases. Input directories must be
trusted; this is not confinement against a concurrently hostile filesystem.

`wasm/compare_profiles.mjs` checks actual generated bindings and binaries,
executes flow editing on the full profile, and compares six HTML/PDF outputs.
Both raw and gzip sizes must improve relative to the same full build, while
remaining below the unchanged 7,900,000 / 3,400,000-byte full-package ceilings.
Its output is profile-vs-profile proof, not native-vs-WASM or browser/raster proof.

```sh
node --experimental-vm-modules --test wasm/browser_profile_api.test.mjs
node --experimental-vm-modules --test scripts/assemble-wasm-profile.test.mjs \
  wasm/flow_profile_probe.test.mjs wasm/compare_profiles.test.mjs
```

The 15 root-facade tests pass using the complete production JavaScript module,
the actual paper normalizer and explicit generated-binding doubles. Fourteen
fail against the previous root facade. They cover stripped, partial, legacy and
full export sets, dispatch, errors, request capture and result cleanup. These
are module-linking/API tests, not native rendering or generated-WASM evidence.
JavaScript syntax and the Cargo feature dependency closure also passed.

Rust fmt/check/clippy/test were attempted but Cargo is unavailable in the
implementation environment. No generated profile was built here. Compiler/linker
behavior, actual size savings, DSR, native parity and browser rendering remain
unverified. The existing DSR/native tests must run with the matching source and
all selected feature combinations before release. The approximately 4.5 MB
render-only target and a lower measured size ratchet are not claimed. No release,
registry publish, Actions workflow or size-budget change is included.
