# Idea Backlog (bead nikq)

This document preserves the wave-1 idea-wizard candidates that did NOT make the
top-5 epics and were not captured in the wave-2 promotion. Each item is held
here so a future discovery pass can promote it to an epic with a real design
sketch and acceptance criteria, without losing the rationale that surfaced it.

Promotion rule: every item below needs a separate discovery pass (design sketch
+ acceptance criteria as a proper epic in the bead tracker) before any work
begins. The caveats in the per-item notes are the unverified assumptions to
challenge during discovery.

## 1. Language server (LSP) for Markdown powered by fmd diagnostics

A real Markdown LSP would surface `fmd` parser/verifier findings live inside
editors: caret-rendered diagnostics for unclosed fences, broken anchors, missing
glyphs, parser/HTML/PDF mismatches. Big surface, but a std-only LSP transport
(streams of length-prefixed JSON-RPC over stdio) is in scope for this project's
zero-third-party-dep engine.

- Why it matters: agents and editor users want inline preview-quality
  diagnostics without leaving their buffer.
- Caveat: LSP itself is a stateful protocol (initialize, didOpen, didChange,
  didSave, completion, codeAction, semanticTokens); a std-only transport is
  tractable but the full feature matrix is large. Discovery should pick a
  minimal-first surface (diagnostics only, no completion/semanticTokens yet) and
  design the stdio framing precisely.
- Synergy: builds directly on the fmd caret renderer (`src/caret.rs`) and the
  shared `ParseDiagnostic` / `VerifyReport` types. The 9wse diagnostics epic
  already proves the underlying error model.

## 2. Standalone SVG render backend

Markdown -> vector poster / one-pager as a single SVG file (no embedded fonts
required, scales for posters / hero images). Aimed at docs sites that want a
raster-clean one-image-per-page deliverable.

- Why it matters: poster artwork, social previews, and "first-screen" docs
  renders currently route through the PDF path or screenshot the HTML preview;
  neither is ideal for vector-clean output.
- Caveat: the SVG emitter would be a new surface, parallel to but separate from
  the PDF vector draw path. `frankenmermaid` already provides a vector-SVG
  precedent; this would be Markdown-shaped rather than diagram-shaped. Discovery
  must define the layout model (page sizes, margins, text flow) and the
  typography story (host-supplied font bytes, subsetting).
- Synergy: the existing SVG -> PDF path (`src/pdf.rs` vector SVG drawing) is
  related but emits PDF content streams, not standalone SVG. Any drawing
  primitive library should be factored to share, not copy.

## 3. Shell completions + man pages

Use `clap_complete` (shell completion generation) and `clap_mangen` (man page
generation) to ship agent-friendly shell ergonomics.

- Why it matters: agents and humans save keystrokes; man pages document the
  current flag surface without forcing a `fmd --help` round trip.
- Caveat: both crates would add third-party dependencies. The current
  zero-third-party-dep engine doctrine means they MUST live behind the `cli`
  feature and NOT touch the core. The dep audit (`docs/POLICY.md` if it
  existed) would need a small exception.
- Synergy: cheap polish for the distribution-parity epic
  (`smif`); only worth promoting once the CLI flag surface is stable enough
  that completions don't churn weekly.

## 4. GitHub Action + pre-commit hook (render-and-verify in CI)

A drop-in GitHub Action and a pre-commit hook that run `fmd verify` on changed
Markdown files, blocking PRs that introduce findings.

- Why it matters: continuous verification is the natural follow-on once
  `fmd verify` is stable. Catches regressions at the same place they're
  authored.
- Caveat: deliberately sequenced BEHIND the fmd-verify epic
  (`yo83`). The hook should call `verify` directly, not duplicate its checks,
  so this item is "promote after yo83 is stable" and not before.
- Synergy: also fits the "doctor" surfacing pattern (what's wrong with my
  document?); would share exit-code conventions and the verify JSON schema.

## 5. External link checker

`fmd verify --links`: HEAD-based external link checker, cached, network-optional
opt-in. Distinct from `fmd verify`'s determinism-preserving default because
network state is by definition non-deterministic.

- Why it matters: documentation drift is a long tail of broken links. Catching
  them in CI before publish saves readers from 404s.
- Caveat: deliberately excluded from `fmd verify`'s default path (breaks
  determinism guarantees). Belongs as an opt-in flag or a separate
  `doctor`/`audit` subcommand, not the default verify. Discovery should also
  decide on the rate-limit / redirect / cache policy.
- Synergy: same JSON envelope as `verify` (an extension to `findings` is fine)
  so existing CI consumers don't need a second tool to learn.

## 6. Frontmatter metadata (YAML / TOML)

A preamble in the Markdown source (between `---` fences, YAML or TOML) that
maps to `title` / `author` / `lang` / `PdfOptions` overrides. Hosts the
growing list of "document-scoped" settings that doesn't belong in a
project-wide config.

- Why it matters: per-document overrides (especially `lang` for the multi-lang
  hyphenation epic `38re`, and `title` / `author` for PDF metadata) are
  awkward to express at the CLI. Frontmatter is the CommonMark-adjacent
  solution.
- Caveat: parser decision required (std-only YAML subset, std-only TOML subset,
  or hand-rolled key=value?). Both YAML and TOML pull in non-trivial
  third-party parsers; the engine's zero-dep doctrine pushes this toward a
  hand-rolled minimal subset, which limits surface and may surprise users.
  Discovery should pick one format first (TOML is more grammar-constrained)
  and gate the other on user demand.
- Synergy: the multi-lang hyphenation epic (`38re`) needs a `lang` knob, and
  the PDF metadata path already exposes `title` / `author`; frontmatter would
  unify these.

## 7. Transclusion / includes (`{{#include file.md}}`)

A wiki-style include syntax that pulls another Markdown file's content into
the current document at parse time. Document composition for multi-page
manuals.

- Why it matters: long-form docs benefit from reuse. Repeating a "Glossary"
  block across 12 files is a maintenance trap; transclusion solves it.
- Caveat: cycle detection is the hard part. Discovery must define: what counts
  as a cycle (A includes B includes A), how errors are reported (spans, exit
  code), how partial renders behave when an included file fails to parse, and
  how the watch epic (`j3e0`) handles include-graph invalidation.
- Synergy: interacts with watch mode (`j3e0`) and batch input walking. A
  change to any included file should re-render the including document, and
  batch should walk the include graph so "render this directory" doesn't
  silently miss nested files.
