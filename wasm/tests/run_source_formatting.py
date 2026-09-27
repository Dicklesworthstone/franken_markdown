#!/usr/bin/env python3
"""Native-DOM checks for source commands and their existing undo integration.

Loads the production entrypoint/planner/controller through local Blob modules.
Extracts the existing editing/history declarations and source validator directly
from their modules; file I/O, drafts and direct-file controls are explicit doubles.
No renderer, generated WASM, persistence or native Markdown parse is exercised.
All requests are fulfilled from this allowlist or blocked; nothing is fetched.
"""
import argparse
import json
from pathlib import Path
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]

def between(text, start, end):
    assert text.count(start) == 1 and text.count(end) == 1
    return text[text.index(start):text.index(end)]

def resources():
    session = (ROOT / "wasm/flow_session.mjs").read_text()
    validation = "export const FLOW_SOURCE_LIMIT = 4 * 1024 * 1024;\n" + between(
        session, "export class FlowError", "export function normalizeFlowError") + between(
        session, "function sourceByteLength", "export function layoutOptions")
    original = (ROOT / "wasm/demo/flow_document.mjs").read_text()
    assert original.count("const HISTORY_PAYLOAD_LIMIT") == 1
    editing = 'import { FLOW_SOURCE_LIMIT, FlowError, sourceText } from "../validation.mjs";\n'
    editing += 'const fail = (code, message, cause) => { throw new FlowError(code, message, { cause }); };\n'
    editing += original[original.index("const HISTORY_PAYLOAD_LIMIT"):]
    editing += '''\nexport function createSourceControls({sourceEditor, filename}) {
      return { dispose() {}, snapshot() {return {source:sourceEditor.value, filename:filename.value};} };
    }\n'''
    names = ["source-filename", "source-status", "open-markdown", "prepare-markdown", "source-download", "remember-draft", "refresh-draft", "restore-draft", "forget-draft", "save-draft", "draft-status", "source-undo", "source-redo", "source-find", "source-find-insensitive", "source-find-previous", "source-find-next", "source-replacement", "source-replace", "source-replace-all", "source-edit-status"]
    html = '<!doctype html><html lang="en"><meta charset="utf-8"><title>Source formatting test</title><section><textarea id="source">café😀</textarea></section><input id="other">'
    for name in names:
        tag = "input" if name in ["source-filename", "source-find", "source-replacement", "source-find-insensitive"] else "button"
        html += f'<{tag} id="{name}">' + (f'</{tag}>' if tag != "input" else "")
    html += '</html>'
    data = {"/": ("text/html", html), "/wasm/validation.mjs": ("text/javascript", validation),
        "/wasm/demo/flow_document.mjs": ("text/javascript", editing),
        "/wasm/demo/flow_file_controls.mjs": ("text/javascript", "export const createFileControls=()=>({dispose(){}});"),
        "/wasm/demo/flow_draft_controls.mjs": ("text/javascript", "export const createDraftControls=()=>({dispose(){}});")}
    for file in ["flow-source.js", "source_formatting.mjs", "source_commands.mjs"]:
        data["/wasm/demo/" + file] = ("text/javascript", (ROOT / "wasm/demo" / file).read_text())
    return data

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    args = parser.parse_args()
    data = resources()
    passed = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
        page = browser.new_page()
        errors = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.route("**/*", lambda request: request.abort())
        def setup(text="café😀"):
            nonlocal page
            page.close()
            page = browser.new_page()
            page.route("**/*", lambda request: request.abort())
            page.on("pageerror", lambda e: errors.append(str(e)))
            page.set_content(data["/"][1])
            modules = {path: value[1] for path, value in data.items() if path != "/"}
            page.evaluate(r"""async modules => {
              const urls = {};
              for (const path of ['/wasm/validation.mjs', '/wasm/demo/source_commands.mjs',
                '/wasm/demo/flow_document.mjs', '/wasm/demo/flow_file_controls.mjs',
                '/wasm/demo/flow_draft_controls.mjs', '/wasm/demo/source_formatting.mjs',
                '/wasm/demo/flow-source.js']) {
                const linked = modules[path].replace(/(from\s*["'])([^"']+)(["'])/g, (match,a,spec,b) => {
                  const target = new URL(spec, 'https://fixture.invalid'+path).pathname;
                  if (!urls[target]) throw new Error('unresolved test import '+target);
                  return a+urls[target]+b;
                });
                urls[path]=URL.createObjectURL(new Blob([linked], {type:'text/javascript'}));
              }
              await import(urls['/wasm/demo/flow-source.js']);
            }""", modules)
            page.wait_for_selector('[data-source-command="bold"]')
            page.evaluate('''text => {
              window.s = document.querySelector('#source'); s.value=text;
              s.dispatchEvent(new Event('fmd-document-replaced'));
              s.setSelectionRange(0,text.length,'backward');
              window.inputs=0; s.addEventListener('input',()=>inputs++);
            }''', text)
        def check(name, script):
            result = page.evaluate("() => {" + script + "}")
            assert result is True, (name, result)
            passed.append(name)
        setup()
        check("twelve accessible native buttons", "return document.querySelectorAll('[data-source-command]').length===12 && document.querySelector('[data-source-formatting]').getAttribute('role')==='group';")
        page.locator('[data-source-command="bold"]').click()
        check("button edits exact source once", "return s.value==='**café😀**' && inputs===1 && s.value.slice(s.selectionStart,s.selectionEnd)==='café😀';")
        page.locator('#source-undo').click()
        check("existing undo restores original text and backward selection", "return s.value==='café😀' && s.selectionStart===0 && s.selectionEnd===6 && s.selectionDirection==='backward' && inputs===2;")
        page.locator('#source-redo').click()
        check("existing redo restores formatting and selection", "return s.value==='**café😀**' && s.selectionStart===2 && s.selectionEnd===8 && inputs===3;")
        setup("one\ntwo")
        page.locator('[data-source-command="task-list"]').press("Enter")
        check("Enter applies a multi-line task transaction", "return s.value==='- [ ] one\\n- [ ] two' && inputs===1;")
        page.locator('#source-undo').click()
        check("one undo restores the entire list transaction", "return s.value==='one\\ntwo' && document.querySelector('#source-undo').disabled;")
        setup("a`b")
        page.locator('[data-source-command="inline-code"]').click()
        check("adaptive inline code delimiter", "return s.value==='``a`b``' && s.value.slice(s.selectionStart,s.selectionEnd)==='a`b';")
        setup("one\n````\ntwo")
        page.locator('[data-source-command="code-block"]').click()
        check("embedded fence cannot close new code block", "return s.value==='`````\\none\\n````\\ntwo\\n`````' && inputs===1;")
        setup()
        page.evaluate("s.addEventListener('beforeinput', e=>e.preventDefault(), {once:true})")
        page.locator('[data-source-command="bold"]').click()
        check("beforeinput cancellation publishes nothing", "return s.value==='café😀' && inputs===0 && document.querySelector('#source-undo').disabled;")
        page.locator('[data-source-command="bold"]').click()
        page.locator('#source-undo').click()
        check("veto does not create a phantom undo entry", "return s.value==='café😀' && document.querySelector('#source-undo').disabled;")
        setup()
        page.evaluate("s.addEventListener('focus', ()=>{s.value='replacement';s.dispatchEvent(new Event('fmd-document-replaced'));}, {once:true})")
        page.locator('[data-source-command="bold"]').click()
        check("focus replacement is fenced before mutation", "return s.value==='replacement' && inputs===0 && document.querySelector('#source-edit-status').textContent.includes('STALE_SOURCE');")
        setup()
        page.evaluate("s.addEventListener('beforeinput', ()=>s.setSelectionRange(0,0), {once:true})")
        page.locator('[data-source-command="bold"]').click()
        check("beforeinput selection changes are fenced", "return s.value==='café😀' && inputs===0;")
        setup()
        page.evaluate("s.addEventListener('beforeinput', ()=>s.dispatchEvent(new Event('fmd-document-replaced')), {once:true})")
        page.locator('[data-source-command="bold"]').click()
        check("same-text document replacement invalidates the command", "return s.value==='café😀' && inputs===0;")
        setup()
        page.evaluate("s.dispatchEvent(new CompositionEvent('compositionstart'))")
        check("IME disables commands without hijacking input", "const e=new KeyboardEvent('keydown',{key:'b',ctrlKey:true,cancelable:true,isComposing:true});s.dispatchEvent(e);return !e.defaultPrevented && inputs===0 && [...document.querySelectorAll('[data-source-command]')].every(b=>b.disabled);")
        page.evaluate("s.dispatchEvent(new CompositionEvent('compositionend'))")
        page.locator('#source').press("Control+b")
        check("formatting resumes after composition", "return s.value==='**café😀**' && inputs===1;")
        setup()
        page.evaluate("s.readOnly=true")
        check("readonly blocks commands", "const e=new KeyboardEvent('keydown',{key:'b',ctrlKey:true,cancelable:true});s.dispatchEvent(e);return !e.defaultPrevented && inputs===0;")
        page.wait_for_function("document.querySelector('[data-source-command=bold]').disabled")
        setup()
        check("unrelated shortcuts and autorepeat remain untouched", "for(const p of [{repeat:true},{altKey:true},{shiftKey:true},{isComposing:true}]){const e=new KeyboardEvent('keydown',{key:'b',ctrlKey:true,cancelable:true,...p});s.dispatchEvent(e);if(e.defaultPrevented)return false;}const e=new KeyboardEvent('keydown',{key:'b',ctrlKey:true,cancelable:true,bubbles:true});document.querySelector('#other').dispatchEvent(e);return !e.defaultPrevented&&inputs===0;")
        setup("one\ntwo")
        page.locator('[data-source-command="bold"]').click()
        check("multiline inline refusal preserves source", "return s.value==='one\\ntwo'&&inputs===0&&document.querySelector('#source-edit-status').textContent.includes('MULTILINE_SELECTION');")
        setup()
        page.locator('[data-source-command="bold"]').click()
        page.evaluate("window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true}))")
        check("pagehide removes UI and shortcuts", "const e=new KeyboardEvent('keydown',{key:'b',ctrlKey:true,cancelable:true});s.dispatchEvent(e);return !e.defaultPrevented && !document.querySelector('[data-source-formatting]');")
        page.evaluate("window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}))")
        check("bfcache resumes one toolbar and fresh existing history", "return document.querySelectorAll('[data-source-formatting]').length===1&&document.querySelector('#source-undo').disabled;")
        page.locator('[data-source-command="italic"]').click()
        check("resume adds exactly one new transaction", "return s.value==='***café😀***'&&inputs===2;")
        setup()
        page.evaluate("s.addEventListener('beforeinput',()=>window.dispatchEvent(new PageTransitionEvent('pagehide')), {once:true})")
        page.locator('[data-source-command="bold"]').click()
        check("suspension during beforeinput cannot edit source", "return s.value==='café😀' && inputs===0;")
        setup("<script>alert(1)</script>")
        page.locator('[data-source-command="bold"]').click()
        check("HTML-looking source stays inert text", "return s.value==='**<script>alert(1)</script>**' && inputs===1 && !document.querySelector('script:not([type=module])');")
        assert not errors, errors
        browser.close()
    print(json.dumps({"passed": len(passed), "checks": passed, "scope": "native DOM + source controls; no WASM, files or persistence"}, indent=2))

if __name__ == "__main__": main()
