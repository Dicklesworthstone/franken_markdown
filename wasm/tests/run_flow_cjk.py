"""Real Chromium CJK acceptance over native and actual generated Flow sessions.

Emit the native fixture with FMD_FLOW_CJK_FIXTURE in tests/flow_cjk.rs. Pass
--generated-package for the cjk-face build, --lean-package for the matching
feature-off build, and --baseline-package for an actual pre-CJK runtime. The
baseline compares ordinary Latin pixels, never substitutes CJK glyphs or shaping.
"""
import argparse
import base64
import hashlib
import gzip
import json
import shutil
from pathlib import Path

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--fixture", type=Path, required=True)
parser.add_argument("--generated-package", type=Path, help="actual wasm-flow,cjk-face wasm-bindgen web package")
parser.add_argument("--lean-package", type=Path, help="actual wasm-flow package without cjk-face")
parser.add_argument("--baseline-package", type=Path, help="actual pre-change package for ordinary Latin pixel parity")
parser.add_argument("--chromium", default=shutil.which("chromium") or shutil.which("google-chrome"))
args = parser.parse_args()
if not args.chromium:
    parser.error("pass --chromium with an installed Chromium executable")
root = Path(__file__).resolve().parents[1]
names = ["flow_session.mjs", "flow_outlines.mjs", "flow-canvas.js", "flow_reading.mjs", "flow-reader.js", "tests/flow_cjk.browser.mjs"]
if args.generated_package or args.lean_package or args.baseline_package:
    names += ["flow.js", "flow_asset_batch.mjs", "flow_export.mjs", "franken_markdown.js", "pdf_page.mjs", "pdf_running.mjs", "render_typography.mjs"]
modules = {name: (root / name).read_text() for name in names}
fixture_bytes = args.fixture.read_bytes()
fixture = json.loads(fixture_bytes)


def package(path):
    if path is None:
        return None, None
    bindings = (path / "franken_markdown.js").read_bytes()
    wasm_path = path / "franken_markdown_bg.wasm"
    length = wasm_path.stat().st_size
    if length > 128 * 1024 * 1024:
        parser.error("generated WASM exceeds the 128 MiB test ingress bound")
    with wasm_path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"bindings": bindings.decode(), "wasmBytes": length, "wasmSha256": digest}, {
        "bindingsSha256": hashlib.sha256(bindings).hexdigest(),
        "wasmSha256": digest, "wasmBytes": length}


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


evaluate = """async ({modules, fixture, generated, reference, mode}) => {
  const urls=[];
  const load=source=>{const url=URL.createObjectURL(new Blob([source],{type:'text/javascript'}));urls.push(url);return url;};
  try {
    const session=load(modules['flow_session.mjs']);
    const outlines=load(modules['flow_outlines.mjs'].replaceAll('"./flow_session.mjs"',JSON.stringify(session)));
    const canvas=load(modules['flow-canvas.js'].replaceAll('"./flow_session.mjs"',JSON.stringify(session)).replaceAll('"./flow_outlines.mjs"',JSON.stringify(outlines)));
    const reading=load(modules['flow_reading.mjs']);
    const reader=load(modules['flow-reader.js'].replaceAll('"./flow_reading.mjs"',JSON.stringify(reading)));
    const suite=load(modules['tests/flow_cjk.browser.mjs'].replaceAll('"../flow-canvas.js"',JSON.stringify(canvas)).replaceAll('"../flow-reader.js"',JSON.stringify(reader)).replaceAll('"../flow_session.mjs"',JSON.stringify(session)));
    async function runtime(bytes) {
      if (!bytes) return null;
      // Each runtime owns distinct generated bindings and facade modules, so
      // initializing the feature-off or baseline package cannot replace another.
      const bindings=load(bytes.bindings);
      const pages=load(modules['pdf_page.mjs']),running=load(modules['pdf_running.mjs']),typography=load(modules['render_typography.mjs']);
      const replace=source=>source.replaceAll('"./flow_session.mjs"',JSON.stringify(session))
        .replaceAll('"./pdf_page.mjs"',JSON.stringify(pages)).replaceAll('"./pdf_running.mjs"',JSON.stringify(running))
        .replaceAll('"./render_typography.mjs"',JSON.stringify(typography)).replaceAll('"./pkg/franken_markdown.js"',JSON.stringify(bindings));
      const exports=load(replace(modules['flow_export.mjs'])),assets=load(replace(modules['flow_asset_batch.mjs']));
      const api=load(replace(modules['franken_markdown.js']));
      const flow=load(replace(modules['flow.js']).replaceAll('"./flow_outlines.mjs"',JSON.stringify(outlines))
        .replaceAll('"./flow_export.mjs"',JSON.stringify(exports)).replaceAll('"./flow_asset_batch.mjs"',JSON.stringify(assets))
        .replaceAll('"./franken_markdown.js"',JSON.stringify(api)));
      const value=await import(flow);
      const owned=globalThis.__fmdFlowGateWasm;
      delete globalThis.__fmdFlowGateWasm;
      if(!owned || owned.offset!==bytes.wasmBytes || owned.bytes.length!==bytes.wasmBytes)
        throw Error("incomplete generated WASM ingress");
      await value.init(owned.bytes);
      return value;
    }
    const test=await import(suite), actual=await runtime(generated);
    return mode==="baseline"
      ? await test.captureLatinBaseline(actual,fixture.original.snapshot.options)
      : await test.run(fixture,actual,reference,mode);
  } finally {for(const url of urls)URL.revokeObjectURL(url);}
}"""

