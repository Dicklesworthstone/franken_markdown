# File destinations in editor completion

`fmd-lsp` completes the last path segment of an explicit Markdown link from the
editor's currently opened documents. With `guide/start.md`, `guide/chapter.md`
and `guide/examples/first.md` open, `[next](cha` offers `chapter.md`, while
`[next](ex` offers `examples/`. Continuing after that slash offers `first.md`.
An empty destination also lists available files and folders. `(` and `/` are
completion triggers alongside the existing heading and reference triggers.

No workspace directory is scanned. A suggested folder means that a synchronized
open document has that logical parent; it does not grant filesystem access.
Unopened files, unsynchronized targets and ambiguous aliases are excluded.
Closing a target immediately removes it from the suggestions. A source buffer
requiring resynchronization receives the existing `ContentModified` response.

Relative, `./`, `../`, rooted and explicit `file:` paths use the same single-pass
URI decoding, authority normalization and Windows drive rules as go-to-definition.
Relative paths in `untitled:` buffers have no inferred directory. External URLs
and traversal above a URI or drive root do not receive local suggestions.

The real Markdown parser must recognize the candidate as a link before an edit
is offered. This supports quotes, lists, tables and referenced footnotes, while
excluding code, image destinations, HTML attributes, link titles, reference labels
and unreferenced note bodies. Unfinished destinations are admitted only when the
existing parser probe can close them at the end of their enclosing block.

Completion replaces only the path before an existing query or fragment. Angle
brackets, titles and surrounding text retain their bytes; ranges are UTF-16 and
support CRLF and astral characters. Filenames are percent-encoded, including
spaces, Unicode, literal percent signs and Markdown delimiters. Editing an earlier
directory segment with a remaining `/child` suffix is deliberately left alone.
Paths containing a raw `&` are also left alone because Markdown character
references can change file identity and delimiter positions. Completion-generated
paths use `%26` for that character and work normally; query strings retain their
original character-reference spelling.
After `#`, the existing provider still offers the target's emitted heading IDs.

Requests retain the server's source/block/session limits and 8-KiB destination
budget. Results are deterministic and bounded by the existing completion limit;
an incomplete escape or an over-budget completion marks the list incomplete.
The server returns edits for the editor to apply and never changes its buffers
or the filesystem while answering completion requests.

Unit tests cover parser ownership, encoded identities, live synchronization,
folder inference, alias conflicts and platform paths. The real-process
`lsp_protocol_test` exercises framed requests, open/change/close transitions and
exact UTF-16 edit ranges with nonexistent filesystem paths.
