"""Independently validate the generated comparison's anchors and safe DOM."""
from html.parser import HTMLParser
import sys


class Document(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.links = []
        self.sections = []
        self.revisions = set()

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        assert tag not in {"script", "iframe", "object", "embed"}, (tag, attrs)
        assert not any(name.lower().startswith("on") for name, _ in attributes), attrs
        if tag == "section":
            classes = attrs.get("class", "").split()
            side = "old" if "diff-old" in classes else "new" if "diff-new" in classes else None
            self.sections.append(side)
            if side:
                self.revisions.add(side)
        side = next((side for side in reversed(self.sections) if side), None)
        if "id" in attrs:
            anchor = attrs["id"]
            assert anchor not in self.ids, f"duplicate id: {anchor}"
            self.ids.add(anchor)
            if side:
                assert anchor.startswith(f"diff-{side}-"), (side, anchor)
        for name in ("href", "src"):
            value = attrs.get(name, "")
            assert not value.lower().startswith(("javascript:", "vbscript:")), value
            if name == "href" and value.startswith("#") and len(value) > 1:
                self.links.append(value[1:])
                if side:
                    assert value.startswith(f"#diff-{side}-"), (side, value)

    def handle_endtag(self, tag):
        if tag == "section":
            assert self.sections, "unmatched section"
            self.sections.pop()


doc = Document()
doc.feed(sys.stdin.read())
doc.close()
assert not doc.sections, "unclosed section"
assert doc.revisions == {"old", "new"}, doc.revisions
assert all(target in doc.ids for target in doc.links), set(doc.links) - doc.ids
