# Scanner Edge Corpus ###

Title
-----

Paragraph with foo_bar_baz, ***bold em***, escaped \*stars\*, literal \[brackets\], and <span data-x="1">raw html</span>.

See https://example.com/a(b)., www.example.org/path?q=1, and <team@example.com>.

| Key | Expr | Note |
|---|:---:|---:|
| pipe | a \| b | `x|y` |
| tick | \` | ``a|b`` |

> - quoted
>   1. nested
> continuation

-	tabbed unordered item
1.	ordered tab item
    2. indented ordered marker stays inside

[ok]: /target "Target"
[logo]: /logo.png 'Logo'
[bad]:

Use [ok], [missing], and ![Logo][logo].

```text
inside
   ```

    ```rust
    let x = 1;
    ```

---
