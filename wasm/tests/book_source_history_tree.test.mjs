import assert from "node:assert/strict";
import test from "node:test";
import vm from "node:vm";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// The complete collection, source host and history controller execute unchanged
// as ESM in one realm. DOM, transport, native-bound option admission and font
// storage below are explicit test doubles, not a second renderer or a WASM test.
const adapters = String.raw`
export const counters = { normalized: 0, fontsRead: 0, rendered: 0, cancelled: 0 };
export const bookError = (code,message) => Object.assign(new Error(message),{code});
export function bookTextBytes(text,limit=Infinity) {
  if(typeof text!=='string'||/[\ud800-\udfff]/u.test(text))throw bookError('INVALID_UNICODE','Invalid fixture text');
  const size=new TextEncoder().encode(text).length;
  if(size>limit)throw bookError('BOOK_LIMIT','Fixture text budget');
  return size;
}
export function prepareBookInput(files,options={}) { counters.normalized++;return structuredClone({files,options}); }
export const BOOK_FONT_LIMITS = {faceBytes:8*1024*1024,totalBytes:32*1024*1024};
export const BOOK_FONT_SLOTS = ['body','bold','italic','mono'];
export function createBookFontStore() {
  const data=new Map();
  return {set(assets,fence=()=>{}){const next=structuredClone(assets);fence();for(const asset of next)data.set(asset.slot,asset)},
    list(){return [...data.values()].map(({bytes,...metadata})=>({...metadata,size:bytes.length}))},
    snapshot(){counters.fontsRead++;return structuredClone([...data.values()])},
    remove(slot){return data.delete(slot)},clear(){const changed=data.size>0;data.clear();return changed},dispose(){data.clear()}};
}
export const createBookCollectionCapture = collection => () => {
  const input=collection.snapshot();return {...input,render(){counters.rendered++;throw new Error('No renderer in this fixture')}};
};
export const normalizePdfPage = () => ({size:{widthPt:612,heightPt:792},margins:{topPt:72,rightPt:72,bottomPt:72,leftPt:72}});
export const bookEditorOffset = (text,offset) => text.slice(0,offset).replace(/\r\n?/g,'\n').length;
`;
const fixture = String.raw`
import assert from 'node:assert/strict';
import {createBookCollection,readPortableBookProject} from './wasm/demo/book_collection.mjs';
import {createBookControls} from './wasm/demo/book_controls.mjs';
import {createBookSourceHistoryControls} from './wasm/demo/book_source_history.mjs';
import {counters} from './adapters.mjs';
const equal=(a,b)=>assert.equal(JSON.stringify(a),JSON.stringify(b));
const file=(path,source=path,role)=>({path,source,...(role==='include'?{role}:{})});
const textView=text=>text.replace(/\r\n?/g,'\n');
class Element {
  constructor(tag='div'){this.tagName=tag;this.children=[];this.listeners=new Map();this.attributes={};this.parentElement=null;this.id='';this.textContent='';this._value='';this.checked=false;this.disabled=false;this.hidden=false;this.selectionStart=0;this.selectionEnd=0;this.scrollTop=0;}
  get value(){return this._value} set value(value){this._value=this.tagName==='textarea'?textView(String(value)):String(value)}
  append(...nodes){for(const node of nodes){node.parentElement=this;this.children.push(node)}}
  appendChild(node){this.append(node);return node}
  insertBefore(node,sibling){node.parentElement=this;const index=this.children.indexOf(sibling);if(index<0)this.children.push(node);else this.children.splice(index,0,node)}
  replaceChildren(...nodes){for(const old of this.children)old.parentElement=null;this.children=[];this.append(...nodes)}
  setAttribute(key,value){this.attributes[key]=value} removeAttribute(key){delete this.attributes[key];delete this[key]}
  querySelector(selector){if(selector==='#'+this.id)return this;for(const child of this.children){const found=child.querySelector(selector);if(found)return found}return null}
  addEventListener(type,fn,capture=false){if(!this.listeners.has(type))this.listeners.set(type,[]);this.listeners.get(type).push({fn,capture:capture===true})}
  removeEventListener(type,fn){this.listeners.set(type,(this.listeners.get(type)??[]).filter(item=>item.fn!==fn))}
  emit(type,extra={}){const event={target:this,cancelable:true,defaultPrevented:false,preventDefault(){if(this.cancelable)this.defaultPrevented=true},...extra};const all=[...(this.listeners.get(type)??[])];for(const capture of [true,false])for(const item of all)if(item.capture===capture)item.fn(event);return event}
  focus(){this.focused=true}setSelectionRange(start,end){this.selectionStart=start;this.selectionEnd=end}
}
function localFile(name,text) {const bytes=new TextEncoder().encode(text);return {[Symbol.toStringTag]:'File',name,size:bytes.length,arrayBuffer:async()=>bytes.slice().buffer};}
const turn=()=>new Promise(resolve=>setTimeout(resolve,0));
function make(initial=[file('a.md','\ufeff# A\r\n😀\rEnd\n'),file('part.txt','shared','include'),file('b.md','# B')], {history=true,wrap}={}) {
  const root=new Element('document');root.createElement=tag=>new Element(tag);
  const parent=root.appendChild(new Element());
  const ids=['source-role','add-include','import-includes','import-include-folder','chapters','chapter-path','chapter-source','add-chapter','move-up','move-down','remove-chapter','import-files','import-folder','open-project','save-project','save-chapter','revoke-images','image-list','title','author','lang','font','dark-mode','font-scale','toc','page-numbers','export-pdf','export-epub','export-site','cancel-export','download','status'];
  for(const id of ids)parent.appendChild(new Element(id==='chapter-source'?'textarea':'div')).id=id;
  const el=id=>root.querySelector('#'+id);
  const collection=createBookCollection();
  collection.append({chapters:initial.filter(f=>f.role!=='include'),includeSources:initial.filter(f=>f.role==='include'),images:[{destination:'x.png',bytes:new Uint8Array([1,2,3])}]});
  collection.replaceSourceTree(initial,collection.revision);
  collection.setFonts([{slot:'body',name:'Fixture',bytes:new Uint8Array([4,5])}],collection.revision);
  let resets=0;
  const worker={hasRetainedBook:true,cancel(){counters.cancelled++},cancelPending(){counters.cancelled++},dispose(){}};
  const controls=createBookControls({root,collection,worker,confirm:()=>true,urls:{createObjectURL:()=> 'blob:fixture',revokeObjectURL(){}},onProjectReplaced(){resets++}});
  const host=wrap?wrap(controls,collection):controls;
  const ui=history?createBookSourceHistoryControls({root,controls:host,collection}):null;
  return {root,el,collection,controls,ui,get resets(){return resets},
    select(index){el('chapters').value=String(index);el('chapters').emit('change')},
    input(id,value){el(id).value=value;el(id).emit('input',{inputType:'insertFromPaste'})},
    role(value){el('source-role').value=value;el('source-role').emit('change')},
    click(id){el(id).emit('click')},close(){ui?.dispose();controls.dispose()}};
}
`;
const cases = {
  "complete tree replacement is one observable revision and preserves current resources": () => {
    const h=make(undefined,{history:false}), c=h.collection, before=c.snapshot(), settings=c.options, stamps=[c.revision,c.renderConfigurationRevision,c.sourceProjectRevision];
    const notices=[];c.subscribe(()=>notices.push(c.files));
    const next=[file('new.md','new'),...c.files.toReversed()];
    const result=c.replaceSourceTree(next,c.revision);
    equal(notices,[next]);assert.equal(result,stamps[0]+1);assert.equal(c.renderConfigurationRevision,stamps[1]+1);assert.equal(c.sourceProjectRevision,stamps[2]);
    const after=c.snapshot();equal(after.options.images,before.options.images);equal(after.options.fontAssets,before.options.fontAssets);equal(c.options,settings);h.close();
  },
  "source-only and no-op trees keep the retained rendering configuration": () => {
    const h=make(undefined,{history:false}), c=h.collection, rev=c.revision, config=c.renderConfigurationRevision;
    assert.equal(c.replaceSourceTree(c.files,rev),rev);assert.equal(c.revision,rev);
    const next=c.files;next[0].source+='!';c.replaceSourceTree(next,c.revision);
    assert.equal(c.renderConfigurationRevision,config);assert.equal(c.revision,rev+1);h.close();
  },
  "typing then add, reorder, rename, role change and deletion reverse chronologically": async () => {
    const h=make(), states=[h.collection.files];
    h.input('chapter-source','edited first');states.push(h.collection.files);
    h.click('add-chapter');states.push(h.collection.files);
    h.click('move-up');states.push(h.collection.files);
    h.input('chapter-path','guide/new.md');states.push(h.collection.files);
    h.role('include');states.push(h.collection.files);
    h.click('remove-chapter');await turn();states.push(h.collection.files);
    assert.equal(h.ui.state.undo,6);
    for(let i=states.length-2;i>=0;i--){h.ui.undo();equal(h.collection.files,states[i]);assert.equal(h.el('chapters').children.length,states[i].length);equal(h.el('chapter-source').value,textView(states[i][h.controls.currentChapter].source));}
    for(let i=1;i<states.length;i++){h.ui.redo();equal(h.collection.files,states[i]);}
    assert.equal(h.resets,0);assert.equal(h.collection.images.length,1);assert.equal(h.collection.fonts.length,1);h.close();
  },
  "first chapter and deleting the final chapter recover through an empty editor": async () => {
    const h=make([]);h.click('add-chapter');assert.equal(h.ui.state.undo,1);h.ui.undo();
    assert.equal(h.collection.files.length,0);assert.equal(h.el('chapter-source').value,'');assert(h.el('chapter-source').disabled);assert.equal(h.el('chapters').children.length,0);
    h.ui.redo();assert.equal(h.collection.files.length,1);assert(!h.el('chapter-source').disabled);
    h.click('remove-chapter');await turn();assert.equal(h.collection.files.length,0);
    h.ui.undo();assert.equal(h.collection.files.length,1);h.ui.redo();assert.equal(h.collection.files.length,0);h.close();
  },
  "partial rename and incomplete Unicode remain recoverable without weakening export admission": () => {
    const h=make(), original=h.collection.files;
    h.input('chapter-path','');assert.equal(h.collection.files[0].path,'');
    assert.throws(()=>h.collection.project(),{code:'INVALID_PATH'});
    h.input('chapter-source','\ud800');h.ui.undo();assert.equal(h.collection.files[0].path,'');
    h.ui.undo();equal(h.collection.files,original);
    h.ui.redo();h.ui.redo();assert.equal(h.collection.files[0].source,'\ud800');
    assert.throws(()=>h.collection.snapshot(),{code:'INVALID_PATH'});h.ui.undo();h.ui.undo();h.close();
  },
  "restored CRLF and BOM keep the original-source relationship after textarea recapture": () => {
    const h=make(), original=h.collection.files;
    h.controls.applySourceTree([],h.controls.checkpoint());h.ui.undo();
    equal(h.collection.files,original);h.controls.captureProject();equal(h.collection.files,original);h.close();
  },
  "reorder retains selected source and clamps selection when it is removed": () => {
    const h=make(undefined,{history:false});h.select(2);h.el('chapter-source').setSelectionRange(1,2);h.el('chapter-source').scrollTop=14;
    const reverse=h.collection.files.toReversed();h.controls.applySourceTree(reverse,h.controls.checkpoint());
    assert.equal(h.controls.currentChapter,0);assert.equal(h.el('chapter-path').value,'b.md');assert.equal(h.el('chapter-source').selectionStart,1);assert.equal(h.el('chapter-source').scrollTop,14);
    h.controls.applySourceTree(reverse.slice(1),h.controls.checkpoint());assert.equal(h.el('chapter-path').value,'part.txt');assert.equal(h.el('chapter-source').selectionStart,0);h.close();
  },
  "revoked resources and new publishing settings never roll back with source history": () => {
    const h=make();h.click('add-chapter');h.input('title','New title');h.collection.revokeImages();h.collection.revokeFonts();
    const before={...counters};h.ui.undo();assert.equal(h.collection.options.title,'New title');assert.equal(h.collection.images.length,0);assert.equal(h.collection.fonts.length,0);
    h.ui.redo();assert.equal(h.collection.images.length,0);assert.equal(h.collection.fonts.length,0);
    assert.equal(counters.fontsRead,before.fontsRead);assert.equal(counters.normalized,before.normalized);assert.equal(counters.rendered,0);h.close();
  },
  "stale collection revisions and raw editor checkpoints reject before mutation": () => {
    const h=make(), c=h.collection, before=c.files, rev=c.revision, stamp=h.controls.checkpoint();
    h.el('chapter-source').value='uncaptured';assert.throws(()=>h.controls.applySourceTree([],stamp),{code:'STALE_SOURCE'});
    equal(c.files,before);assert.throws(()=>c.replaceSourceTree([],rev-1),{code:'STALE_SOURCE'});
    h.el('chapter-source').value=textView(before[0].source);h.click('add-chapter');h.el('chapter-path').value='silent.md';
    assert.throws(()=>h.ui.undo(),{code:'STALE_SOURCE'});assert.equal(h.ui.state.undo,1);h.close();
  },
  "malformed records cannot smuggle settings or resource authority into source replay": () => {
    const h=make(undefined,{history:false}), c=h.collection, rev=c.revision, before=c.files;
    for(const bad of [null,{},new Array(1),[{}],[file('x.md',3)],[{...file('x.md'),images:[]}],[{...file('x.md'),role:'asset'}],[{...file('x.md'),get extra(){throw new Error('must not run')}}]]){
      assert.throws(()=>c.replaceSourceTree(bad,c.revision));assert.equal(c.revision,rev);equal(c.files,before);
    }h.close();
  },
  "tree capture refuses accessors and ignores user-provided array iterators": () => {
    const h=make(undefined,{history:false}), c=h.collection;let calls=0;
    assert.throws(()=>c.replaceSourceTree([{path:'x.md',get source(){calls++;return 'x'}}],c.revision),{code:'INVALID_SOURCE_TREE'});
    const next=[file('x.md','x')];next[Symbol.iterator]=()=>{calls++;throw new Error('iterator')};c.replaceSourceTree(next,c.revision);assert.equal(calls,0);equal(c.files,[file('x.md','x')]);h.close();
  },
  "tree limits reject over-budget final trees atomically": () => {
    const h=make(undefined,{history:false}),c=h.collection,before=c.files,rev=c.revision;
    const bad=[Array.from({length:129},(_,i)=>file(i+'.md','')),Array.from({length:129},(_,i)=>file(i+'.txt','','include')),
      [file('x'.repeat(1025), '')],[file('x.md','x'.repeat(4*1024*1024+1))],Array.from({length:5},(_,i)=>file(i+'.md','x'.repeat(4*1024*1024)))];
    for(const next of bad){assert.throws(()=>c.replaceSourceTree(next,rev),{code:'BOOK_LIMIT'});assert.equal(c.revision,rev);equal(c.files,before);}h.close();
  },
  "ordinary validated source transactions still cannot change structure": () => {
    const h=make(undefined,{history:false}), c=h.collection;
    assert.throws(()=>c.replaceSources(c.files.toReversed(),c.revision),{code:'INVALID_TRANSACTION'});
    assert.throws(()=>c.replaceSources([],c.revision),{code:'INVALID_TRANSACTION'});h.close();
  },
  "byte-identical project replacement resets source history before it can cross books": () => {
    const h=make();h.click('add-chapter');const project=h.collection.project(),epoch=h.collection.sourceProjectRevision;
    h.controls.replaceProject(project,h.controls.checkpoint());
    assert.equal(h.collection.sourceProjectRevision,epoch+1);assert.equal(h.ui.state.undo,0);assert.equal(h.ui.state.redo,0);
    assert.throws(()=>h.ui.undo(),{code:'NO_HISTORY'});h.close();
  },
  "failed project admission preserves both project identity and existing history": () => {
    const h=make();h.click('add-chapter');const epoch=h.collection.sourceProjectRevision;
    assert.throws(()=>h.controls.replaceProject({schemaVersion:99},h.controls.checkpoint()));
    assert.equal(h.collection.sourceProjectRevision,epoch);assert.equal(h.ui.state.undo,1);h.ui.undo();h.close();
  },
  "source-project import and portable restore establish new history identities": async () => {
    const h=make();h.click('add-chapter');const project=h.collection.project(),epoch=h.collection.sourceProjectRevision;
    await h.controls.importFiles([localFile('book.json',JSON.stringify(project))],'project');
    assert.equal(h.collection.sourceProjectRevision,epoch+1);assert.equal(h.ui.state.undo,0);assert.equal(h.resets,1);
    h.click('add-chapter');const portable={format:'franken-markdown-portable-book',schemaVersion:1,project:h.collection.project(),images:[],fontAssets:[]};
    const review=await readPortableBookProject(localFile('bundle.json',JSON.stringify(portable)));
    h.collection.replacePortableProject(review,h.collection.revision,{authorizeResources:true});
    assert.equal(h.collection.sourceProjectRevision,epoch+2);assert.equal(h.ui.state.undo,0);h.close();
  },
  "append imports become one source-history transaction, not a project switch": async () => {
    const h=make(), before=h.collection.files,epoch=h.collection.sourceProjectRevision;
    await h.controls.importFiles([localFile('new.md','# Imported')]);
    assert.equal(h.collection.sourceProjectRevision,epoch);assert.equal(h.ui.state.undo,1);h.ui.undo();equal(h.collection.files,before);h.ui.redo();assert.equal(h.collection.files.at(-1).path,'new.md');h.close();
  },
  "project identity changes during a replay cannot acknowledge an old undo": () => {
    let replace=false;
    const h=make(undefined,{wrap(controls,c){const descriptors=Object.getOwnPropertyDescriptors(controls);descriptors.applySourceTree={value(...args){const rev=controls.applySourceTree(...args);if(replace){replace=false;c.replaceProject(c.project())}return rev}};return Object.defineProperties({},descriptors)}});
    h.click('add-chapter');replace=true;assert.throws(()=>h.ui.undo(),{code:'HISTORY_CONFLICT'});assert.equal(h.ui.state.undo,0);assert.equal(h.ui.state.redo,0);h.close();
  },
  "reentrant tree admission is fenced after a host edit or disposal": () => {
    for(const dispose of [false,true]){
      const h=make(undefined,{history:false}),c=h.collection,rev=c.revision;let once=false;
      const value=new Proxy(file('x.md','x'),{ownKeys(target){if(!once){once=true;if(dispose)c.dispose();else c.edit(0,'newer.md','newer')}return Reflect.ownKeys(target)}});
      assert.throws(()=>c.replaceSourceTree([value],rev),{code:dispose?'SESSION_DISPOSED':'STALE_SOURCE'});
      if(!dispose){assert.equal(c.files[0].source,'newer');h.close();}
    }
  },
  "text composition and pending imports block structural replay": async () => {
    const h=make();h.click('add-chapter');h.el('chapter-source').emit('compositionstart');
    assert.throws(()=>h.ui.undo(),{code:'BOOK_BUSY'});h.el('chapter-source').emit('compositionend');
    h.el('chapter-path').emit('compositionstart');assert.throws(()=>h.ui.undo(),{code:'BOOK_BUSY'});h.el('chapter-path').emit('compositionend');
    let release;const input=localFile('new.md','new');const bytes=await input.arrayBuffer();input.arrayBuffer=()=>new Promise(resolve=>{release=()=>resolve(bytes)});
    const pending=h.controls.importFiles([input]);assert(h.controls.sourceBusy);assert.throws(()=>h.ui.undo(),{code:'BOOK_BUSY'});
    release();await pending;h.ui.undo();h.ui.undo();h.close();
  },
  "source recovery clears on suspension and disposal without starting renders": () => {
    const h=make();h.click('add-chapter');const before=h.collection.files;
    h.ui.suspend();assert.equal(h.ui.state.undo,0);equal(h.collection.files,before);assert.throws(()=>h.ui.undo(),{code:'HISTORY_CLOSED'});
    h.ui.resume();assert.equal(h.ui.state.undo,0);h.click('add-chapter');h.ui.dispose();const rev=h.collection.revision;
    h.el('book-history-undo').emit('click');assert.equal(h.collection.revision,rev);assert.equal(counters.rendered,0);h.controls.dispose();
  },
};

