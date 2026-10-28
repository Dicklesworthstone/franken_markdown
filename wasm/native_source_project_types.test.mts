// Compile-only public API consumer; no engine is loaded by this check.
import {createNativeSourceProject, readNativeSourceProject, createNativeWorkspaceExporter,
  NATIVE_SOURCE_PROJECT_LIMITS, type FmdNativeSourceProjectInput,
  type FmdNativeSourceProjectOutput, type FmdNativeWorkspaceRuntime} from './native_workspace.js';
import {renderPdf, type FmdRenderOutput} from './franken_markdown.js';

const source: FmdNativeSourceProjectOutput = createNativeSourceProject('# Document', {
  font: 'serif', fontScale: 1.125, allowRawHtml: false, metadataEpochSeconds: 0,
  page: {size: 'a4', margins: 36},
  typography: {homogeneous: true, antiriver: false, pareto: true},
  optimalPagination: true, strictPagination: true, baselineGridPt: 13.375,
  microtype: 'expansion',
  pdfImages: [{destination: 'chart.png', bytes: new DataView(new ArrayBuffer(8))}],
  fontAssets: [{slot: 'body-regular', bytes: new Uint8Array([1]), weight: 525}],
});
const inputs: FmdNativeSourceProjectInput[] = [source.text(), source.bytes, source.bytes.buffer as ArrayBuffer, new DataView(new ArrayBuffer(0))];
const project = readNativeSourceProject(inputs[0]);
project.options.title = 'Changed';
project.options.pdfImages?.push({destination: 'next.png', bytes: new Uint8Array([2])});
if (project.options.fontAssets?.[0]) project.options.fontAssets[0].weight = 400;
const reencoded = createNativeSourceProject(project.source, project.options);
const mime: 'application/json' = reencoded.mimeType;
const format: 'native-source-project' = reencoded.format;
const size: number = NATIVE_SOURCE_PROJECT_LIMITS.sourceBytes;
const directPdf: Promise<FmdRenderOutput> = renderPdf(project.source, project.options);
const selectedRuntime: FmdNativeWorkspaceRuntime = {wasm: new Uint8Array(8), bindings: 'trusted source'};
async function render() {
  const exporter = await createNativeWorkspaceExporter(selectedRuntime);
  const output: FmdRenderOutput = exporter.renderProject(reencoded.bytes);
  const legacy: FmdRenderOutput = exporter.render(project.source, project.options);
  return [output, legacy];
}
// @ts-expect-error Executable fields are not accepted as project render settings.
createNativeSourceProject('', {bindings: 'code'});
// @ts-expect-error Project output is data, not native rendering output.
const publication: FmdRenderOutput = source;
// @ts-expect-error The workspace retains the safe-HTML contract.
createNativeSourceProject('', {allowRawHtml: true});
// @ts-expect-error Pagination policy is a boolean, not a truthy string.
createNativeSourceProject('', {strictPagination: 'true'});
// @ts-expect-error Only native microtype modes are admitted.
createNativeSourceProject('', {microtype: 'automatic'});
// @ts-expect-error Blob reading must be explicit before this synchronous API.
readNativeSourceProject(new Blob());
// @ts-expect-error The source result binding cannot be replaced accidentally.
project.source = 'changed';
void [mime, format, size, directPdf, render, publication];
