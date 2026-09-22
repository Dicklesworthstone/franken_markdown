// tsc --noEmit --strict --target ES2022 --module NodeNext wasm/interactive_types.test.ts
import {renderOfflineWorkspace, type FmdOfflineRuntime, type FmdOfflineWorkspaceOptions} from './interactive.js';
const bytes = new Uint8Array([0,97,115,109,1,0,0,0]);
const runtime: FmdOfflineRuntime = {bindings:'trusted generated module source',wasm:bytes};
const options: FmdOfflineWorkspaceOptions = {font:'serif',fontScale:1.125,lang:'fr',toc:true,
  metadataEpochSeconds:0,pdfImages:[{destination:'a.png',bytes:new DataView(bytes.buffer)}],
  fontAssets:[{slot:'body-regular',bytes,weight:550}]};
const output = await renderOfflineWorkspace('# Source',runtime,options);
const data: Uint8Array = output.bytes;
const blob: Blob = output.blob();
const filename: string = output.filename('document');
void [data,blob,filename];
// @ts-expect-error Explicit bytes, not an ambient runtime URL.
renderOfflineWorkspace('source',{bindings:'x',wasm:new URL('https://example.invalid/runtime.wasm')});
// @ts-expect-error Safe-only API does not admit raw HTML.
renderOfflineWorkspace('source',runtime,{allowRawHtml:true});
// @ts-expect-error Workspace captures numeric typography, not coercible presets.
renderOfflineWorkspace('source',runtime,{fontScale:'large'});
// @ts-expect-error Unsupported custom CSS must not disappear silently.
renderOfflineWorkspace('source',runtime,{customCss:'body {}'});
