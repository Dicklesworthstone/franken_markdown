# Native Mermaid diagrams

Mermaid fences use the same first-party SVG compiler in HTML, PDF, SVG posters,
and EPUB. Rendering does not load Mermaid.js, execute scripts, or contact the
network. Flowcharts (`graph`/`flowchart`), sequence diagrams (`sequenceDiagram`),
and pie charts are supported. Other diagram types retain their source as a
highlighted code block.

## Flowcharts

````markdown
```mermaid
flowchart RL
    A[Start] --> B{Choose} -->|Yes| C[Finish]
    B -->|No| D[Retry]
    E["Read A --> B; keep this label"] & F[Review] --> C
```
````

Flowcharts accept `graph` or `flowchart`, with `TD`/`TB`, `BT`, `LR`, or `RL`
direction. A bare header defaults to top-to-bottom. A fence explicitly named
`flowchart` can omit the header. Newlines and semicolons separate statements;
quoted labels own their arrows, brackets, semicolons, and `%%` characters.
Outside labels, `%%` starts a line comment.

Nodes can be rectangles (`A[Text]`), rounded rectangles (`A(Text)`), stadiums
(`A([Text])`), diamonds (`A{Text}`), or circles (`A((Text))`). Identifiers use
letters, digits, `_`, `.`, and single hyphens; quote labels containing syntax
characters. A later explicit node declaration updates its label and shape,
while a bare reference keeps the existing declaration. Circle bounds include
the full label.

Supported links include solid arrows (`-->`), open lines (`---`), dotted arrows
(`-.->`) and lines (`-.-`), and thick arrows (`==>`) and lines (`===`). Text can
follow a link in pipes (`-->|Yes|`) or occur inside it (`-- Yes -->`,
`-. Yes .->`, `== Yes ==>`). Quotes also protect syntax within edge labels.
Chains retain each intermediate node; `A & B --> C & D` creates all four
connections. Acyclic joins are placed after every predecessor, and `RL`/`BT`
reverse both node positions and edge direction.

Self-loops (`A --> A`) and feedback connections to earlier nodes use separate
return lanes outside the node area. Their arrows approach the destination from
its normal incoming side, and dotted, thick, open, and labeled links keep their
meaning. Return labels have their own space, and the SVG canvas expands to keep
the routes and labels visible in every direction. Forward-only graphs retain
their existing geometry.

Admission is bounded to 64 KiB of source, 256 nodes, 1024 expanded edges,
128 UTF-8 bytes per identifier, and 512 bytes per label. Malformed statements,
exceeded limits, and unsupported constructs retain the **entire source block**.
This includes subgraphs, style/class/click directives, Mermaid configuration,
extended shapes, and edge metadata. Labels are single-line plain text, escaped
for SVG. No part of an unsupported graph is silently discarded or replaced
with an invented node.

The supported grammar follows the [official Mermaid flowchart syntax](https://mermaid.js.org/syntax/flowchart.html).
See [the flowchart example](../examples/flowcharts.md) for chains, grouped links,
quoted labels, and reverse directions.

## Sequence diagrams

````markdown
```mermaid
sequenceDiagram
    autonumber
    actor User
    participant API as Application
    User->>+API: Submit request
    loop Until accepted
        API->>API: Retry internally
        alt accepted
            API-->>User: Accepted
        else overloaded
            API-->>User: Retry later
        end
    end
    API-->>-User: Complete
    Note left of User: Keep the original request
```
````

Sequence diagrams retain participant order and explicit aliases, including a
declaration after a participant's first message. `actor` draws a person symbol.
The header is `sequenceDiagram`; an explicitly named `sequence` fence may omit
it. Newlines and semicolons separate statements, and `%%` introduces a comment
at the start of a statement. Initialization/configuration directives are not
treated as ordinary comments.

Messages distinguish solid and dotted lines (`->`, `-->`), filled arrows
(`->>`, `-->>`), open asynchronous arrows (`-)`, `--)`), crossed endpoints
(`-x`, `--x`), and bidirectional arrows (`<<->>`, `<<-->>`). Self-messages have
an outward and return path. `activate`/`deactivate` and the `+`/`-` message
suffixes draw stacked activation bars; `+` activates the recipient and `-`
deactivates the sender. An unmatched deactivation retains the whole source;
an activation left open extends to the bottom of its lifeline.

Nested `loop`, `alt`/`else`, `opt`, `par`/`and`, `critical`/`option`, and `break`
fragments draw explicit frames and branch separators, with every condition
visible. `Note left of`, `Note right of`, and `Note over` preserve their actual
placement; `over` can name one or two participants. Titles, `accTitle:` and
single-line `accDescr:` metadata are retained. `autonumber` starts at one, or
accepts an unsigned integer start and a positive integer increment.

Labels wrap within measured conservative boxes, participant columns make room
for message text, and the canvas includes side notes and self-messages.
`<br>`, `<br/>`, and `<br />` introduce label line breaks; other markup is
escaped. Decimal character references such as `#59;` (a literal semicolon) and
named references such as `#infin;` are decoded as text, never executable markup.

Admission is bounded to 64 KiB of source, 64 participants, 1024 events,
16 nested fragments, 32 stacked activations per participant, 128 UTF-8 bytes
per identifier, and 1024 bytes per source label. Unknown statements, malformed
branches, unsupported arrows, and exceeded bounds retain the **entire source
block**. Background `rect` blocks, participant `box` groups, participant JSON
configuration, creation/destruction, menus, central connections, half-arrows,
fractional numbering, and Mermaid initialization remain outside this subset.

The supported grammar follows the [official Mermaid sequence syntax](https://mermaid.js.org/syntax/sequenceDiagram.html).
See [the sequence example](../examples/sequence-diagrams.md) for complete
request/response, retry, and parallel-work examples.

## Pie charts

````markdown
```mermaid
pie showData title Example operating costs
    "Engineering" : 42.25
    "Operations" : 31.50
    "Support" : 26.25
```
````

The `pie` header is required. `showData` is optional and adds the original values
to the legend. An optional `title` may appear on the header or on its own line
before the data. Each data line contains a unique, nonempty double-quoted label,
a colon, and a positive decimal value with at most two decimal places. Slices
follow source order clockwise from the top. `%%` introduces a comment outside
quoted labels; the same characters inside a label remain text.

Values and totals use integer hundredths. Displayed percentages are rounded to
one decimal place, while the accessible SVG description retains every original
value. Labels and titles are escaped and wrapped; single-slice charts render a
complete circle. PDF uses native vector paths and figure tags, and EPUB declares
its inline SVG in the publication manifest.

Admission is bounded to 64 KiB of diagram source, 64 slices, 512 UTF-8 bytes per
label, and 1024 bytes of title. Each value must fit an unsigned 64-bit integer
after multiplication by 100. Invalid numbers, duplicate labels, extra directives,
or exceeded limits preserve the **entire source block** instead of drawing a
partial or misleading chart. Frontmatter configuration and Mermaid initialization
directives, including donut and legend-position customization, are not supported
and likewise retain source. Browser Flow renders supported charts as native
paths with bundled-font labels and preserves selectable diagram source; see
[the Flow diagram contract](FLOW_DIAGRAMS.md).

The supported pie grammar follows the [official Mermaid pie syntax](https://mermaid.js.org/syntax/pie.html).
See [the example document](../examples/pie-charts.md) for ready-to-render input.
