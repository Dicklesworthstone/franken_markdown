"""Actual Chromium SVG admission and decoding, using the shipped importer.

Runs no renderer doubles: this checks the browser XML/image APIs and import
pipeline. Native output and saved-workspace ownership have separate gates.
Requires installed Playwright and Chromium; never downloads dependencies.
"""
import argparse
import base64
import json
import shutil
import struct
import tempfile
import zlib
from pathlib import Path
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
SVG_NS = 'http://www.w3.org/2000/svg'
PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8DwHwQBEPgD/U6VwW8AAAAASUVORK5CYII='


def svg(body='', attributes='width="160" height="80"'):
    return f'<svg xmlns="{SVG_NS}" {attributes}>{body}</svg>'


def run(args):
    output = Path(tempfile.mkdtemp(prefix='fmd-svg-import-'))
    reports, errors, network = [], [], []
    accepted = {
        'basic paths': svg('<path d="M0 0L160 80" stroke="blue"/>'),
        'UTF-8 BOM, XML declaration and Unicode': '\ufeff<?xml version="1.0" encoding="UTF-8"?>\n'
            + svg('<title>Chärt 日本語</title><text x="5" y="20">é 中</text>'),
        'viewBox intrinsic size': svg('<circle cx="20" cy="20" r="10"/>', 'viewBox="0 0 160 80"'),
        'percentage intrinsic size': svg('', 'width="100%" height="100%" viewBox="0 0 160 80"'),
        'absolute units': svg('', 'width="1in" height="72pt" viewBox="0 0 160 80"'),
        'fractional size': svg('', 'width="160.5" height="80.25"'),
        'local definitions and styles': svg('<defs><linearGradient id="paint"><stop stop-color="red"/>'
            '<stop offset="1" stop-color="blue"/></linearGradient><path id="line" d="M0 0L100 70"/></defs>'
            '<style>.chart { fill:u\\72l(#paint);stroke:#123 }</style>'
            '<rect class="chart" width="150" height="70"/><use href="#line" stroke="black"/>',
            'width="160" height="80" style="max-width:160px;height:auto"'),
        'XML-decoded paint references': svg('<defs><linearGradient id="p"/></defs>'
            '<rect width="100" height="70" fill="u&#114;l(#p)"/>'),
        'embedded raster': svg(f'<image href="data:image/png;base64,{PNG}" width="80" height="40"/>'),
        'safe native PDF hyperlink': svg('<a href="https://example.invalid/docs"><rect width="160" height="80"/></a>'),
        'xlink and metadata': svg('<metadata><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
            '<rdf:Description>Author metadata</rdf:Description></rdf:RDF></metadata>'
            '<defs><path id="p" d="M0 0L100 70"/></defs><use xlink:href="#p"/>',
            'xmlns:xlink="http://www.w3.org/1999/xlink" width="160" height="80"'),
        'inert comments and CDATA': svg('<!-- <!DOCTYPE svg> -->'
            '<text x="0" y="20"><![CDATA[<!DOCTYPE svg>]]></text>'),
        'prefixed SVG namespace': f'<s:svg xmlns:s="{SVG_NS}" width="160" height="80">'
            '<s:rect width="160" height="80"/></s:svg>',
    }
    def raster(width, height):
        # Complete independent PNG bytes; compress one row at a time so this
        # negative admission test does not allocate a giant raster on the host.
        def chunk(tag, data):
            return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data))
        compressor = zlib.compressobj()
        row = b'\0' * (1 + width * 3)
        compressed = b''.join(compressor.compress(row) for _ in range(height)) + compressor.flush()
        return base64.b64encode(b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', compressed) + chunk(b'IEND', b'')).decode()

    large_raster = raster(4000, 3100) # 12.4M pixels each, under the individual limit.
    expansion = '<defs><g id="part0"><path d="M0 0L1 1"/></g>' + ''.join(
        f'<g id="part{i}"><use href="#part{i-1}"/><use href="#part{i-1}"/></g>' for i in range(1, 17)
    ) + '</defs><use href="#part16"/>'
    refused = {
        'script': svg('<script>globalThis.svgExecuted=true</script>'),
        'event handler': svg('<rect onload="globalThis.svgExecuted=true"/>'),
        'foreign HTML': svg('<foreignObject><div xmlns="http://www.w3.org/1999/xhtml">unsafe</div></foreignObject>'),
        'remote image': svg('<image href="https://example.invalid/image.png" width="80" height="40"/>'),
        'remote xlink': svg('<image xlink:href="//example.invalid/image.png"/>',
            'width="160" height="80" xmlns:xlink="http://www.w3.org/1999/xlink"'),
        'relative external use': svg('<use href="other.svg#shape"/>'),
        'remote filter image': svg('<filter id="f"><feImage href="https://example.invalid/image.png"/></filter>'),
        'external base': svg('<use href="#shape"/>', 'width="160" height="80" xml:base="https://example.invalid/"'),
        'nested SVG data URI': svg('<image href="data:image/svg+xml;base64,PHN2Zy8+"/>'),
        'recursive same-document image': svg('<image href="#same"/>'),
        'script hyperlink': svg('<a href="javascript:alert(1)"><rect width="1" height="1"/></a>'),
        'animated reference': svg('<image id="i"/><set href="#i" attributeName="href" to="https://example.invalid/image.png"/>'),
        'CSS import': svg('<style>@import "https://example.invalid/style.css";</style>'),
        'escaped CSS import': svg('<style>@\\69mport "https://example.invalid/style.css";</style>'),
        'escaped CSS URL': svg('<rect style="fill:u\\72l(https://example.invalid/p.svg#paint)"/>'),
        'quoted URL comment characters': svg('<style>rect{fill:url("/*external-resource*/#paint")}</style>'),
        'escaped URL quote delimiters': svg('<style>rect{fill:url(\\27#paint\\27)}</style>'),
        'unquoted URL comment characters': svg('<style>rect{fill:url(/*ignored*/#paint)}</style>'),
        'Unicode-spaced CSS URL': svg('<style>rect{fill:url(\u00a0#paint)}</style>'),
        'escaped Unicode-spaced CSS URL': svg('<style>rect{fill:url(\\a0#paint)}</style>'),
        'Unicode-spaced image reference': svg('<use href="\u00a0#paint"/>'),
        'Unicode-spaced alternative reference': svg('<use href="\u2000#paint"/>'),
        'CSS image-set': svg('<style>svg{background:image-set("https://example.invalid/image.png" 1x)}</style>'),
        'external font': svg('<style>@font-face{font-family:remote;src:url(https://example.invalid/font.ttf)}</style>'),
        'processing instruction': '<?xml-stylesheet href="https://example.invalid/style.css"?>' + svg(),
        'external document type': '<!DOCTYPE svg SYSTEM "https://example.invalid/svg.dtd">' + svg(),
        'entity declaration': '<!DOCTYPE svg [<!ENTITY text "expand">]>' + svg('<text>&text;</text>'),
        'oversized root': svg('', 'width="16385" height="1"'),
        'oversized pixel area': svg('', 'width="5000" height="5000"'),
        'oversized CSS': svg('<style>svg{width:20000px}</style>'),
        'CSS pixel area': svg('<style>svg{width:5000px;height:5000px}</style>'),
        'unbounded CSS expression': svg('', 'width="160" height="80" style="width:calc(10000px + 10000px)"'),
        'logical CSS size': svg('<style>svg{inline-size:20000px}</style>'),
        'unbounded aspect ratio': svg('<style>svg{aspect-ratio:1/9999999}</style>'),
        'missing size': svg('', ''),
        'negative viewBox': svg('', 'viewBox="0 0 -160 80"'),
        'malformed XML': svg('<path>'),
        'wrong namespace': '<svg xmlns="https://example.invalid/svg" width="160" height="80"/>',
        'duplicate attribute': svg('', 'width="160" width="200" height="80"'),
        'too much nesting': svg('<g>' * 130 + '</g>' * 130),
        'too many nodes': svg('<g/>' * 32769),
        'aggregate raster pixels': svg(f'<image href="data:image/png;base64,{large_raster}"/>' * 2),
        'referenced raster pixel expansion': svg(f'<defs><image id="large" href="data:image/png;base64,{large_raster}"/></defs>'
            '<use href="#large"/><use href="#large"/>'),
        'exponential local reference expansion': svg(expansion),
        'self-referencing use': svg('<g id="self"><use href="#self"/></g>'),
        'indirect reference cycle': svg('<defs><g id="one"><use href="#two"/></g>'
            '<g id="two"><use href="#one"/></g></defs><use href="#one"/>'),
        'encoded reference cycle': svg('<g id="self"><use href="#%73elf"/></g>'),
        'expanded text payload': svg('<defs><g id="text"><text>' + 'a' * 100000 + '</text></g></defs>'
            + '<use href="#text"/>' * 1000),
        'expanded path payload': svg('<defs><path id="path" d="M0 0' + 'L1 1' * 25000 + '"/></defs>'
            + '<use href="#path"/>' * 1000),
        'unterminated repeated comments': svg('<style>' + '/* unclosed ' * 10000 + '</style>'),
        'unterminated repeated URLs': svg('<style>' + 'fill:url(' * 10000 + '</style>'),
    }
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=args.chromium, headless=True, args=['--no-sandbox'])
        context = browser.new_context()
        context.on('request', lambda request: network.append(request.url) if request.url.startswith(('http:', 'https:')) else None)
        context.route('http://**/*', lambda route: route.abort())
        context.route('https://**/*', lambda route: route.abort())
        page = context.new_page()
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.add_script_tag(content=(ROOT / 'src/interactive_import.js').read_text())
        admitted_raster = page.evaluate('''encoded => fmdImportImageInfo(
          Uint8Array.from(atob(encoded), character => character.charCodeAt(0)))''', large_raster)
        assert admitted_raster == {'mime':'image/png', 'width':4000, 'height':3100}, admitted_raster
        reports.append({'name':'aggregate-budget fixture passes individual PNG byte/dimension admission',
                        'proof':'admission only; large raster intentionally not decoded', 'result':admitted_raster})
        for expected, cases in [(True, accepted), (False, refused)]:
            for name, source in cases.items():
                result = page.evaluate('''async source => {
                  let decodes=0, info=null;
                  const original=new TextEncoder().encode(source);
                  const file=new File([original], 'misleading.png', {type:'image/png'});
                  try {
                    const images=await fmdReadImageImport([file], async(uri, metadata) => {
                      decodes++;info=metadata;
                      const image=new Image();image.src=uri;await image.decode();
                      fmdImportDimensions(image.naturalWidth,image.naturalHeight);
                      image.removeAttribute('src');
                    }, ()=>true);
                    return {accepted:true,decodes,info,bytes:btoa(String.fromCharCode(...images[0].bytes)),
                      active:globalThis.svgExecuted===true};
                  } catch(error) {return {accepted:false,decodes,error:String(error.message),active:globalThis.svgExecuted===true};}
                }''', source)
                assert result['accepted'] == expected, f'{name}: {result}'
                assert not result['active'], f'{name}: active SVG code executed'
                if expected:
                    assert result['decodes'] == 1 and result['info']['mime'] == 'image/svg+xml', (name, result)
                    assert base64.b64decode(result['bytes']) == source.encode(), f'{name}: SVG bytes changed'
                else:
                    assert result['decodes'] == 0, f'{name}: unsafe SVG reached the image decoder'
                reports.append({'name':name, 'accepted':expected, 'result':result})
        assert not errors and not network, {'errors':errors, 'requests':network}
        report = {'browser':browser.version, 'passed':len(reports), 'cases':reports, 'errors':errors, 'requests':network}
        (output / 'report.json').write_text(json.dumps(report, indent=2))
        print(json.dumps({'passed':len(reports), 'browser':browser.version, 'artifacts':str(output), 'errors':errors, 'requests':network}))
        browser.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chromium', default=shutil.which('chromium'))
    run(parser.parse_args())
