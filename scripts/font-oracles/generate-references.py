#!/usr/bin/env python3
"""Independent reference generation; dev-only fonttools 4.65 / uharfbuzz 0.56.
Run from repository root. No production dependency, network, or host fonts.
"""
from pathlib import Path
import struct
from fontTools.ttLib import TTFont
from fontTools.pens.recordingPen import RecordingPen
import uharfbuzz as hb
ROOT=Path('fmd-font/fonts')
f=TTFont(ROOT/'test-cff/Bravura.otf'); gs=f.getGlyphSet()
def fingerprint(commands):
    value=0xcbf29ce484222325
    for op,pts in commands:
        payload={'moveTo':b'M','lineTo':b'L','curveTo':b'C','closePath':b'Z'}[op]
        for pt in pts:
            for coordinate in pt: payload+=struct.pack('<d',float(coordinate) if coordinate else 0.0)
        for byte in payload: value=((value^byte)*0x100000001b3)&0xffffffffffffffff
    return value
rows=[]
for gid,name in enumerate(f.getGlyphOrder()):
    pen=RecordingPen();gs[name].draw(pen)
    aw,lsb=f['hmtx'][name]
    rows.append(f'{gid} {aw} {lsb} {fingerprint(pen.value):016x}')
(ROOT/'test-cff/fonttools-reference.txt').write_text('\n'.join(rows)+'\n')
f=hb.Font(hb.Face((ROOT/'test-shaping/FmdShaping.ttf').read_bytes()));f.scale=(1000,1000)
cases=[('latn','ab'),('latn','bb'),('latn','ba'),('latn','fi'),('latn','a\u0301'),('latn','a\u0301\u0307'),('latn','a\u0301b'),('arab','بب'),('arab','ببب'),('arab','باب'),('arab','لا'),('arab','بَب'),('arab','بَبَب'),('arab','بََب')]
rows=[]
for script,text in cases:
    raw=text.encode();b=hb.Buffer();b.add_utf8(raw);b.script=script;b.direction='rtl' if script=='arab' else 'ltr';b.language='ar' if script=='arab' else 'en';hb.shape(f,b)
    clusters=sorted(set(i.cluster for i in b.glyph_infos))+[len(raw)]
    glyphs=[]
    for i,p in zip(b.glyph_infos,b.glyph_positions):
        end=clusters[clusters.index(i.cluster)+1]
        glyphs.append(','.join(map(str,[i.codepoint,i.cluster,end,p.x_advance,p.y_advance,p.x_offset,p.y_offset])))
    rows.append('\t'.join([script,text,';'.join(glyphs)]))
(ROOT/'test-shaping/harfbuzz-reference.tsv').write_text('\n'.join(rows)+'\n')
print('Generated FontTools CFF and HarfBuzz', hb.version_string(), 'shaping references')
