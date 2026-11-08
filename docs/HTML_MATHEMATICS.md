# Mathematics in HTML and EPUB

HTML renders supported TeX formulas as native MathML using the shared
`fmd-math` parser. EPUB uses the same MathML in its XHTML chapters. The output
does not need a JavaScript math renderer or a remote typesetting service.
Single-dollar inline formulas, inline display-style formulas and standalone
display blocks share this serializer.

## Text inside mathematics

Text commands can contain mathematical conditions and nested formatting. Those
parts remain separate semantic MathML nodes, so the browser and reading system
receive the complete expression:

```markdown
$$
x=1\text{ if $n_1>0$ and $m^2=4$}
$$

$$
\textbf{Case $k_j$: \textit{positive} \& <proof>}
$$
```

The conditions `n_1>0` and `m^2=4` remain subscript, superscript and comparison
nodes inside the text box. Nested text fonts, underlining, colors, size changes
and explicit spaces such as `\hspace{2em}` keep their own markup. Literal text
and characters are escaped, so `<proof>` is displayed as text.

A mathematical island inside styled text resumes ordinary mathematical
alphabets. For example, `\textbf{Case $x$: positive}` has bold prose and a normal
mathematical `x`; the bold text resumes after the island. Explicit mathematical
font commands inside the island still apply normally.

An island also follows its mathematical context: scripts and fraction parts
retain their smaller styles. `$...$` uses text style and `$$...$$` uses display
style when the surrounding context is not already smaller. Style-sensitive
constructs such as `\mathchoice` therefore keep the same authored branch as
the vector math layout engine.

Ordinary literal text boxes retain the compact single-`mtext` output used
previously. Extra structural markup is used when flattening the text would
lose content, formatting, spacing or mathematical context. Buffered HTML,
streamed HTML, HTML fragments, browser adapters and single-document or book
EPUBs use the same implementation. [Explicit equation labels](DISPLAY_EQUATION_LABELS.md)
also use this shared text path.

## Validation

`cargo test -p fmd-math --test mathml_text_semantics` covers mathematical content
inside text, nested font restoration, math-island font resets, color and size
scope, explicit spacing, underlining, escaping, plain-output byte retention
and style-sensitive branches. The existing `mathml_box_check` suite includes
text-island formulas and compares their MathML atoms with the actual vector
layout's glyph atoms.

`cargo test --test mathml_text_semantics_test` exercises the public buffered,
streamed, fragment and browser adapter paths. When Python 3 is installed, it
also independently inflates single-document and book EPUBs and parses their
chapter XML to verify that the formulas and nested styles survived packaging.
