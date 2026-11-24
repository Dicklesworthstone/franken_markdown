# Analyze and compare documents with includes

`fmd stats` and `fmd diff` can analyze the document assembled by native include
expansion. Pass `--expand-includes` to select this behavior:

```sh
fmd stats manual.md --expand-includes --json
fmd stats manual.md --expand-includes --profile commonmark-gfm
fmd diff before/main.md after/main.md --expand-includes --json
fmd diff before/main.md after/main.md --expand-includes --out review.html
```

Without the flag, these commands continue to inspect the original Markdown
text. They do not read included files, so raw analysis still works when those
files are unavailable. `render` and `verify` retain their existing automatic
include preparation.

## Compare the content behind an unchanged directive

Suppose both `before/main.md` and `after/main.md` contain:

```md
# Manual

{{#include parts/body.md}}
```

A raw diff sees identical source files even when the two `parts/body.md` files
contain different prose. With `--expand-includes`, each revision resolves the
directive relative to its own directory. The semantic diff and HTML comparison
then include those content changes. Each revision is an independent source
tree; the command does not retrieve historical include contents from Git.
Supply separate checkouts or captured directories when comparing revisions.

Nested includes, named snippets such as `parts/body.md:example`, and line
selections such as `parts/body.md:2:8` use the same expander as native rendering.
Unselected text does not activate its own includes. Relative Markdown image
and link destinations are rebased from the file containing the selected text.
Analysis does not load images, fonts, or remote URLs; it compares source
structure and destinations, not the contents of those resource files.

## Grammar, stdin, and raw text

An explicit `--profile commonmark-gfm` or `--profile gfm-plus` selects the
grammar before expansion and parsing. Omission inherits the native `profile`
configuration. `--no-config` disables configuration, and omission of both the
flag and config profile retains historical authoring behavior.

Directive examples inside fenced, indented, or inline code remain literal
under the selected grammar. They do not require a file input. This also works
with `fmd stats --text ...`, `fmd stats -`, and either stdin operand of
`fmd diff`. An active include in stdin or `--text` fails with usage exit code
64 because there is no source directory. Providing a positional filename
alongside `stats --text` does not authorize filesystem resolution for that text.

## Report semantics and limits

The existing stats and `fmd-diff-v1` JSON schemas are unchanged. Expanded stats
describe the complete expanded and rebased Markdown buffer: `bytes` and
`lines` count that buffer, while readability, outline, links, and findings use
its parsed document. Diff revision names remain the supplied input names;
block changes describe the expanded documents. These reports do not invent
original-file locations for text assembled from multiple sources.

`--max-input-bytes` bounds each primary/include read and each expanded
revision separately. It defaults to 64 MiB; expansion also retains the engine's
64 MiB ceiling, 16-level nesting limit, and 4096-resolution limit. Increasing
the input flag does not remove the expansion ceiling. Only regular UTF-8
include files beneath the corresponding top input directory are admitted.
Symlinks do not authorize reads outside that root.

Missing includes, cycles, invalid UTF-8 or selectors, and exceeded source or
expansion budgets fail with input exit code 66. No partial JSON/HTML report is
published, and an existing `--out` file is preserved on failure.

## Preserve every consumed source

The diff output cannot replace either primary input, any included file that
was actually read, or the active configuration file. Protection includes
symlink and hard-link aliases. Selecting one snippet still protects the entire
file read to obtain it. A collision fails with usage exit code 64 before the
output is written.

Files mentioned only by inactive directives or by includes outside a selected
snippet are not read or claimed as analysis inputs. They do not prevent an
otherwise unrelated output. `--json` retains its existing stdout behavior;
`--out` selects the destination for an HTML comparison.

## Regression coverage

`tests/cli_analysis_includes.rs` exercises composed reports against explicit
expanded Markdown, identical directives with different revision-local bodies,
nested named/line selections, profile-dependent code protection, stdin/text
admission, invalid graphs and budgets, and source/output alias protection.

```sh
cargo test --locked --test cli_analysis_includes -- --test-threads=1
```

