from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from pathlib import Path
names=['.notdef','space','a','b','f','i','fi','acute','dot','beh','alef','lam','beh.init','beh.medi','beh.fina','lam.init','lam.medi','lam.fina','alef.fina','lam_alef','lam_alef.fina','fatha']
fb=FontBuilder(1000,isTTF=True);fb.setupGlyphOrder(names)
fb.setupCharacterMap({32:'space',97:'a',98:'b',102:'f',105:'i',0x301:'acute',0x307:'dot',0x628:'beh',0x627:'alef',0x644:'lam',0x64e:'fatha'})
glyphs={}
for i,name in enumerate(names):
 p=TTGlyphPen(None)
 if name!='space':
  p.moveTo((20,0));p.lineTo((220,0));p.lineTo((120,400));p.closePath()
 glyphs[name]=p.glyph()
fb.setupGlyf(glyphs);fb.setupHorizontalMetrics({n:(0 if n in ['acute','dot','fatha'] else (300 if n=='space' else 500),20) for n in names})
fb.setupHorizontalHeader(ascent=800,descent=-200);fb.setupNameTable({'familyName':'Fmd Shaping Fixture','styleName':'Regular','uniqueFontIdentifier':'FmdShapingFixture1','fullName':'Fmd Shaping Fixture','psName':'FmdShapingFixture','version':'Version 1.0','copyright':'Copyright 2026 FrankenMarkdown contributors. SIL Open Font License 1.1.'});fb.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200);fb.setupPost();fb.setupMaxp()
fea='''languagesystem latn dflt;
languagesystem arab dflt;
markClass acute <anchor 100 0> @TOP;
markClass dot <anchor 100 0> @TOP;
markClass fatha <anchor 100 0> @TOP;
feature liga { script latn; sub f i by fi; } liga;
feature init { script arab; sub beh by beh.init; sub lam by lam.init; } init;
feature medi { script arab; sub beh by beh.medi; sub lam by lam.medi; } medi;
feature fina { script arab; sub beh by beh.fina; sub lam by lam.fina; sub alef by alef.fina; } fina;
feature rlig { script arab; lookupflag IgnoreMarks; sub lam.init alef.fina by lam_alef; sub lam.medi alef.fina by lam_alef.fina; } rlig;
feature kern { script latn; pos a b -40; lookup ClassPairs { pos [b f] [a b] -20; } ClassPairs; script arab; pos beh.init beh.fina -40; } kern;
feature mark { pos base [a b f i fi beh alef lam beh.init beh.medi beh.fina lam.init lam.medi lam.fina alef.fina lam_alef lam_alef.fina] <anchor 250 500> mark @TOP; } mark;
feature mkmk { pos mark [acute dot fatha] <anchor 100 150> mark @TOP; } mkmk;
'''
addOpenTypeFeaturesFromString(fb.font,fea)
fb.font['head'].created=fb.font['head'].modified=0
fb.font.recalcTimestamp=False
fb.save('fmd-font/fonts/test-shaping/FmdShaping.ttf')
Path('fmd-font/fonts/test-shaping/features.fea').write_text(fea)
