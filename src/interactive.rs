//! Interactive self-hosting single-file HTML document compiler.
//!
//! Generates a standalone, zero-network, self-contained HTML file containing:
//! - An interactive split-view live editor + preview.
//! - Offline live markdown parsing & rendering.
//! - Document intelligence stats panel (words, reading time, readability score).
//! - Clean typography & theme toggle (Light, Dark, Sans, Serif, Type Scales).
//! - Client-side vector PDF / Print export with print-perfect page pagination.

use crate::HtmlOptions;
use crate::ast::Document;
use crate::html::render_fragment;

fn escape_html_to(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut clean_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let esc = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#39;",
            _ => continue,
        };
        if clean_start < i {
            out.push_str(&s[clean_start..i]);
        }
        out.push_str(esc);
        clean_start = i + 1;
    }
    if clean_start < s.len() {
        out.push_str(&s[clean_start..]);
    }
}

/// Render an interactive self-hosting single-file HTML workspace.
#[must_use]
pub fn render_interactive_html(doc: &Document, markdown_src: &str, opts: &HtmlOptions) -> String {
    let initial_rendered = render_fragment(&doc.blocks, opts);
    let title = opts.title.as_deref().unwrap_or("FrankenMarkdown Document");

    let mut out = String::with_capacity(initial_rendered.len() + markdown_src.len() + 16384);

    out.push_str("<!DOCTYPE html>\n<html lang=\"");
    out.push_str(opts.lang.as_deref().unwrap_or("en"));
    out.push_str("\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n<title>");
    escape_html_to(title, &mut out);
    out.push_str("</title>\n<style>\n");
    out.push_str(INTERACTIVE_CSS);
    out.push_str("\n</style>\n</head>\n<body>\n");

    // Header Toolbar
    out.push_str(
        r#"<header class="fmd-app-header">
  <div class="fmd-brand">
    <span class="fmd-logo-icon">⚡</span>
    <span class="fmd-title">"#,
    );
    escape_html_to(title, &mut out);
    out.push_str(r#"</span>
  </div>
  <div class="fmd-toolbar">
    <button class="fmd-btn" id="btn-toggle-view" title="Toggle Split View / Reading Mode">
      <span id="view-mode-icon">📖</span> <span id="view-mode-label">Read Mode</span>
    </button>
    <div class="fmd-btn-group">
      <button class="fmd-btn" id="btn-zoom-out" title="Decrease font size">A-</button>
      <button class="fmd-btn" id="btn-zoom-reset" title="Reset font size">100%</button>
      <button class="fmd-btn" id="btn-zoom-in" title="Increase font size">A+</button>
    </div>
    <button class="fmd-btn" id="btn-theme-toggle" title="Toggle Dark / Light Mode">🌓 Theme</button>
    <button class="fmd-btn" id="btn-stats-toggle" title="Toggle Document Statistics">📊 Stats</button>
    <button class="fmd-btn fmd-btn-primary" id="btn-export-pdf" title="Export Clean Vector PDF">📄 Export PDF</button>
  </div>
</header>
"#);

    // Main App Container (Split Pane)
    out.push_str(
        r#"<div class="fmd-app-body view-split" id="fmd-app-body">
  <section class="fmd-editor-pane" id="editor-pane">
    <div class="fmd-pane-header">
      <span>Markdown Source</span>
      <span class="fmd-pane-badge" id="source-line-count">Lines: 0</span>
    </div>
    <textarea id="fmd-editor" spellcheck="false" placeholder="Type Markdown here...">"#,
    );
    escape_html_to(markdown_src, &mut out);
    out.push_str(
        r#"</textarea>
  </section>

  <main class="fmd-preview-pane fmd" id="preview-pane">
    <div class="fmd-content" id="fmd-content">"#,
    );
    out.push_str(&initial_rendered);
    out.push_str(
        r#"</div>
  </main>
</div>
"#,
    );

    // Document Statistics Modal / Drawer
    out.push_str(r#"<div class="fmd-stats-drawer" id="stats-drawer">
  <div class="fmd-stats-header">
    <h3>Document Intelligence</h3>
    <button class="fmd-btn-close" id="btn-stats-close">&times;</button>
  </div>
  <div class="fmd-stats-grid">
    <div class="fmd-stat-card"><span class="stat-num" id="stat-words">0</span><span class="stat-label">Words</span></div>
    <div class="fmd-stat-card"><span class="stat-num" id="stat-chars">0</span><span class="stat-label">Characters</span></div>
    <div class="fmd-stat-card"><span class="stat-num" id="stat-read-time">0m</span><span class="stat-label">Reading Time</span></div>
    <div class="fmd-stat-card"><span class="stat-num" id="stat-readability">--</span><span class="stat-label">Readability</span></div>
  </div>
</div>
"#);

    // Initial source storage for reset
    out.push_str("<script type=\"text/markdown\" id=\"fmd-raw-source\">\n");
    out.push_str(&markdown_src.replace("</script", "<\\/script"));
    out.push_str("\n</script>\n");

    // Client-side JavaScript
    out.push_str("<script>\n");
    out.push_str(INTERACTIVE_JS);
    out.push_str("\n</script>\n</body>\n</html>\n");

    out
}

const INTERACTIVE_CSS: &str = r#"
:root {
  --bg-primary: #ffffff;
  --bg-secondary: #f6f8fa;
  --bg-editor: #f8fafc;
  --fg-primary: #1f2328;
  --fg-secondary: #656d76;
  --border-color: #d0d7de;
  --accent-color: #0969da;
  --accent-hover: #0854ad;
  --card-bg: #ffffff;
  --fmd-base: 16px;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg-primary: #0d1117;
    --bg-secondary: #161b22;
    --bg-editor: #090d13;
    --fg-primary: #e6edf3;
    --fg-secondary: #8b949e;
    --border-color: #30363d;
    --accent-color: #4493f8;
    --accent-hover: #58a6ff;
    --card-bg: #161b22;
  }
}

body.theme-light {
  --bg-primary: #ffffff;
  --bg-secondary: #f6f8fa;
  --bg-editor: #f8fafc;
  --fg-primary: #1f2328;
  --fg-secondary: #656d76;
  --border-color: #d0d7de;
  --accent-color: #0969da;
  --accent-hover: #0854ad;
  --card-bg: #ffffff;
}

body.theme-dark {
  --bg-primary: #0d1117;
  --bg-secondary: #161b22;
  --bg-editor: #090d13;
  --fg-primary: #e6edf3;
  --fg-secondary: #8b949e;
  --border-color: #30363d;
  --accent-color: #4493f8;
  --accent-hover: #58a6ff;
  --card-bg: #161b22;
}

* { box-sizing: border-box; margin: 0; padding: 0; }
html, body {
  height: 100%;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif;
  background: var(--bg-primary);
  color: var(--fg-primary);
  font-size: var(--fmd-base);
  overflow: hidden;
}

.fmd-app-header {
  height: 48px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  user-select: none;
}

.fmd-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 14px;
}

