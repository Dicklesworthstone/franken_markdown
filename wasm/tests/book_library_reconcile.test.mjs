import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// Execute the COMPLETE production controller, library session and shared merger
// in one ESM realm. DOM, source host, error factory and revisioned storage below
// are explicit adapters. This does NOT emulate or qualify native IndexedDB,
// browser focus/IME, the real source-host/undo controller, or Rust/WASM rendering.
const fixture = String.raw`
import { createBookLibrarySession } from '../demo/book_library_session.mjs';
import { createBookLibraryControls } from '../demo/book_library_controls.mjs';
const assert = globalThis.fixtureAssert;
const copy = value => JSON.parse(JSON.stringify(value));
const error = (code,message=code) => Object.assign(new Error(message),{code});
const file = (path,source=path,role) => ({path,source,...(role==='include'?{role}:{})});
const initial = () => [file('a.md','a\nb\nc\nd\ne\n'),file('part.txt','include','include')];
const view = text => text.replace(/\r\n?/g,'\n');
const tick = () => new Promise(resolve=>setTimeout(resolve,0));
async function until(check) {for(let i=0;i<100;i++){if(check())return;await tick()}assert.fail('Fixture did not settle');}
const deferred = () => {let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b});return {promise,resolve,reject};};
const choices = (review,choice='local') => review.conflicts.map(({index})=>({index,choice}));
function storage() {
  let serial=0,closed=false;
  const records=new Map(),reads=[],writes=[];
  const store={records,reads,writes,readHook:null,saveHook:null,
    async list(){return copy([...records.values()].map(value=>value.entry));},
    async save(input){
      input=copy(input);writes.push(input);if(store.saveHook)await store.saveHook(input);
      if(closed)throw error('LIBRARY_CLOSED');
      const id=input.id??'book-'+(++serial),old=records.get(id);
      if(input.id&&old?.entry.revision!==input.expectedRevision)throw error('LIBRARY_CONFLICT');
      const revision=(old?.entry.revision??0)+1;
      const snapshots=new Map(old?.snapshots??[]);
      const saved={project:copy(input.project),name:input.name.trim()};snapshots.set(revision,saved);
      const versions=[{revision,savedAt:revision,bytes:100,name:saved.name},...(old?.entry.versions??[])].slice(0,5);
      for(const rev of snapshots.keys())if(!versions.some(v=>v.revision===rev))snapshots.delete(rev);
      const entry={id,revision,name:saved.name,updatedAt:revision,versions};records.set(id,{entry,snapshots});return copy(entry);
    },
    async read(id,revision=null){
      reads.push({id,revision});const saved=records.get(id),selected=revision??saved?.entry.revision;
      const snapshot=saved?.snapshots.get(selected);if(!snapshot)throw error('LIBRARY_NOT_FOUND');
      const value={id,revision:saved.entry.revision,snapshotRevision:selected,...copy(snapshot)};
      if(store.readHook)await store.readHook({id,revision,call:reads.length},value);
      return value;
    },
    async remove(id,revision){if(records.get(id)?.entry.revision!==revision)throw error('LIBRARY_CONFLICT');records.delete(id);},
    close(){closed=true;},
  };return store;
}
function harness(files=initial(),{legacy=false,onState=()=>{}}={}) {
  let tree=copy(files),revision=1,identity=1,raw='',blocked=false,applyMode=null,session;
  const store=storage(),jobs=new Map();let timer=0,applied=0,captured=0;
  const settings={title:'Local',page:{width:612}},assets=new Set(['image.png','body-font']);
  const stamp=()=>JSON.stringify([revision,identity,raw]);
  const host={checkpoint:stamp,captureProject(){captured++;if(raw==='invalid')throw error('INVALID_OPTIONS');
    return {schemaVersion:4,files:copy(tree),options:copy(settings)};},
    replaceProject(project,expected){assert.equal(expected,stamp());tree=copy(project.files);identity++;revision++;assets.clear();session.changed();}};
  const sources={read:()=>({files:copy(tree),stamp:stamp(),identity}),isBlocked:()=>blocked,
    apply(next,expected){assert.equal(expected,stamp());applied++;
      if(applyMode==='reject')throw error('HOST_REFUSED');
      if(applyMode==='promise')return Promise.resolve('bad');
      if(applyMode==='silent')return stamp();
      tree=copy(next);revision++;session.changed();
      if(applyMode==='identity')identity++;
      if(applyMode==='partial')tree.pop();
      if(applyMode==='throw')throw error('HOST_FAILED');
      return applyMode==='wrong-stamp'?'wrong':stamp();
    }};
  session=createBookLibrarySession({store,host,sources:legacy?null:sources,confirm:()=>true,onState,
    timers:{setTimeout(fn){jobs.set(++timer,fn);return timer;},clearTimeout(id){jobs.delete(id);}}});
  return {session,store,host,sources,settings,assets,jobs,
    get files(){return copy(tree)},get applied(){return applied},get captures(){return captured},
    get identity(){return identity},set mode(value){applyMode=value},
    edit(next){tree=copy(next);revision++;session.changed();},
    silent(next){tree=copy(next)},raw(value){raw=value;session.changed()},block(value){blocked=value},
    replace(){identity++;revision++;session.detach()},
    async saved(next,name='Incoming'){const target=session.state.binding;return store.save({id:target.id,
      expectedRevision:store.records.get(target.id).entry.revision,name,
      project:{schemaVersion:4,files:copy(next),options:{title:'Incoming',page:{width:800}}}});},
    close(){session.dispose()},
  };
}
async function linked(files,options) {const h=harness(files,options);await h.session.save();return h;}
class Element {
  constructor(tag='div'){this.tagName=tag;this.id='';this.children=[];this.parentElement=null;this.listeners=new Map();this.attributes={};
    this._value='';this.textContent='';this.disabled=false;this.hidden=false;this.checked=false;}
  set value(value){this._value=this.tagName==='textarea'?view(String(value)):String(value)}get value(){return this._value}
  appendChild(node){node.parentElement=this;this.children.push(node);return node}
  replaceChildren(...nodes){for(const node of this.children)node.parentElement=null;this.children=[];for(const node of nodes)this.appendChild(node)}
  setAttribute(key,value){this.attributes[key]=value}
  querySelector(selector){if(selector==='#'+this.id)return this;for(const child of this.children){const found=child.querySelector(selector);if(found)return found}return null}
  addEventListener(type,fn){if(!this.listeners.has(type))this.listeners.set(type,[]);this.listeners.get(type).push(fn)}
  removeEventListener(type,fn){this.listeners.set(type,(this.listeners.get(type)??[]).filter(value=>value!==fn))}
  emit(type,extra={}){const event={target:this,defaultPrevented:false,preventDefault(){this.defaultPrevented=true},...extra};
    for(let node=this;node;node=node.parentElement)for(const fn of [...node.listeners.get(type)??[]])fn(event);return event}
  focus(){let root=this;while(root.parentElement)root=root.parentElement;root.activeElement=this}
}
async function uiFixture(files=initial(),{legacy=false}={}) {
  const root=new Element('document'),window=new Element('window'),parent=root.appendChild(new Element());
  root.createElement=tag=>new Element(tag);const store=storage(),notices=new Set(),busyNotices=new Set();
  let tree=copy(files),revision=1,identity=1,selected=0,busy=false,applied=0,captured=0;
  const settings={title:'Local'},assets=new Set(['image','font']),undo=[];
  const ids=['library-books','library-versions','library-name','library-save','library-copy','library-autosave','library-open','library-delete','library-refresh','library-new','library-status','library-current',
    'chapter-source','chapter-path','source-role','chapters','title','author','lang','font','dark-mode','font-scale','toc','page-numbers','unapplied-setting'];
  for(const id of ids)parent.appendChild(new Element(id==='chapter-source'?'textarea':'div')).id=id;
  const el=id=>root.querySelector('#'+id);
  const show=()=>{const f=tree[selected];el('chapter-source').value=f?.source??'';el('chapter-path').value=f?.path??'';el('source-role').value=f?.role??'chapter';};
  const checkpoint=()=>JSON.stringify([revision,identity,selected,...['chapter-source','chapter-path','source-role','title','unapplied-setting'].map(id=>el(id).value)]);
  const notify=()=>{for(const fn of notices)fn()};
  const controls={checkpoint,get currentChapter(){return selected},get sourceBusy(){return busy},
    subscribeSourceState(fn){busyNotices.add(fn);return()=>busyNotices.delete(fn)},
    captureProject(){captured++;if(el('title').value==='invalid')throw error('INVALID_OPTIONS');
      const f=tree[selected];if(f&&el('chapter-source').value!==view(f.source)){tree[selected]={...f,source:el('chapter-source').value};revision++;notify()}
      return {schemaVersion:4,files:copy(tree),options:copy(settings)};},
    replaceProject(project,expected){assert.equal(expected,checkpoint());tree=copy(project.files);identity++;revision++;selected=0;assets.clear();show();notify()},
  };
  if(!legacy)controls.applySourceTree=(next,expected)=>{assert.equal(expected,checkpoint());if(busy)throw error('BOOK_BUSY');
    undo.push(copy(tree));tree=copy(next);applied++;revision++;selected=Math.min(selected,Math.max(0,tree.length-1));show();notify();};
  const collection={get files(){return copy(tree)},get sourceProjectRevision(){return identity},
    subscribe(fn){notices.add(fn);return()=>notices.delete(fn)},get options(){throw Error('Merge must not read settings')},
    get images(){throw Error('Merge must not read grants')},get fonts(){throw Error('Merge must not read fonts')}};
  show();window.confirm=()=>{throw Error('Source merge uses in-panel review')};
  const ui=createBookLibraryControls({root,window,controls,collection,store,confirm:()=>true});await ui.ready;
  return {ui,root,window,el,controls,store,settings,assets,undo,notices,busyNotices,
    get files(){return copy(tree)},get applied(){return applied},get captures(){return captured},
    edit(next){tree=copy(next);revision++;show();notify()},
    raw(id,text,event=null){el(id).value=text;if(event)el(id).emit(event)},
    busy(value){busy=value;for(const fn of busyNotices)fn()},
    replace(){identity++;revision++;show();notify()},
    async saved(next,name='Other tab'){const target=ui.session.state.binding;await store.save({id:target.id,
      expectedRevision:store.records.get(target.id).entry.revision,name,project:{schemaVersion:4,files:copy(next),options:{title:'Incoming'}}});},
    async begin(){el('library-reconcile').emit('click');await until(()=>!el('library-merge-approve').hidden||!ui.session.state.busy);
      assert.equal(el('library-merge-approve').hidden,false,el('library-status').textContent)},
    choose(index,value){el('library-merge-choice-'+index).value=value;el('library-merge-choice-'+index).emit('change')},
    async approve(){el('library-merge-approve').emit('click');await until(()=>!ui.session.state.busy)},
    close(){ui.dispose()},
  };
}
async function linkedUI(files,options){const h=await uiFixture(files,options);await h.ui.session.save();return h;}
`;

