# Escape-heavy scanner corpus

This fixture packs HTML-special bytes into running text, attributes, and code
so the escape scanners cannot stay on the clean-copy path.

Ampersands & angle brackets <like this> and quotes "quoted" appear in almost
every sentence. A comparison such as `a < b && b > c` plus an attribute
`title="x & y"` forces `find_html_text_escape` and `find_html_escape` to stop
early and often.

## Mixed markup

Use `<span class="warn">` only as example text; the renderer must escape it by
default. The string `foo &amp; bar <baz>` is the kind of hostile input that
makes a naive copy unsafe. Quotes show up in `key="value"` and in
`say "hello"`.

| col | note |
|---|---|
| a < b | comparison |
| x & y | conjunction |
| "q" | quoted |

```html
<div class="x" data-title="A & B">
  <p>1 < 2 && 2 > 1</p>
</div>
```

Link-like noise: <http://example.test/a?b=1&c=2> and `click <here>`.

## Repeated hostile runs

&&&& <<<< >>>> """" &&&& <<<< >>>> """"
foo & bar < baz > qux "zip"
alpha < beta & gamma > delta "epsilon"
`code with <tags> & "quotes"`
![alt <x>](http://example.test/a.png?x=1&y=2)
[text & more](http://example.test/?a=1&b=2 "title & quotes")