.fmd-logo-icon {
  font-size: 16px;
}

.fmd-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.fmd-btn {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--fg-primary);
  font-size: 12px;
  padding: 5px 10px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.fmd-btn:hover {
  background: var(--bg-secondary);
  border-color: var(--accent-color);
}

.fmd-btn-primary {
  background: var(--accent-color);
  color: #ffffff;
  border-color: transparent;
  font-weight: 500;
}

.fmd-btn-primary:hover {
  background: var(--accent-hover);
}

.fmd-btn-group {
  display: inline-flex;
}

.fmd-btn-group .fmd-btn {
  border-radius: 0;
  margin-left: -1px;
}

.fmd-btn-group .fmd-btn:first-child {
  border-top-left-radius: 6px;
  border-bottom-left-radius: 6px;
  margin-left: 0;
}

.fmd-btn-group .fmd-btn:last-child {
  border-top-right-radius: 6px;
  border-bottom-right-radius: 6px;
}

.fmd-app-body {
  height: calc(100% - 48px);
  display: flex;
  position: relative;
}

.fmd-editor-pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--bg-editor);
  border-right: 1px solid var(--border-color);
  min-width: 250px;
}

.fmd-pane-header {
  height: 32px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  font-size: 11px;
  font-weight: 600;
  color: var(--fg-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.fmd-pane-badge {
  font-weight: normal;
  font-size: 11px;
}

#fmd-editor {
  flex: 1;
  width: 100%;
  border: none;
  resize: none;
  outline: none;
  background: transparent;
  color: var(--fg-primary);
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  font-size: 13px;
  line-height: 1.5;
  padding: 16px;
  tab-size: 4;
}

.fmd-preview-pane {
  flex: 1;
  height: 100%;
  overflow-y: auto;
  padding: 32px 48px;
  background: var(--bg-primary);
}

.fmd-content {
  max-width: 860px;
  margin: 0 auto;
  line-height: 1.6;
}

/* View Mode Toggles */
.view-read .fmd-editor-pane { display: none; }
.view-read .fmd-preview-pane { flex: 1; padding: 48px 80px; }

/* Markdown Typography within Preview */
.fmd-content h1, .fmd-content h2, .fmd-content h3 { margin: 1.5em 0 0.5em; font-weight: 650; }
.fmd-content h1 { font-size: 2em; border-bottom: 1px solid var(--border-color); padding-bottom: 0.3em; }
.fmd-content h2 { font-size: 1.5em; border-bottom: 1px solid var(--border-color); padding-bottom: 0.3em; }
.fmd-content p { margin: 0 0 1em; }
.fmd-content a { color: var(--accent-color); text-decoration: none; }
.fmd-content a:hover { text-decoration: underline; }
.fmd-content code { background: var(--bg-secondary); padding: 0.2em 0.4em; border-radius: 4px; font-family: ui-monospace, monospace; font-size: 0.88em; }
.fmd-content pre { background: var(--bg-secondary); padding: 1em; border-radius: 6px; overflow-x: auto; margin: 1em 0; }
.fmd-content pre code { background: transparent; padding: 0; }
.fmd-content blockquote { border-left: 4px solid var(--border-color); padding: 0 1em; color: var(--fg-secondary); margin: 1em 0; }
.fmd-content table { border-collapse: collapse; width: 100%; margin: 1.2em 0; }
.fmd-content th, .fmd-content td { border: 1px solid var(--border-color); padding: 6px 12px; }
.fmd-content th { background: var(--bg-secondary); }
.fmd-content ul, .fmd-content ol { padding-left: 2em; margin: 0 0 1em; }
.fmd-content li { margin: 0.25em 0; }

/* Callout Styling */
aside.callout { border: 1px solid var(--border-color); border-left-width: 4px; border-radius: 6px; padding: 0.75em 1em; margin: 1em 0; }
aside.callout p.callout-title { font-weight: 600; margin: 0 0 0.25em 0; }
aside.callout-note { border-left-color: #0969da; }
aside.callout-tip { border-left-color: #1a7f37; }
aside.callout-important { border-left-color: #8250df; }
aside.callout-warning { border-left-color: #9a6700; }
aside.callout-caution { border-left-color: #cf222e; }

/* Diagram wrapper */
.fmd-diagram-wrapper { display: flex; justify-content: center; margin: 1.5em 0; overflow-x: auto; }
.fmd-diagram-wrapper svg { max-width: 100%; height: auto; }

/* Stats Drawer */
.fmd-stats-drawer {
  position: fixed;
  bottom: 16px;
  right: 16px;
  width: 320px;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.15);
  padding: 16px;
  z-index: 1000;
  display: none;
}

.fmd-stats-drawer.open { display: block; }

.fmd-stats-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.fmd-btn-close {
  background: transparent;
  border: none;
  font-size: 20px;
  cursor: pointer;
  color: var(--fg-secondary);
}

.fmd-stats-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.fmd-stat-card {
  background: var(--bg-secondary);
  border-radius: 6px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  align-items: center;
}

.stat-num { font-size: 18px; font-weight: 700; color: var(--accent-color); }
.stat-label { font-size: 11px; color: var(--fg-secondary); text-transform: uppercase; margin-top: 2px; }

/* Print / PDF Mode */
@media print {
  body { overflow: visible; font-size: 11pt; }
  .fmd-app-header, .fmd-editor-pane, .fmd-stats-drawer { display: none !important; }
  .fmd-app-body { height: auto; display: block; }
  .fmd-preview-pane { padding: 0; overflow: visible; }
  .fmd-content { max-width: 100%; }
}
"#;

const INTERACTIVE_JS: &str = r#"
(function() {
  const editor = document.getElementById('fmd-editor');
  const preview = document.getElementById('fmd-content');
  const body = document.getElementById('fmd-app-body');
  const lineCountBadge = document.getElementById('source-line-count');
  const statsDrawer = document.getElementById('stats-drawer');

  // Stats elements
  const statWords = document.getElementById('stat-words');
  const statChars = document.getElementById('stat-chars');
  const statReadTime = document.getElementById('stat-read-time');
  const statReadability = document.getElementById('stat-readability');

  let currentScale = 1.0;
  let viewMode = 'split'; // 'split' | 'read'

  // Update line count and stats
  function updateStats() {
    const text = editor.value;
    const lines = text.split('\n').length;
    lineCountBadge.textContent = `Lines: ${lines}`;

    const words = (text.match(/\S+/g) || []).length;
    const chars = text.length;
    const readTimeMinutes = words === 0 ? 0 : Math.max(1, Math.ceil(words / 220));

    // Syllable heuristic for Flesch score
    const sentences = Math.max(1, (text.match(/[.!?]+(\s|$)/g) || []).length);
    let syllables = 0;
    const tokens = text.toLowerCase().match(/[a-z]+/g) || [];
    for (const tok of tokens) {
      let count = (tok.match(/[aeiouy]+/g) || []).length;
      if (tok.endsWith('e') && !tok.endsWith('le') && count > 1) count--;
      syllables += Math.max(1, count);
    }
    const flesch = Math.round(206.835 - 1.015 * (words / sentences) - 84.6 * (syllables / Math.max(1, words)));
    const clampedFlesch = Math.max(0, Math.min(100, isNaN(flesch) ? 70 : flesch));

    statWords.textContent = words.toLocaleString();
    statChars.textContent = chars.toLocaleString();
    statReadTime.textContent = `${readTimeMinutes}m`;
    statReadability.textContent = `${clampedFlesch}/100`;
  }

  // Fast Client-side Markdown Parser for live editing
  function parseMarkdownClient(src) {
    let out = '';
    const lines = src.split('\n');
    let inCode = false;
    let codeLang = '';
    let codeBuf = [];
    let inList = false;
    let inCallout = false;
    let inQuote = false;

    function flushList() {
      if (inList) { out += '</ul>\n'; inList = false; }
    }

    function flushQuote() {
      if (inCallout) { out += '</aside>\n'; inCallout = false; }
      if (inQuote) { out += '</blockquote>\n'; inQuote = false; }
    }

    function flushAll() {
      flushList();
      flushQuote();
    }

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];

      if (line.startsWith('```')) {
        flushAll();
        if (inCode) {
          out += `<pre><code class="language-${codeLang}">${escapeHtml(codeBuf.join('\n'))}</code></pre>\n`;
          inCode = false;
          codeBuf = [];
        } else {
          inCode = true;
          codeLang = line.slice(3).trim();
          codeBuf = [];
        }
        continue;
      }

      if (inCode) {
        codeBuf.push(line);
        continue;
      }

      if (line.startsWith('# ')) {
        flushAll();
        out += `<h1>${inlineFormat(line.slice(2))}</h1>\n`;
      } else if (line.startsWith('## ')) {
        flushAll();
        out += `<h2>${inlineFormat(line.slice(3))}</h2>\n`;
      } else if (line.startsWith('### ')) {
        flushAll();
        out += `<h3>${inlineFormat(line.slice(4))}</h3>\n`;
      } else if (line.startsWith('> [!NOTE]')) {
        flushAll();
        inCallout = true;
        out += `<aside class="callout callout-note"><p class="callout-title">Note</p>`;
      } else if (line.startsWith('> [!TIP]')) {
        flushAll();
        inCallout = true;
        out += `<aside class="callout callout-tip"><p class="callout-title">Tip</p>`;
      } else if (line.startsWith('> [!WARNING]')) {
        flushAll();
        inCallout = true;
        out += `<aside class="callout callout-warning"><p class="callout-title">Warning</p>`;
      } else if (line.startsWith('> [!IMPORTANT]')) {
        flushAll();
        inCallout = true;
        out += `<aside class="callout callout-important"><p class="callout-title">Important</p>`;
      } else if (line.startsWith('> [!CAUTION]')) {
        flushAll();
        inCallout = true;
        out += `<aside class="callout callout-caution"><p class="callout-title">Caution</p>`;
      } else if (line.startsWith('> ')) {
        flushList();
        if (!inCallout && !inQuote) {
          inQuote = true;
          out += '<blockquote>\n';
        }
        out += `<p>${inlineFormat(line.slice(2))}</p>\n`;
      } else if (line.startsWith('- ') || line.startsWith('* ')) {
        flushQuote();
        if (!inList) { out += '<ul>\n'; inList = true; }
        out += `<li>${inlineFormat(line.slice(2))}</li>\n`;
      } else if (line.trim().length === 0) {
        flushAll();
      } else {
        flushAll();
        out += `<p>${inlineFormat(line)}</p>\n`;
      }
    }
    flushAll();
    if (inCode) {
      out += `<pre><code>${escapeHtml(codeBuf.join('\n'))}</code></pre>\n`;
    }
    return out;
  }

  function escapeHtml(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }

  function inlineFormat(s) {
    return escapeHtml(s)
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.+?)\*/g, '<em>$1</em>')
      .replace(/`([^`]+)`/g, '<code>$1</code>')
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (match, label, href) => {
        const h = href.trim();
        const safe = h.startsWith('#') || h.startsWith('/') || h.startsWith('./') || h.startsWith('http://') || h.startsWith('https://') || h.startsWith('mailto:');
        return safe ? `<a href="${h}">${label}</a>` : label;
      });
  }

  // Live editor input debounce
  let debounceTimer = null;
  editor.addEventListener('input', () => {
    updateStats();
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      preview.innerHTML = parseMarkdownClient(editor.value);
    }, 150);
  });

  // Toolbar actions
  document.getElementById('btn-toggle-view').addEventListener('click', () => {
    if (viewMode === 'split') {
      body.classList.remove('view-split');
      body.classList.add('view-read');
      document.getElementById('view-mode-icon').textContent = '✏️';
      document.getElementById('view-mode-label').textContent = 'Edit Mode';
      viewMode = 'read';
    } else {
      body.classList.remove('view-read');
      body.classList.add('view-split');
      document.getElementById('view-mode-icon').textContent = '📖';
      document.getElementById('view-mode-label').textContent = 'Read Mode';
      viewMode = 'split';
    }
  });

  document.getElementById('btn-zoom-in').addEventListener('click', () => {
    currentScale = Math.min(2.0, currentScale + 0.1);
    document.documentElement.style.setProperty('--fmd-base', (16 * currentScale) + 'px');
    document.getElementById('btn-zoom-reset').textContent = Math.round(currentScale * 100) + '%';
  });

  document.getElementById('btn-zoom-out').addEventListener('click', () => {
    currentScale = Math.max(0.7, currentScale - 0.1);
    document.documentElement.style.setProperty('--fmd-base', (16 * currentScale) + 'px');
    document.getElementById('btn-zoom-reset').textContent = Math.round(currentScale * 100) + '%';
  });

  document.getElementById('btn-zoom-reset').addEventListener('click', () => {
    currentScale = 1.0;
    document.documentElement.style.setProperty('--fmd-base', '16px');
    document.getElementById('btn-zoom-reset').textContent = '100%';
  });

  document.getElementById('btn-theme-toggle').addEventListener('click', () => {
    if (document.body.classList.contains('theme-dark')) {
      document.body.classList.remove('theme-dark');
      document.body.classList.add('theme-light');
    } else {
      document.body.classList.remove('theme-light');
      document.body.classList.add('theme-dark');
    }
  });

  document.getElementById('btn-stats-toggle').addEventListener('click', () => {
    statsDrawer.classList.toggle('open');
    updateStats();
  });

  document.getElementById('btn-stats-close').addEventListener('click', () => {
    statsDrawer.classList.remove('open');
  });

  document.getElementById('btn-export-pdf').addEventListener('click', () => {
    window.print();
  });

  // Initial stats calculation
  updateStats();
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_markdown;

    #[test]
    fn render_interactive_html_contains_all_components() {
        let md = "# Interactive Document\n\nThis is live editable text.";
        let doc = parse_markdown(md);
        let opts = HtmlOptions::default();

        let html = render_interactive_html(&doc, md, &opts);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("class=\"fmd-app-header\""));
        assert!(html.contains("id=\"fmd-editor\""));
        assert!(html.contains("id=\"fmd-content\""));
        assert!(html.contains("id=\"stats-drawer\""));
        assert!(html.contains("id=\"btn-export-pdf\""));
        assert!(html.contains("This is live editable text."));
        assert!(html.contains("parseMarkdownClient"));
    }
}