const cases = {
  "session: construction and refresh never merge, save or restore automatically": async () => {
    const h=harness();await h.session.refresh();assert.equal(h.session.state.canReconcile,false);
    await assert.rejects(h.session.reconcile(()=>[]),{code:'LIBRARY_RECONCILE_UNAVAILABLE'});
    assert.equal(h.applied,0);assert.equal(h.store.writes.length,0);h.close();
  },
  "session: legacy hosts preserve save and recovery without advertising merge": async () => {
    const h=await linked(undefined,{legacy:true});assert.equal(h.session.state.canReconcile,false);
    await assert.rejects(h.session.reconcile(()=>[]),{code:'LIBRARY_RECONCILE_UNAVAILABLE'});
    h.edit([file('a.md','edit')]);await h.session.save();assert.equal(h.session.state.binding.revision,2);h.close();
  },
  "session: disjoint text and structural changes install once and do not save": async () => {
    const h=await linked(),base=h.files;
    const local=[file('a.md','A\nb\nc\nd\ne\n'),...base.slice(1)];h.edit(local);
    await h.saved([file('a.md','a\nb\nc\nd\nE\n'),...base.slice(1),file('new.md','new')]);
    const writes=h.store.writes.length,captures=h.captures,settings=copy(h.settings),assets=[...h.assets];
    let review;const receipt=await h.session.reconcile(value=>{review=value;return []});
    assert.equal(review.scope,'sources-only');assert.equal(review.baseRevision,1);assert.equal(review.revision,2);
    assert(Object.isFrozen(review));assert.equal(h.applied,1);assert.equal(h.files[0].source,'A\nb\nc\nd\nE\n');
    assert.equal(h.files[2].path,'new.md');assert.equal(h.files[1].role,'include');assert.equal(receipt.saved,false);
    assert.equal(h.store.writes.length,writes);assert.equal(h.captures,captures);assert.deepEqual(h.settings,settings);assert.deepEqual([...h.assets],assets);
    assert.equal(h.session.state.dirty,true);assert.equal(h.session.state.autosave,false);assert.equal(h.session.state.binding.revision,2);h.close();
  },
  "session: a conflict can be reconciled then saved using the new compare-and-swap revision": async () => {
    const h=await linked();h.edit([file('a.md','ours')]);await h.saved([file('a.md','theirs')]);
    await assert.rejects(h.session.save(),{code:'LIBRARY_CONFLICT'});
    await h.session.reconcile(value=>choices(value,'incoming'));assert.equal(h.files[0].source,'theirs');
    assert.equal(h.session.state.conflict,false);await h.session.save();
    assert.equal(h.store.writes.at(-1).expectedRevision,2);assert.equal(h.store.writes.at(-1).project.options.title,'Local');
    assert.equal(h.session.state.binding.revision,3);h.close();
  },
  "session: autosave timers are cancelled and never resumed by reconciliation": async () => {
    const h=await linked();h.session.setAutosave(true);h.edit([file('a.md','local')]);assert.equal(h.jobs.size,1);
    await h.session.reconcile(()=>[]);assert.equal(h.jobs.size,0);assert.equal(h.session.state.autosave,false);
    assert.equal(h.store.writes.length,1);h.close();
  },
  "session: cancelled review preserves source, binding and original common ancestor": async () => {
    const h=await linked(),base=h.files;h.edit([file('a.md','A\nb\nc\nd\ne\n'),base[1]]);
    await h.saved([file('a.md','a\nb\nc\nd\nE\n'),base[1]]);
    assert.equal(await h.session.reconcile(()=>false),null);assert.equal(h.session.state.binding.revision,1);assert.equal(h.applied,0);
    await h.session.reconcile(()=>[]);assert.equal(h.files[0].source,'A\nb\nc\nd\nE\n');h.close();
  },
  "session: regional decisions retain independent edits from both sides": async () => {
    const h=await linked([file('a.md','a\nb\nc\nd\ne\n')]);h.edit([file('a.md','A\nb\nlocal\nd\ne\n')]);
    await h.saved([file('a.md','a\nb\nsaved\nd\nE\n')]);
    await h.session.reconcile(value=>value.conflicts.map(c=>({index:c.index,choice:'regions',regions:c.hunks.map(h=>({index:h.index,choice:'disk'}))})));
    assert.equal(h.files[0].source,'A\nb\nsaved\nd\nE\n');h.close();
  },
  "session: membership decisions can empty the editor but never delete saved records": async () => {
    const h=await linked([file('a.md','old')]);h.edit([file('a.md','edited')]);await h.saved([]);
    await h.session.reconcile(value=>choices(value,'incoming'));assert.deepEqual(h.files,[]);assert.equal(h.store.records.size,1);
    assert.equal(h.store.records.values().next().value.entry.revision,2);h.close();
  },
  "session: newer editor input during review cannot be overwritten": async () => {
    const h=await linked();await h.saved([file('a.md','incoming')]);
    await assert.rejects(h.session.reconcile(()=>{h.edit([file('a.md','newer')]);return []}),{code:'STALE_SOURCE'});
    assert.equal(h.files[0].source,'newer');assert.equal(h.applied,0);assert.equal(h.session.state.binding.revision,1);h.close();
  },
  "session: exact tree comparison catches a lying unchanged checkpoint": async () => {
    const h=await linked();await h.saved([file('a.md','incoming')]);
    await assert.rejects(h.session.reconcile(()=>{h.silent([file('a.md','silent')]);return []}),{code:'STALE_SOURCE'});
    assert.equal(h.files[0].source,'silent');assert.equal(h.applied,0);h.close();
  },
  "session: a source change during the initial asynchronous read invalidates the review": async () => {
    const h=await linked();await h.saved([file('a.md','incoming')]);const gate=deferred();h.store.readHook=()=>gate.promise;
    const work=h.session.reconcile(()=>{assert.fail('stale review callback')});work.catch(()=>{});h.edit([file('a.md','newer')]);gate.resolve();
    await assert.rejects(work,{code:'STALE_SOURCE'});assert.equal(h.applied,0);h.close();
  },
  "session: metadata or local name changes also retire a pending source review": async () => {
    for(const change of [h=>h.raw('draft'),h=>h.session.rename('Renamed')]){
      const h=await linked();await h.saved([file('a.md','incoming')]);
      await assert.rejects(h.session.reconcile(()=>{change(h);return []}),{code:'STALE_SOURCE'});assert.equal(h.applied,0);h.close();
    }
  },
  "session: byte-identical project replacement cannot inherit old merge authority": async () => {
    const h=await linked();await h.saved([file('a.md','incoming')]);
    await assert.rejects(h.session.reconcile(()=>{h.replace();return []}),{code:'STALE_SOURCE'});
    assert.equal(h.session.state.binding,null);assert.equal(h.applied,0);h.close();
  },
  "session: suspension and disposal stop pending approvals without late installation": async () => {
    for(const dispose of [false,true]){const h=await linked();await h.saved([file('a.md','incoming')]);
      await assert.rejects(h.session.reconcile(()=>{if(dispose)h.close();else h.session.suspend();return []}),
        {code:dispose?'LIBRARY_CLOSED':'LIBRARY_SUSPENDED'});assert.equal(h.applied,0);h.close();}
  },
  "session: composition and imports block merge before reading storage and during approval": async () => {
    const h=await linked();h.block(true);await assert.rejects(h.session.reconcile(()=>[]),{code:'BOOK_BUSY'});assert.equal(h.store.reads.length,0);
    h.block(false);await h.saved([file('a.md','incoming')]);
    await assert.rejects(h.session.reconcile(()=>{h.session.composition(true);return []}),{code:'STALE_SOURCE'});
    assert.equal(h.applied,0);h.session.composition(false);h.close();
  },
  "session: saved revision advancing during human review cannot partially install": async () => {
    const h=await linked();await h.saved([file('a.md','incoming')]);
    await assert.rejects(h.session.reconcile(async()=>{await h.saved([file('a.md','newer incoming')]);return []}),{code:'LIBRARY_CONFLICT'});
    assert.equal(h.session.state.binding.revision,1);assert.equal(h.applied,0);h.close();
  },
  "session: storage same-revision content tampering is rejected by exact readback": async () => {
    const h=await linked();const receipt=await h.saved([file('a.md','incoming')]);
    await assert.rejects(h.session.reconcile(()=>{h.store.records.get(receipt.id).snapshots.get(2).project.files[0].source='tampered';return []}),{code:'LIBRARY_CONFLICT'});
    assert.equal(h.applied,0);h.close();
  },
  "session: a write after final review is caught by the next save CAS": async () => {
    const h=await linked();await h.saved([file('a.md','incoming')]);await h.session.reconcile(()=>[]);
    await h.saved([file('a.md','latest')]);await assert.rejects(h.session.save(),{code:'LIBRARY_CONFLICT'});
    assert.equal(h.files[0].source,'incoming');assert.equal((await h.store.read(h.session.state.binding.id)).project.files[0].source,'latest');h.close();
  },
  "session: cached ancestor survives store retention pruning without guessing": async () => {
    const h=await linked([file('a.md','a\nb\nc\n')]);h.edit([file('a.md','A\nb\nc\n')]);
    for(let i=0;i<7;i++)await h.saved([file('a.md','a\nb\nC'+i+'\n')]);
    assert.equal(h.store.records.values().next().value.snapshots.has(1),false);
    await h.session.reconcile(()=>[]);assert.equal(h.files[0].source,'A\nb\nC6\n');h.close();
  },
  "session: historical recovery reads the observed head, not the displayed older snapshot, as its ancestor": async () => {
    const h=await linked([file('a.md','a\nb\nc\n')]);await h.saved([file('a.md','A\nb\nc\n')]);const id=h.session.state.binding.id;
    await h.session.open(id,1);await h.saved([file('a.md','A\nb\nC\n')]);
    await h.session.reconcile(()=>[]);assert(h.store.reads.some(item=>item.revision===2));assert.equal(h.files[0].source,'a\nb\nC\n');h.close();
  },
  "session: pruned lazy ancestors fail closed and never substitute a different snapshot": async () => {
    const h=await linked([file('a.md','base')]);await h.saved([file('a.md','second')]);await h.session.open(h.session.state.binding.id,1);
    for(let i=0;i<6;i++)await h.saved([file('a.md','later '+i)]);
    await assert.rejects(h.session.reconcile(()=>{assert.fail('no guessed review')}),{code:'LIBRARY_CONFLICT'});
    assert.equal(h.files[0].source,'base');assert.equal(h.applied,0);h.close();
  },
  "session: a second reconciliation uses the saved incoming tree, not the unsaved merged tree, as baseline": async () => {
    const h=await linked([file('a.md','a\nb\nc\nd\ne\n')]);h.edit([file('a.md','A\nb\nc\nd\ne\n')]);
    await h.saved([file('a.md','a\nb\nc\nd\nE\n')]);await h.session.reconcile(()=>[]);
    await h.saved([file('a.md','a\nb\nC\nd\nE\n')]);await h.session.reconcile(()=>[]);
    assert.equal(h.files[0].source,'A\nb\nC\nd\nE\n');h.close();
  },
  "session: saving older captured text cannot attach newer edits to the wrong common ancestor": async () => {
    const h=harness([file('a.md','a\nb\nc\n')]),gate=deferred();h.store.saveHook=()=>gate.promise;
    const saving=h.session.save();h.edit([file('a.md','A\nb\nc\n')]);gate.resolve();await saving;h.store.saveHook=null;
    await h.saved([file('a.md','a\nb\nC\n')]);await h.session.reconcile(()=>[]);assert.equal(h.files[0].source,'A\nb\nC\n');h.close();
  },
  "session: invalid editor drafts are rejected before storage and invalid choices before installation": async () => {
    const h=await linked();h.edit([file('../bad.md','x')]);await assert.rejects(h.session.reconcile(()=>[]),{code:'INVALID_SOURCE_TREE_MERGE'});
    assert.equal(h.store.reads.length,0);h.edit([file('a.md','ours')]);await h.saved([file('a.md','theirs')]);
    for(const decision of [true,[],[{index:0,choice:'edited',source:'\ud800'}]])
      await assert.rejects(h.session.reconcile(()=>decision),{code:'INVALID_SOURCE_TREE_MERGE'});
    assert.equal(h.applied,0);h.close();
  },
  "session: oversized edited results cannot advance binding or partially update sources": async () => {
    const h=await linked([file('a.md','base')]);h.edit([file('a.md','ours')]);await h.saved([file('a.md','theirs')]);
    await assert.rejects(h.session.reconcile(()=>[{index:0,choice:'edited',source:'x'.repeat(4194305)}]),{code:'BOOK_LIMIT'});
    assert.equal(h.session.state.binding.revision,1);assert.equal(h.applied,0);h.close();
  },
  "session: unchanged rejected host transactions remain retryable": async () => {
    const h=await linked();await h.saved([file('a.md','incoming')]);h.mode='reject';
    await assert.rejects(h.session.reconcile(()=>[]),{code:'HOST_REFUSED'});assert.equal(h.session.state.binding.revision,1);
    h.mode=null;await h.session.reconcile(()=>[]);assert.equal(h.files[0].source,'incoming');h.close();
  },
  "session: uncertain, partial or asynchronous host transactions retire save authority": async () => {
    for(const mode of ['promise','silent','identity','partial','throw','wrong-stamp']){
      const h=await linked();await h.saved([file('a.md','incoming')]);h.mode=mode;
      await assert.rejects(h.session.reconcile(()=>[]),{code:'LIBRARY_RECONCILE_UNCERTAIN'});
      assert.equal(h.session.state.binding,null,mode);assert.equal(h.session.state.dirty,true);assert.equal(h.session.state.autosave,false);h.close();
    }
  },
  "session: no-source-change approval acknowledges the incoming head without a fake source transaction": async () => {
    const h=await linked();await h.saved(h.files);const receipt=await h.session.reconcile(()=>[]);
    assert.equal(receipt.outcome,'unchanged');assert.equal(h.applied,0);assert.equal(h.session.state.binding.revision,2);assert.equal(h.session.state.dirty,true);h.close();
  },
  "session: invalid metadata drafts do not block source reconciliation or get committed": async () => {
    const h=await linked();h.raw('invalid');await h.saved([file('a.md','incoming')]);const count=h.captures;
    await h.session.reconcile(()=>[]);assert.equal(h.captures,count);assert.equal(h.files[0].source,'incoming');
    await assert.rejects(h.session.save(),{code:'INVALID_OPTIONS'});h.close();
  },
  "session: separate-copy saves replace the ancestor and library identity together": async () => {
    const h=await linked();const old=h.session.state.binding.id;h.edit([file('a.md','a\nb\nc\n')]);await h.session.save({copy:true});
    assert.notEqual(h.session.state.binding.id,old);h.edit([file('a.md','A\nb\nc\n')]);
    await h.saved([file('a.md','a\nb\nC\n')]);await h.session.reconcile(()=>[]);assert.equal(h.files[0].source,'A\nb\nC\n');h.close();
  },
  "controls: startup installs explicit source-only review without writing, importing or reading resources": async () => {
    const h=await uiFixture();assert(h.el('library-reconcile').disabled);assert(h.el('library-merge-approve').hidden);
    assert.match(h.el('library-merge-help').textContent,/incoming settings are NOT imported/);
    assert.equal(h.store.writes.length,0);assert.equal(h.applied,0);assert.equal(h.captures,0);h.close();
  },
  "controls: legacy hosts retain original library behavior without a broken merge action": async () => {
    const h=await linkedUI(undefined,{legacy:true});assert.equal(h.el('library-merge'),null);
    assert.equal(h.ui.session.state.binding.revision,1);h.close();
  },
  "controls: real review button merges through one source-only host transaction": async () => {
    const h=await linkedUI();const base=h.files;h.edit([file('a.md','A\nb\nc\nd\ne\n'),base[1]]);
    await h.saved([file('a.md','a\nb\nc\nd\nE\n'),base[1],file('extra.md','extra')]);
    const captures=h.captures,writes=h.store.writes.length;await h.begin();assert.equal(h.applied,0);
    assert.match(h.el('library-merge-review').textContent,/add: extra.md/);assert.equal(h.el('library-merge-approve').disabled,false);
    await h.approve();assert.equal(h.files[0].source,'A\nb\nc\nd\nE\n');assert.equal(h.applied,1);assert.equal(h.undo.length,1);
    assert.equal(h.captures,captures);assert.equal(h.store.writes.length,writes);assert.deepEqual([...h.assets],['image','font']);h.close();
  },
  "controls: regional selection requires every choice and retains other independent edits": async () => {
    const h=await linkedUI([file('a.md','a\nb\nc\nd\ne\n')]);h.edit([file('a.md','A\nb\nours\nd\ne\n')]);
    await h.saved([file('a.md','a\nb\ntheirs\nd\nE\n')]);await h.begin();assert(h.el('library-merge-approve').disabled);
    h.choose(0,'regions');assert(h.el('library-merge-approve').disabled);
    h.el('library-merge-region-0-0').value='disk';h.el('library-merge-region-0-0').emit('change');
    await h.approve();assert.equal(h.files[0].source,'A\nb\ntheirs\nd\nE\n');h.close();
  },
  "controls: untouched manual text preserves exact imported BOM and CRLF bytes": async () => {
    const h=await linkedUI([file('a.md','base')]);const exact='\ufeffours\r\n日本 😀\rEnd';h.edit([file('a.md',exact)]);
    await h.saved([file('a.md','theirs')]);await h.begin();h.choose(0,'edited');assert.equal(h.el('library-merge-source-0').value,view(exact));
    await h.approve();assert.equal(h.files[0].source,exact);assert.equal(h.applied,0);h.close();
  },
  "controls: undispatched manual input is captured and empty replacement is not source removal": async () => {
    const h=await linkedUI([file('a.md','base')]);h.edit([file('a.md','ours')]);await h.saved([file('a.md','theirs')]);
    await h.begin();h.choose(0,'edited');h.el('library-merge-source-0').value='';await h.approve();
    assert.equal(h.files.length,1);assert.equal(h.files[0].source,'');h.close();
  },
  "controls: source-editor raw input before review is captured without applying settings drafts": async () => {
    const h=await linkedUI([file('a.md','a\nb\nc\n')]);h.raw('chapter-source','A\nb\nc\n');h.raw('title','invalid');
    await h.saved([file('a.md','a\nb\nC\n')]);const captures=h.captures;await h.begin();await h.approve();
    assert.equal(h.files[0].source,'A\nb\nC\n');assert.equal(h.el('title').value,'invalid');assert.equal(h.captures,captures);h.close();
  },
  "controls: raw chapter role changes are retained alongside incoming text edits": async () => {
    const h=await linkedUI([file('a.md','base')]);h.raw('source-role','include');await h.saved([file('a.md','incoming')]);
    await h.begin();await h.approve();assert.deepEqual(h.files,[file('a.md','incoming','include')]);h.close();
  },
  "controls: invalid raw paths are rejected before any library source read": async () => {
    const h=await linkedUI();h.raw('chapter-path','../bad.md');h.el('library-reconcile').emit('click');await until(()=>!h.ui.session.state.busy);
    assert.equal(h.store.reads.length,0);assert.equal(h.applied,0);assert.match(h.el('library-status').textContent,/INVALID_SOURCE_TREE_MERGE/);h.close();
  },
  "controls: cancel and Escape settle review without changing sources or common revision": async () => {
    for(const escape of [false,true]){const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();
      if(escape)assert(h.root.emit('keydown',{key:'Escape'}).defaultPrevented);else h.el('library-merge-cancel').emit('click');
      await until(()=>!h.ui.session.state.busy);assert.equal(h.applied,0);assert.equal(h.ui.session.state.binding.revision,1);h.close();}
  },
  "controls: source changes promptly release pending review and cannot overwrite the new draft": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();h.raw('chapter-source','newer','input');
    await until(()=>!h.ui.session.state.busy);assert(h.el('library-merge-approve').hidden);assert.equal(h.el('chapter-source').value,'newer');
    assert.equal(h.applied,0);h.close();
  },
  "controls: undispatched editor changes are rejected at final approval": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();h.raw('chapter-source','silent');await h.approve();
    assert.equal(h.applied,0);assert.equal(h.el('chapter-source').value,'silent');assert.match(h.el('library-status').textContent,/STALE_SOURCE/);h.close();
  },
  "controls: external setting-panel changes retire review without importing those settings": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();h.raw('unapplied-setting','draft','input');
    await until(()=>!h.ui.session.state.busy);assert.equal(h.applied,0);assert.equal(h.el('unapplied-setting').value,'draft');h.close();
  },
  "controls: renaming during review preserves the keystroke across cancellation repaint": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();h.raw('library-name','New local name','input');
    await until(()=>!h.ui.session.state.busy);assert.equal(h.applied,0);assert.equal(h.ui.session.state.name,'New local name');
    assert.equal(h.el('library-name').value,'New local name');h.close();
  },
  "controls: an asynchronous source host is observed and cannot retain save authority": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);
    h.controls.applySourceTree=()=>Promise.reject(error('ASYNC_HOST_FAILED'));
    await h.begin();await h.approve();assert.equal(h.ui.session.state.binding,null);assert.equal(h.applied,0);
    assert.match(h.el('library-status').textContent,/LIBRARY_RECONCILE_UNCERTAIN/);await tick();h.close();
  },
  "controls: project identity switches disconnect even when source bytes are identical": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();h.replace();await until(()=>!h.ui.session.state.busy);
    assert.equal(h.ui.session.state.binding,null);assert.equal(h.applied,0);h.close();
  },
  "controls: an import becoming busy retires a pending review and disables new requests": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();h.busy(true);await until(()=>!h.ui.session.state.busy);
    assert.equal(h.applied,0);assert(h.el('library-reconcile').disabled);h.busy(false);assert.equal(h.el('library-reconcile').disabled,false);h.close();
  },
  "controls: manual text composition blocks approval without dropping review choices": async () => {
    const h=await linkedUI([file('a.md','base')]);h.edit([file('a.md','ours')]);await h.saved([file('a.md','theirs')]);await h.begin();h.choose(0,'edited');
    const editor=h.el('library-merge-source-0');editor.emit('compositionstart');assert(h.el('library-merge-approve').disabled);
    editor.value='日本';editor.emit('input');h.el('library-merge-approve').emit('click');assert.equal(h.applied,0);
    editor.emit('compositionend');assert.equal(h.el('library-merge-approve').disabled,false);await h.approve();assert.equal(h.files[0].source,'日本');h.close();
  },
  "controls: source composition cancels reviews and stays blocked until composition ends": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();h.el('chapter-path').emit('compositionstart');
    await until(()=>!h.ui.session.state.busy);assert(h.el('library-reconcile').disabled);assert.equal(h.applied,0);
    h.el('chapter-path').emit('compositionend');assert.equal(h.el('library-reconcile').disabled,false);h.close();
  },
  "controls: selecting another library row never changes the current merge target": async () => {
    const h=await linkedUI();const current=h.ui.session.state.binding.id;
    const other=await h.store.save({id:null,expectedRevision:null,name:'Unrelated',project:{schemaVersion:4,files:[file('other.md')],options:{}}});
    await h.ui.session.refresh();h.el('library-books').value=other.id;h.el('library-books').emit('change');await h.saved([file('a.md','incoming')]);
    await h.begin();await h.approve();assert.equal(h.ui.session.state.binding.id,current);assert.equal(h.files[0].path,'a.md');h.close();
  },
  "controls: order conflicts require an explicit order and keep sources from the other side": async () => {
    const base=['a','b','c'].map(path=>file(path+'.md'));
    const h=await linkedUI(base);h.edit([base[1],base[0],base[2],file('ours.md')]);await h.saved([base[0],base[2],base[1],file('theirs.md')]);
    await h.begin();assert(h.el('library-merge-approve').disabled);h.choose(0,'local');await h.approve();
    assert.deepEqual(h.files.map(f=>f.path),['b.md','a.md','c.md','ours.md','theirs.md']);h.close();
  },
  "controls: source removals are reviewed and can leave an empty editor without deleting saved books": async () => {
    const h=await linkedUI([file('a.md','base')]);h.edit([file('a.md','edited')]);await h.saved([]);await h.begin();h.choose(0,'incoming');await h.approve();
    assert.deepEqual(h.files,[]);assert.equal(h.el('chapter-source').value,'');assert.equal(h.store.records.size,1);h.close();
  },
  "controls: saved-source mutation during review displays a conflict and leaves editor untouched": async () => {
    const h=await linkedUI();await h.saved([file('a.md','incoming')]);const original=h.files;await h.begin();await h.saved([file('a.md','newer')]);await h.approve();
    assert.deepEqual(h.files,original);assert.equal(h.applied,0);assert.equal(h.ui.session.state.conflict,true);assert(h.el('library-save').disabled);h.close();
  },
  "controls: bounded inert excerpts never become replacement source or executable markup": async () => {
    const h=await linkedUI([file('a.md','base')]);const source='<script>attack()</script>\n'+'😀'.repeat(1500);h.edit([file('a.md',source)]);
    await h.saved([file('a.md','incoming')]);await h.begin();
    const group=h.el('library-merge-conflicts').children[0];assert(group.children.some(node=>node.textContent.includes('[Excerpt truncated]')));
    h.choose(0,'local');await h.approve();assert.equal(h.files[0].source,source);h.close();
  },
  "controls: suspend and dispose settle visible reviews and remove listeners without late writes": async () => {
    for(const dispose of [false,true]){const h=await linkedUI();await h.saved([file('a.md','incoming')]);await h.begin();
      if(dispose)h.close();else h.ui.suspend();await until(()=>!h.ui.session.state.busy);assert.equal(h.applied,0);
      if(dispose){assert.equal(h.notices.size,0);assert.equal(h.busyNotices.size,0)}else{h.ui.resume();assert.equal(h.ui.session.state.autosave,false)}h.close();}
  },
};

