"""Real Chromium inline mathematics over native and optional actual WASM sessions.

Create the fixture by running tests/flow_inline_math.rs with the explicit
FMD_FLOW_INLINE_MATH_FIXTURE environment variable. No browser runtime dependency or
generated-WASM substitute is installed by this test.
"""
import argparse
import base64
import hashlib
import json
import shutil
from pathlib import Path

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--fixture", type=Path, required=True)
parser.add_argument("--generated-package", type=Path,
    help="optional real wasm-bindgen web output directory; executes public createFlowSession")
parser.add_argument("--chromium", default=shutil.which("chromium") or shutil.which("google-chrome"))
args = parser.parse_args()
if not args.chromium:
    parser.error("pass --chromium with an installed Chromium executable")
root = Path(__file__).resolve().parents[1]
names = ["flow_session.mjs", "flow_outlines.mjs", "flow-canvas.js", "flow_reading.mjs", "flow-reader.js", "tests/flow_inline_math.browser.mjs"]
generated = None
generated_evidence = None
if args.generated_package:
    names += ["flow.js", "flow_asset_batch.mjs", "flow_export.mjs", "franken_markdown.js", "pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"]
    binding_bytes = (args.generated_package / "franken_markdown.js").read_bytes()
    wasm_path = args.generated_package / "franken_markdown_bg.wasm"
    length = wasm_path.stat().st_size
    if length > 128 * 1024 * 1024:
        parser.error("generated WASM exceeds the 128 MiB test ingress bound")
    with wasm_path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    generated = {"bindings": binding_bytes.decode(), "wasmBytes": length, "wasmSha256": digest}
    generated_evidence = {"bindingsSha256": hashlib.sha256(binding_bytes).hexdigest(),
        "wasmSha256": digest, "wasmBytes": length}
modules = {name: (root / name).read_text() for name in names}
fixture_bytes = args.fixture.read_bytes()
fixture = json.loads(fixture_bytes)
requests, errors = [], []


def upload_wasm(page, path, generated):
    # Keep each transport message bounded; the browser owns one exact-sized
    # byte buffer instead of three processes retaining a whole base64 module.
    length = generated["wasmBytes"]
    page.evaluate("length => { globalThis.__fmdFlowGateWasm = {bytes: new Uint8Array(length), offset: 0}; }", length)
    digest = hashlib.sha256()
    offset = 0
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            if offset + len(chunk) > length:
                raise ValueError("generated WASM changed length while streaming")
            digest.update(chunk)
            page.evaluate("""({offset, chunk}) => {
                const owned = globalThis.__fmdFlowGateWasm, binary = atob(chunk);
                if (owned.offset !== offset || offset + binary.length > owned.bytes.length)
                    throw Error("nonsequential generated WASM ingress");
                for (let i = 0; i < binary.length; i++) owned.bytes[offset + i] = binary.charCodeAt(i);
                owned.offset += binary.length;
            }""", {"offset": offset, "chunk": base64.b64encode(chunk).decode("ascii")})
            offset += len(chunk)
    if offset != length or digest.hexdigest() != generated["wasmSha256"]:
        raise ValueError("generated WASM changed while streaming")
    page.evaluate("""length => {
        const owned = globalThis.__fmdFlowGateWasm;
        if (owned.offset !== length || owned.bytes.length !== length)
            throw Error("incomplete generated WASM ingress");
    }""", length)


with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=args.chromium, headless=True, timeout=10000,
        args=["--no-sandbox", "--disable-dev-shm-usage"])
    page = browser.new_page()
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("request", lambda request: requests.append(request.url) if request.url.startswith(("http:", "https:")) else None)
    page.route("http://**/*", lambda route: route.abort())
    page.route("https://**/*", lambda route: route.abort())
    page.set_content("<!doctype html><title>Native Flow inline math proof</title>")
    if generated:
        upload_wasm(page, wasm_path, generated)
    result = page.evaluate("""async ({modules, fixture, generated}) => {
      const urls=[];
      const load=source=>{const url=URL.createObjectURL(new Blob([source],{type:'text/javascript'}));urls.push(url);return url;};
      try {
        const session=load(modules['flow_session.mjs']);
        const outlines=load(modules['flow_outlines.mjs'].replaceAll('"./flow_session.mjs"',JSON.stringify(session)));
        const canvas=load(modules['flow-canvas.js'].replaceAll('"./flow_session.mjs"',JSON.stringify(session)).replaceAll('"./flow_outlines.mjs"',JSON.stringify(outlines)));
        const reading=load(modules['flow_reading.mjs']);
        const reader=load(modules['flow-reader.js'].replaceAll('"./flow_reading.mjs"',JSON.stringify(reading)));
        const suite=load(modules['tests/flow_inline_math.browser.mjs'].replaceAll('"../flow-canvas.js"',JSON.stringify(canvas)).replaceAll('"../flow-reader.js"',JSON.stringify(reader)).replaceAll('"../flow_session.mjs"',JSON.stringify(session)));
        let runtime=null;
        if (generated) {
          const bindings=load(generated.bindings);
          const pages=load(modules['pdf_page.mjs']), running=load(modules['pdf_running.mjs']), typography=load(modules['render_typography.mjs']);
          const replace=source=>source.replaceAll('"./flow_session.mjs"',JSON.stringify(session))
            .replaceAll('"./pdf_page.mjs"',JSON.stringify(pages)).replaceAll('"./pdf_running.mjs"',JSON.stringify(running))
            .replaceAll('"./render_typography.mjs"',JSON.stringify(typography)).replaceAll('"./pkg/franken_markdown.js"',JSON.stringify(bindings));
          const exports=load(replace(modules['flow_export.mjs'])), assets=load(replace(modules['flow_asset_batch.mjs']));
          const api=load(replace(modules['franken_markdown.js']));
          const flow=load(replace(modules['flow.js']).replaceAll('"./flow_outlines.mjs"',JSON.stringify(outlines))
            .replaceAll('"./flow_export.mjs"',JSON.stringify(exports)).replaceAll('"./flow_asset_batch.mjs"',JSON.stringify(assets))
            .replaceAll('"./franken_markdown.js"',JSON.stringify(api)));
          runtime=await import(flow);
          const owned = globalThis.__fmdFlowGateWasm;
          delete globalThis.__fmdFlowGateWasm;
          if (!owned || owned.offset !== generated.wasmBytes || owned.bytes.length !== generated.wasmBytes)
            throw Error("incomplete generated WASM ingress");
          await runtime.init(owned.bytes);
        }
        return await (await import(suite)).run(fixture,runtime);
      } finally {for (const url of urls) URL.revokeObjectURL(url);}
    }""", {"modules": modules, "fixture": fixture, "generated": generated})
    result["browser"] = browser.version
    browser.close()
result["fixtureSha256"] = hashlib.sha256(fixture_bytes).hexdigest()
result["moduleSha256"] = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in names}
result["generatedWasm"] = generated_evidence
result["networkRequests"] = requests
result["pageErrors"] = errors
print(json.dumps(result, indent=2), flush=True)
if result["failed"] or requests or errors:
    raise SystemExit(1)