requests, errors, phases, generated_evidence = [], [], [], {}
reference = None
reference_sha = None
with sync_playwright() as p:
    def phase(mode, generated, reference, directory):
        # Closing the whole owned process releases compiled WASM and caches
        # before the next runtime, even under shared host memory pressure.
        browser = p.chromium.launch(executable_path=args.chromium, headless=True, timeout=10000,
            args=["--no-sandbox", "--disable-dev-shm-usage"])
        try:
            context = browser.new_context()
            page = context.new_page()
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("request", lambda request: requests.append(request.url) if request.url.startswith(("http:", "https:")) else None)
            page.route("http://**/*", lambda route: route.abort())
            page.route("https://**/*", lambda route: route.abort())
            page.set_content("<!doctype html><title>Actual Flow CJK proof</title>")
            if generated:
                upload_wasm(page, directory / "franken_markdown_bg.wasm", generated)
            result = page.evaluate(evaluate, {"modules": modules, "fixture": fixture,
                "generated": generated, "reference": reference, "mode": mode})
            return result, browser.version
        finally:
            browser.close()

    if args.baseline_package and (args.generated_package or args.lean_package):
        generated, generated_evidence["baseline"] = package(args.baseline_package)
        reference, version = phase("baseline", generated, None, args.baseline_package)
        del generated
        reference_bytes = json.dumps(reference, ensure_ascii=False, separators=(",", ":")).encode()
        reference_sha = hashlib.sha256(reference_bytes).hexdigest()
        # Reproducible evidence only; every gate still captures its oracle from
        # the supplied actual old runtime, never from a precomputed golden.
        reference_path = args.fixture.with_name("flow-cjk-latin-reference.json.gz")
        reference_path.write_bytes(gzip.compress(reference_bytes, mtime=0))
        del reference_bytes

    for mode, directory in [("enabled", args.generated_package), ("lean", args.lean_package)]:
        if mode == "lean" and directory is None:
            continue
        generated, generated_evidence[mode] = package(directory)
        result, version = phase(mode, generated, reference, directory)
        phases.append({"mode": mode, **result})
        del generated
report = {"passed": sum(phase["passed"] for phase in phases), "failed": sum(phase["failed"] for phase in phases),
    "browser": version, "fixtureSha256": hashlib.sha256(fixture_bytes).hexdigest(),
    "moduleSha256": {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in names},
    "generatedWasm": generated_evidence, "latinReferenceSha256": reference_sha, "networkRequests": requests, "pageErrors": errors, "phases": phases}
print(json.dumps(report, indent=2), flush=True)
if report["failed"] or requests or errors:
    raise SystemExit(1)
