#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$repo_root/target/fmd-checks}"

work_dir="$CARGO_TARGET_DIR/determinism"
mkdir -p "$work_dir"

sample="$work_dir/sample.md"
cat > "$sample" <<'MD'
# Determinism Sample

This paragraph has **strong text**, *emphasis*, `inline code`, and a
[link](https://example.com).

```rust
fn main() {
    let x = 42;
    println!("{x}");
}
```

| Name | Value |
|---|---:|
| alpha | 1 |
| beta | 2 |

> stable blockquote
MD

run_fmd() {
  cargo run --quiet --bin fmd -- "$@"
}

compare_stdout() {
  local name="$1"
  shift
  local a="$work_dir/$name.a"
  local b="$work_dir/$name.b"
  run_fmd "$@" >"$a"
  run_fmd "$@" >"$b"
  if ! cmp -s "$a" "$b"; then
    printf 'fmd determinism check: %s stdout changed across identical runs\n' "$name" >&2
    diff -u "$a" "$b" >&2 || true
    exit 1
  fi
}

compare_file_output() {
  local name="$1"
  local ext="$2"
  shift
  shift
  local a="$work_dir/$name.a.$ext"
  local b="$work_dir/$name.b.$ext"
  run_fmd "$@" --out "$a" >/dev/null
  run_fmd "$@" --out "$b" >/dev/null
  if ! cmp -s "$a" "$b"; then
    printf 'fmd determinism check: %s output file changed across identical runs\n' "$name" >&2
    if [ "$ext" = "html" ]; then
      diff -u "$a" "$b" >&2 || true
    else
      cmp -l "$a" "$b" | head -20 >&2 || true
    fi
    exit 1
  fi
}

echo "fmd determinism check: agent JSON surfaces"
compare_stdout capabilities capabilities --json
compare_stdout doctor doctor --json
compare_stdout robot-triage --robot-triage

echo "fmd determinism check: HTML stdout"
compare_stdout html-stdout "$sample"

echo "fmd determinism check: HTML file"
compare_file_output html-file html "$sample"

echo "fmd determinism check: PDF file"
compare_file_output pdf-file pdf "$sample" --to pdf

echo "fmd determinism check: ok"
