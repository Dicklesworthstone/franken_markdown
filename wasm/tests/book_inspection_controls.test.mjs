// Production panel/controller/report validation. DOM, host and engine are
// explicit doubles; native crypto, Blob, fetch and object-URL lifetimes are real.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createBookInspectionPanel, createBookInspectionControls } from '../demo/book_inspection_controls.mjs';
import { inspectBook } from '../book_inspection.mjs';
const gate = () => { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return { promise, resolve, reject }; };
class Element extends EventTarget {
  constructor(tag, ids) { super(); this.tag=tag; this.ids=ids; this.children=[]; this.attributes={}; this.value=''; this.textContent=''; }
  setAttribute(key,value) { this.attributes[key]=value; if(key==='id') this.ids[value]=this; if(key==='disabled') this.disabled=true; if(key==='hidden') this.hidden=true; if(key==='value') this.value=value; }
  removeAttribute(key) { delete this[key]; delete this.attributes[key]; }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children=children; this.textContent=''; }
  before(value) { this.inserted=value; }
  click() { const event=new Event('click',{cancelable:true}); this.dispatchEvent(event); return event; }
  set innerHTML(_) { throw new Error('Untrusted HTML insertion'); }
}
function structure() { return Object.fromEntries(['headings_total','paragraphs','code_blocks','tables','table_rows','table_cells','lists','list_items','task_items_total','task_items_completed','blockquotes','math_blocks','math_inlines','links_total','links_external','links_internal_anchors','images','footnote_definitions','footnote_references'].map(k=>[k,1])); }
function engine(count=1) {
  return { documentStats(source) { return { schema:'fmd-document-stats-v1', bytes:new TextEncoder().encode(source).length, lines:1,words:3,characters:8,sentences:1,syllables:3,reading_time_secs:1,speaking_time_secs:2,flesch_reading_ease:70,flesch_kincaid_grade:8,reading_ease_label:'Standard',structure:structure(),outline:[{level:1,text:'<script>not markup</script>',slug:'title'}],findings:Array.from({length:count},(_,i)=>({severity:i%2?'info':'warning',code:'source_check',message:`Finding ${i}`})) }; },
    accessibilityAudit() { return {schema_version:'1',target:'pdf',findings:[{code:'missing_alt',detail:'<img onerror=bad()>'}]}; } };
}
function setup(t) {
  const el={}, root={createElement:tag=>new Element(tag,el),createTextNode:text=>({textContent:text}),querySelector:q=>q[0]==='#'?el[q.slice(1)]:q==='[aria-labelledby="publish-title"]'?root.publish:null};
  root.body=new Element('body',el); root.publish=new Element('section',el);
  const panel=createBookInspectionPanel(root);
  for(const id of ['chapter-source','chapter-path','chapters','title','author','lang','font','dark-mode','font-scale','toc','page-numbers']) el[id]=new Element('input',el);
  const subscriptions=new Set(), stateListeners=new Set(), jobs=[], navigations=[];
  let revision=0, active=0, sourceBusy=false, invalid=false, disposed=false, cancelled=0;
  const files=[{path:'one.md',source:'# One'},{path:'nested/two.md',source:'\ufeff# Two\r\n'}];
  const checkpoint=()=>`${revision}:${active}:${el['chapter-source'].value}:${el['font-scale'].value}`;
  const host={get sourceBusy(){return sourceBusy;},checkpoint,captureProject(){if(invalid)throw new Error('Invalid current input');return {files:structuredClone(files)};},
    subscribeSourceState(fn){stateListeners.add(fn);return()=>stateListeners.delete(fn);},
    selectSourceRange(index,start,end,stamp){assert.equal(stamp,checkpoint());navigations.push([index,start,end]);active=index;}};
  const collection={get files(){return structuredClone(files);},subscribe(fn){subscriptions.add(fn);return()=>subscriptions.delete(fn);}};
  const worker={render(f,format,options){const job={...gate(),files:f,format,options};jobs.push(job);return job.promise;},cancel(){cancelled++;},dispose(){disposed=true;}};
  const view=createBookInspectionControls({root,controls:host,collection,worker}); t.after(()=>view.dispose());
  return {view,el,root,panel,jobs,files,navigations,get disposed(){return disposed;},get cancelled(){return cancelled;},
    invalid(value){invalid=value;},change(){revision++;for(const fn of subscriptions)fn();},busy(value){sourceBusy=value;for(const fn of stateListeners)fn();}};
}
async function complete(h,e=engine()) { const p=h.view.run();const job=h.jobs.at(-1);job.resolve({format:'book-inspection',...await inspectBook(e,job.files)});return p; }
function choose(el,value){el.value=value;el.dispatchEvent(new Event('change'));}
test('actual panel is inserted before publishing with unique ids and associated controls',t=>{
  const h=setup(t);assert.equal(h.root.publish.inserted,h.panel);assert.throws(()=>createBookInspectionPanel(h.root),/already exists/);
  assert.equal(h.el['inspection-status'].attributes.role,'status');assert.equal(h.jobs.length,0);
});
test('explicit inspection uses every source chapter and does not transmit asset or settings authority',async t=>{
  const h=setup(t);const r=await complete(h);assert.equal(h.jobs[0].format,'inspection');assert.equal(h.jobs[0].options,undefined);
  assert.deepEqual(h.jobs[0].files,h.files);assert.equal(r.summary.total.words,6);assert.match(h.el['inspection-summary'].textContent,/2\/2 source chapters/);
  assert.equal(h.el['inspection-chapters'].children.length,2);assert.equal(h.navigations.length,0);
});
test('findings, headings and file labels are text, never injected markup',async t=>{
  const h=setup(t);await complete(h);assert.equal(h.el['inspection-findings'].children[1].children[0].textContent.includes('<img'),true);
  assert.equal(h.el['inspection-outline'].children[0].textContent,'<script>not markup</script>');
});
test('source navigation opens the selected chapter without inventing spans or modifying source',async t=>{
  const h=setup(t), original=structuredClone(h.files);await complete(h);choose(h.el['inspection-chapters'],'1');h.el['inspection-source'].click();
  assert.deepEqual(h.navigations,[[1,0,0]]);assert.deepEqual(h.files,original);assert.match(h.el['inspection-status'].textContent,/does not provide exact source spans/);
  h.el['inspection-source'].click();assert.equal(h.navigations.length,2);
});
test('large finding sets are paginated and severity filters reset the page',async t=>{
  const h=setup(t);await complete(h,engine(120));assert.equal(h.el['inspection-findings'].children.length,50);
  h.el['inspection-next'].click();assert.match(h.el['inspection-count'].textContent,/51–100/);
  choose(h.el['inspection-filter'],'warning');assert.match(h.el['inspection-count'].textContent,/1–50 of 61/);
  choose(h.el['inspection-filter'],'error');assert.equal(h.el['inspection-findings'].children.length,0);assert.match(h.el['inspection-count'].textContent,/Check the whole-book summary/);
});
test('failed checks are visible and missing statistics are not shown as zero',async t=>{
  const h=setup(t);await complete(h,{documentStats(){throw 'renderer unavailable';},accessibilityAudit:()=>({schema_version:'1',target:'pdf',findings:[]})});
  assert.match(h.el['inspection-summary'].textContent,/INCOMPLETE/);assert.match(h.el['inspection-metrics'].children[0].textContent,/No zero-valued substitute/);
  choose(h.el['inspection-filter'],'failed');assert.equal(h.el['inspection-findings'].children.length,1);
});
test('inspection report downloads are independently prepared and revoked immediately on edits',async t=>{
  const h=setup(t);await complete(h);assert(h.el['inspection-download'].hidden);h.el['inspection-save'].click();const url=h.el['inspection-download'].href;
  const data=await(await fetch(url)).json();assert.equal(data.schema,'fmd-book-inspection-v1');assert.equal(h.el['inspection-download'].download,'book-inspection.json');
  h.change();assert(h.el['inspection-download'].hidden);await assert.rejects(fetch(url));assert(h.el['inspection-source'].disabled);
});
test('raw DOM changes without events fence both source navigation and report activation',async t=>{
  const h=setup(t);await complete(h);h.el['inspection-save'].click();h.el['font-scale'].value='unreported';
  assert(h.el['inspection-download'].click().defaultPrevented);assert.match(h.el['inspection-status'].textContent,/STALE_INSPECTION/);assert.equal(h.navigations.length,0);
});
test('late old replies cannot replace a newer report',async t=>{
  const h=setup(t),old=h.view.run(),oldRejected=assert.rejects(old,{code:'STALE_INSPECTION'});h.change();await complete(h);const summary=h.el['inspection-summary'].textContent;
  h.jobs[0].resolve({format:'book-inspection',...await inspectBook(engine(),h.jobs[0].files)});await oldRejected;assert.equal(h.el['inspection-summary'].textContent,summary);
});
test('same-size wrong-source reports are rejected even when chapter paths match',async t=>{
  const h=setup(t),pending=h.view.run(),f=structuredClone(h.files);f[0].source='# Two';
  h.jobs[0].resolve({format:'book-inspection',...await inspectBook(engine(),f)});await assert.rejects(pending,{code:'STALE_INSPECTION'});assert(h.el['inspection-save'].disabled);
});
test('invalid current input and source busy state cannot trigger stale analysis',async t=>{
  const h=setup(t);h.invalid(true);await assert.rejects(h.view.run(),/Invalid current input/);assert.equal(h.jobs.length,0);
  h.invalid(false);h.busy(true);assert(h.el['inspection-run'].disabled);await assert.rejects(h.view.run(),{code:'BOOK_BUSY'});
  h.busy(false);assert.equal(h.el['inspection-run'].disabled,false);await complete(h);
});
test('cancel, composition/import transitions and suspension clear results without touching source',async t=>{
  const h=setup(t),original=structuredClone(h.files);await complete(h);h.el['inspection-cancel'].click();assert(h.el['inspection-save'].disabled);
  await complete(h);h.busy(true);assert(h.el['inspection-source'].disabled);h.busy(false);await complete(h);h.view.suspend();
  await assert.rejects(h.view.run(),{code:'INSPECTION_CLOSED'});h.view.resume();assert.deepEqual(h.files,original);assert.equal(h.navigations.length,0);
});
test('dispose terminates inspection and prevents late publication',async t=>{
  const h=setup(t),pending=h.view.run();h.view.dispose();h.jobs[0].resolve({format:'book-inspection',...await inspectBook(engine(),h.files)});
  await assert.rejects(pending,{code:'STALE_INSPECTION'});assert(h.disposed);assert(h.el['inspection-download'].hidden);
});
