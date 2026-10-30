// A single trusted bootstrap runs in an opaque-origin sandbox. Generated HTML
// is data in that script, then parsed INERTLY and restricted before insertion.
// No renderer HTML, event handler or navigation executes in the host document.
export function mountBookPreview(doc, scope, markup, channel, fragment) {
  const links = new WeakMap();
  const send = (kind, detail = {}) =>
    scope.parent.postMessage({ schemaVersion: 1, channel, kind, ...detail }, "*");
  const htmlTags = new Set(
    "a abbr article aside b bdi bdo blockquote br caption cite code col colgroup dd del details dfn div dl dt em figcaption figure footer h1 h2 h3 h4 h5 h6 header hr i img input kbd li main mark nav ol p pre q rp rt ruby s samp section small span strong style sub summary sup table tbody td tfoot th thead time tr u ul var wbr".split(
      " ",
    ),
  );
  const svgTags = new Set(
    "svg a g path rect circle ellipse line polyline polygon text tspan defs clippath mask pattern lineargradient radialgradient stop use image title desc symbol".split(
      " ",
    ),
  );
  const mathTags = new Set(
    "math semantics annotation mi mn mo mtext mspace ms mrow mfrac msqrt mroot mstyle merror mpadded mphantom mfenced menclose msub msup msubsup munder mover munderover mmultiscripts mprescripts none mtable mlabeledtr mtr mtd maligngroup malignmark".split(
      " ",
    ),
  );
  const dataImage = (value) => /^data:image\/(?:png|jpeg|gif|webp|svg\+xml)(?:;|,)/i.test(value);
  const anchor = (event) => {
    const element = event.target?.closest?.("a");
    if (!element) return;
    event.preventDefault();
    event.stopPropagation();
    if (links.has(element)) send("link", { href: links.get(element) });
  };
  try {
    const template = doc.createElement("template");
    template.innerHTML = markup;
    const nodes = template.content.querySelectorAll("*");
    if (nodes.length > 50000)
      throw new Error("Preview contains too many elements; export the site instead.");
    let restricted = 0;
    for (const node of nodes) {
      const tag = node.localName.toLowerCase(),
        ns = node.namespaceURI;
      const allowed =
        ns === "http://www.w3.org/1999/xhtml"
          ? htmlTags.has(tag)
          : ns === "http://www.w3.org/2000/svg"
            ? svgTags.has(tag)
            : ns === "http://www.w3.org/1998/Math/MathML" && mathTags.has(tag);
      if (
        !allowed ||
        (tag === "input" && node.getAttribute("type")?.toLowerCase() !== "checkbox")
      ) {
        node.remove();
        restricted++;
        continue;
      }
      for (const attribute of Array.from(node.attributes)) {
        const name = attribute.localName.toLowerCase(),
          value = attribute.value;
        if (name === "href") {
          node.removeAttributeNode(attribute);
          if (tag === "a") {
            if (!links.has(node)) links.set(node, value);
          } else if (tag === "use" && /^#[^\s]+$/.test(value)) node.setAttribute("href", value);
          else if (tag === "image" && dataImage(value)) node.setAttribute("href", value);
          else restricted++;
        } else if (
          name.startsWith("on") ||
          [
            "srcdoc",
            "srcset",
            "ping",
            "target",
            "download",
            "action",
            "autofocus",
            "contenteditable",
            "is",
          ].includes(name) ||
          name.startsWith("form") ||
          (name === "src" && !(tag === "img" && dataImage(value)))
        ) {
          node.removeAttributeNode(attribute);
          restricted++;
        }
      }
      // All anchors become local inert placeholders, including context-menu and
      // middle-click destinations. The original href survives only in WeakMap.
      if (tag === "a") node.setAttribute("href", "#");
      if (tag === "input") node.setAttribute("disabled", "");
    }
    doc.addEventListener("click", anchor, true);
    doc.addEventListener("auxclick", anchor, true);
    doc.addEventListener("submit", (event) => event.preventDefault(), true);
    doc.body.replaceChildren(template.content);
    // Rust emits a quoted BCP-47 language on the document root. Fragment parsing
    // ignores that outer element; restore only a conservative language token.
    const lang = markup.match(/<html\b[^>]*\blang="([A-Za-z0-9-]{1,128})"/i)?.[1];
    if (lang) doc.documentElement.setAttribute("lang", lang);
    let missing = false;
    if (fragment) {
      const destination = doc.getElementById(fragment);
      if (destination) {
        destination.scrollIntoView();
        destination.setAttribute("tabindex", "-1");
        destination.focus({ preventScroll: true });
      } else missing = true;
    }
    send("ready", { restricted });
    if (missing) send("missing-fragment");
  } catch (failure) {
    doc.body.textContent = "Preview could not be displayed. Source and exports are unchanged.";
    send("error", { message: String(failure?.message ?? "Preview failed.").slice(0, 256) });
  }
}
const literal = (value) =>
  JSON.stringify(value)
    .replace(/</g, "\\u003c")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
export function bookPreviewFrame(html, channel, fragment = "") {
  if (
    typeof html !== "string" ||
    html.length > 8 * 1024 * 1024 ||
    !/^[a-f0-9]{32}$/.test(channel) ||
    typeof fragment !== "string" ||
    fragment.length > 4096
  )
    throw new TypeError("Invalid book preview frame input.");
  const csp = `default-src 'none'; script-src 'nonce-${channel}'; script-src-attr 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`;
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"><meta name="referrer" content="no-referrer"></head><body><script nonce="${channel}">(${mountBookPreview.toString()})(document,window,${literal(html)},${literal(channel)},${literal(fragment)});</script></body></html>`;
}
