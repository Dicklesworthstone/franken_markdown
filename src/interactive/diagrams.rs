//! Exact fence source and compiled vector images for offline edits.
//! Capture the initial renderer's result while it is already in hand.

#[derive(Default)]
pub(crate) struct Catalog {
    entries: Vec<[String; 4]>,
    retained: usize,
}

impl Catalog {
    pub(crate) fn capture(&mut self, lang: &str, code: &str, svg: &str, alt: String) {
        const LIMIT: usize = 8 * 1024 * 1024;
        if self.entries.len() >= 128
            || code.len() > 65_536
            || lang.len() > 128
            || alt.len() > 8192
            || svg.len() > 4 * 1024 * 1024
        {
            return;
        }
        // Script-safe JSON can expand one input byte into a six-byte escape.
        // Include delimiters and base64 padding in the retained size budget.
        let bytes = 32 + 6 * (lang.len() + code.len() + alt.len()) + 4 * svg.len().div_ceil(3);
        if bytes > LIMIT - self.retained
            || self
                .entries
                .iter()
                .any(|row| row[0] == lang && row[1] == code)
        {
            return;
        }
        self.entries.push([
            lang.to_owned(),
            code.to_owned(),
            alt,
            crate::html::base64_encode(svg.as_bytes()),
        ]);
        self.retained += bytes;
    }

    pub(crate) fn push_manifest(&self, out: &mut String) {
        if self.entries.is_empty() {
            return;
        }
        out.push_str("<script type=\"application/json\" id=\"fmd-diagram-assets\">[");
        for (i, row) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('[');
            for (column, field) in row.iter().enumerate() {
                if column > 0 {
                    out.push(',');
                }
                if column == 3 {
                    // Encoded by our base64 writer, with no script delimiters.
                    out.push('"');
                    out.push_str(field);
                    out.push('"');
                } else {
                    super::push_source_json(field, out);
                }
            }
            out.push(']');
        }
        out.push_str("]</script>\n");
    }
}
