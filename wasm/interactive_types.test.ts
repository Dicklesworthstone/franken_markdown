// tsc --noEmit --strict --target ES2022 --module NodeNext wasm/interactive_types.test.ts
import {renderOfflineWorkspace, type FmdOfflineRuntime, type FmdOfflineWorkspaceOptions} from './interactive.js';
import {renderInteractiveHtml} from './franken_markdown.js';
const bytes = new Uint8Array([0,97,115,109,1,0,0,0]);
const runtime: FmdOfflineRuntime = {bindings:'trusted generated module source',wasm:bytes};
const options: FmdOfflineWorkspaceOptions = {font:'serif',fontScale:1.125,lang:'fr',toc:true,
  metadataEpochSeconds:0,pdfImages:[{destination:'a.png',bytes:new DataView(bytes.buffer)}],
  fontAssets:[{slot:'body-regular',bytes,weight:550}],pdfA:'2b',pdfAStrict:true,footnoteBacklinks:true,baselineGridPt:13.125,
  page:{size:'a4',orientation:'landscape',margins:{topPt:24,rightPt:36,bottomPt:48,leftPt:60}},
  running:{header:{left:'{chapter}',right:'{section}',image:{dest:'a.png',position:'right',heightPt:20}},
    footer:{center:'{page} / {pages}'},skipFirstPage:true}};
const output = await renderOfflineWorkspace('# Source',runtime,options);
const data: Uint8Array = output.bytes;
const blob: Blob = output.blob();
const filename: string = output.filename('document');
void [data,blob,filename];
void renderOfflineWorkspace('source',runtime,{pdfA:'off',pdfAStrict:false,footnoteBacklinks:false});
void renderOfflineWorkspace('source',runtime,{splitFootnotes:true});
void renderOfflineWorkspace('source',runtime,{splitFootnotes:false});
// @ts-expect-error Explicit bytes, not an ambient runtime URL.
renderOfflineWorkspace('source',{bindings:'x',wasm:new URL('https://example.invalid/runtime.wasm')});
// @ts-expect-error Safe-only API does not admit raw HTML.
renderOfflineWorkspace('source',runtime,{allowRawHtml:true});
// @ts-expect-error Workspace captures numeric typography, not coercible presets.
renderOfflineWorkspace('source',runtime,{fontScale:'large'});
// @ts-expect-error Unsupported custom CSS must not disappear silently.
renderOfflineWorkspace('source',runtime,{customCss:'body {}'});
// @ts-expect-error Only the implemented archival profile can be requested.
renderOfflineWorkspace('source',runtime,{pdfA:'3b'});
// @ts-expect-error Strictness is a boolean rather than a form-control string.
renderOfflineWorkspace('source',runtime,{pdfA:'2b',pdfAStrict:'true'});
// @ts-expect-error Backlinks are a boolean rather than a form-control string.
renderOfflineWorkspace('source',runtime,{footnoteBacklinks:'true'});
// @ts-expect-error Continuation requires a boolean rather than a form-control string.
renderOfflineWorkspace('source',runtime,{splitFootnotes:'true'});
// @ts-expect-error The lightweight HTML-only API cannot retain PDF continuation.
renderInteractiveHtml('source',{splitFootnotes:true});
// @ts-expect-error The lightweight HTML-only API cannot retain PDF backlinks.
renderInteractiveHtml('source',{footnoteBacklinks:true});
// @ts-expect-error The lightweight HTML-only API cannot retain PDF archival policy.
renderInteractiveHtml('source',{pdfA:'off'});
// @ts-expect-error Even disabled PDF-only settings must not disappear silently.
renderInteractiveHtml('source',{pdfAStrict:false});

// @ts-expect-error Running slots preserve strings, not truthy numeric titles.
renderOfflineWorkspace('source',runtime,{running:{header:{left:7}}});
// @ts-expect-error Per-side margins use numbers in PDF points.
renderOfflineWorkspace('source',runtime,{page:{margins:{topPt:'24'}}});