async function run(name) {
  const context=vm.createContext({fixtureAssert:assert,TextEncoder,TextDecoder,setTimeout,clearTimeout,
    structuredClone:value=>JSON.parse(JSON.stringify(value))});
  const modules=new Map();
  function load(url) {
    if(modules.has(url))return modules.get(url);
    let source;
    if(url.endsWith('/book_worker.mjs'))source='export const bookError=(code,message)=>Object.assign(new Error(message),{code});';
    else if(url.endsWith('/demo/book_library_store.mjs'))source='export function createBookLibraryStore(){throw new Error("Tests must inject explicit revisioned storage")}';
    else source=readFileSync(fileURLToPath(url),'utf8');
    const module=new vm.SourceTextModule(source,{context,identifier:url});modules.set(url,module);return module;
  }
  const module=new vm.SourceTextModule(fixture+'\nawait ('+cases[name].toString()+')();',
    {context,identifier:new URL('./library-reconcile-fixture.mjs',import.meta.url).href});
  await module.link((specifier,referrer)=>load(new URL(specifier,referrer.identifier).href));
  await module.evaluate({timeout:10000});
}
if(process.env.FMD_LIBRARY_MERGE_CASE) {
  await run(process.env.FMD_LIBRARY_MERGE_CASE);
} else for(const name of Object.keys(cases))test(name,async()=>{
  if(typeof vm.SourceTextModule==='function')return run(name);
  const result=spawnSync(process.execPath,['--experimental-vm-modules',fileURLToPath(import.meta.url)],{
    env:{...process.env,FMD_LIBRARY_MERGE_CASE:name},encoding:'utf8',timeout:15000,
  });
  assert.equal(result.status,0,result.stderr||result.stdout);
});