async function run(name) {
  const context=vm.createContext({console,TextEncoder,TextDecoder,Uint8Array,ArrayBuffer,Blob,AbortController,URL,
    structuredClone,setTimeout,clearTimeout,performance,atob,btoa});
  const modules=new Map();
  const paths={
    'wasm/demo/book_collection.mjs':new URL('../demo/book_collection.mjs',import.meta.url),
    'wasm/demo/book_controls.mjs':new URL('../demo/book_controls.mjs',import.meta.url),
    'wasm/demo/book_source_history.mjs':new URL('../demo/book_source_history.mjs',import.meta.url),
  };
  const adapterPaths=new Set(['wasm/book_session.mjs','wasm/book_worker.mjs','wasm/pdf_page.mjs','wasm/demo/book_font_assets.mjs','wasm/demo/book_source_search.mjs','adapters.mjs']);
  const get=name=>{
    if(adapterPaths.has(name))name='adapters.mjs';
    if(modules.has(name))return modules.get(name);
    let module;
    if(name==='node:assert/strict')module=new vm.SyntheticModule(['default'],function(){this.setExport('default',assert)},{context,identifier:name});
    else {
      assert(name==='adapters.mjs'||paths[name],`Unbound fixture import ${name}`);
      module=new vm.SourceTextModule(name==='adapters.mjs'?adapters:readFileSync(paths[name],'utf8'),{context,identifier:name});
    }
    modules.set(name,module);return module;
  };
  const suite=new vm.SourceTextModule(fixture+`\nawait (${cases[name].toString()})();`,{context,identifier:'suite.mjs'});
  await suite.link((specifier,parent)=>{
    const resolved=specifier.startsWith('node:')?specifier:new URL(specifier,'file:///'+parent.identifier).pathname.slice(1);
    return get(resolved);
  });
  await suite.evaluate();
}
if(process.env.FMD_TREE_CASE) {
  assert(Object.hasOwn(cases,process.env.FMD_TREE_CASE));
  await run(process.env.FMD_TREE_CASE);
} else for(const name of Object.keys(cases)) test(name,async()=>{
  if(typeof vm.SourceTextModule==='function')return run(name);
  const result=spawnSync(process.execPath,['--experimental-vm-modules',fileURLToPath(import.meta.url)],{
    env:{...process.env,FMD_TREE_CASE:name},encoding:'utf8',timeout:15000,
  });
  assert.equal(result.status,0,result.stderr||result.stdout);
});
