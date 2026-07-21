# UAX #14 Line Breaking Implementation Design

## Objective

Implement Unicode Line Breaking Algorithm (UAX #14) for CJK + Latin core subset, enabling proper line break opportunities for mixed CJK/Latin text without relying on third-party crates.

## Scope

### In Scope
- UAX #14 Line Break Classes: ID, AL, NU, OP, CL, QU, NS, IS, PR, PO, SY, SP, B2, CM, WJ, JL, JV, JT, HY, BA, BB, BK, CR, LF, NL, CB, EX, GL, ZW, ZWJ
- Core pair rules: ID+ID, OP prohibition, CL prohibition, QU prohibition, Korean syllable (JL+JV/JT)
- Integration with existing pdf.rs and layout.rs paths
- Zero third-party dependency (core library stays pure std)

### Out of Scope
- Full 41-class implementation (Thai, Arabic, Devanagari rules)
- Tailored line breaking (locale-specific overrides)
- Word boundary detection (that's UAX #29)

## Architecture

### Module Structure

```
src/
├── line_break.rs          # NEW: UAX #14 core implementation
│   ├── LineBreakClass enum
│   ├── LB_CLASS_RANGES table
│   ├── line_break_class() function
│   ├── LineBreakOpportunity struct
│   └── line_break_opportunities() iterator
├── pdf.rs                  # MODIFIED: use line_break_opportunities()
└── layout.rs               # MODIFIED: optional use via feature flag
```

### LineBreakClass Enum

Core subset of UAX #14 classes (20 classes covering CJK + Latin):

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LineBreakClass {
    // Break opportunities
    BK,  // Mandatory Break
    CR,  // Carriage Return
    LF,  // Line Feed
    NL,  // Next Line
    SP,  // Space
    B2,  // Break Both
    CB,  // Contingent Break
    ZW,  // Zero Width Space
    
    // Prohibitions
    GL,  // Non-breaking Glue (don't break before or after)
    WJ,  // Word Joiner (don't break inside)
    ZWJ, // Zero Width Joiner
    
    // CJK
    ID,  // Ideographic
    JL,  // Hangul Jamo Leading
    JV,  // Hangul Jamo Vowel
    JT,  // Hangul Jamo Trailing
    
    // Latin/General
    AL,  // Alphabetic
    NU,  // Numeric
    OP,  // Open Punctuation
    CL,  // Close Punctuation
    QU,  // Quotation
    NS,  // Numeric Start
    IS,  // Infix Numeric Separator
    PR,  // Prefix Numeric
    PO,  // Postfix Numeric
    SY,  // Break Symbols
    HY,  // Hyphen
    BA,  // Break After
    BB,  // Break Before
    EX,  // Exclamation
    CM,  // Combining Mark
    
    // Fallback
    XX,  // Unknown
}
```

### Data Table Design

Follow `unicode_punct.rs` pattern: sorted non-overlapping ranges with binary search.

```rust
/// Sorted, non-overlapping ranges of code points grouped by Line Break Class.
/// Generated from Unicode 15.1.0 LineBreak.txt. Binary search for lookup.
static LB_CLASS_RANGES: &[(u32, u32, LineBreakClass)] = &[
    // Example entries (actual table generated from UCD)
    (0x0000, 0x0009, LineBreakClass::BK),
    (0x000A, 0x000A, LineBreakClass::LF),
    (0x000B, 0x000C, LineBreakClass::BK),
    (0x000D, 0x000D, LineBreakClass::CR),
    (0x0020, 0x0020, LineBreakClass::SP),
    // ... full table from LineBreak.txt
    (0x4E00, 0x9FFF, LineBreakClass::ID),  // CJK Unified Ideographs
    (0xAC00, 0xD7A3, LineBreakClass::ID),  // Hangul Syllables
    // ...
];
```

Estimated size: ~600-800 ranges, ~15-20KB binary footprint.

### Classification Function

```rust
/// Returns the UAX #14 Line Break Class for a character.
/// ASCII fast path, then binary search in LB_CLASS_RANGES.
#[must_use]
pub(crate) fn line_break_class(c: char) -> LineBreakClass {
    // ASCII fast path: handle common cases inline
    if c.is_ascii() {
        return match c {
            '\x00'..='\x09' | '\x0B'..='\x0C' => LineBreakClass::BK,
            '\x0A' => LineBreakClass::LF,
            '\x0D' => LineBreakClass::CR,
            ' ' => LineBreakClass::SP,
            '!' => LineBreakClass::EX,
            '"' => LineBreakClass::QU,
            '#' | '$' | '%' | '^' | '&' | '*' => LineBreakClass::PO,
            '(' | '[' | '{' => LineBreakClass::OP,
            ')' | ']' | '}' => LineBreakClass::CL,
            '+' => LineBreakClass::PR,
            ',' => LineBreakClass::IS,
            '-' => LineBreakClass::HY,
            '.' => LineBreakClass::IS,
            '/' => LineBreakClass::SY,
            '0'..='9' => LineBreakClass::NU,
            ':' | ';' => LineBreakClass::IS,
            '<' | '>' => LineBreakClass::QU,
            '=' => LineBreakClass::AL,
            'A'..='Z' | 'a'..='z' => LineBreakClass::AL,
            '\\' | '|' => LineBreakClass::BA,
            '_' => LineBreakClass::AL,
            '~' => LineBreakClass::AL,
            _ => LineBreakClass::XX,
        };
    }
    
    // Non-ASCII: binary search
    let cp = c as u32;
    LB_CLASS_RANGES
        .binary_search_by(|&(lo, hi, _)| {
            if cp < lo { Ordering::Greater }
            else if cp > hi { Ordering::Less }
            else { Ordering::Equal }
        })
        .map(|i| LB_CLASS_RANGES[i].2)
        .unwrap_or(LineBreakClass::XX)
}
```

### Line Break Opportunities

```rust
/// A line break opportunity with position and penalty hint.
#[derive(Clone, Copy, Debug)]
pub struct LineBreakOpportunity {
    /// Character position (0-based) where break may occur.
    /// Position N means break occurs BEFORE character at index N.
    pub pos: usize,
    /// Whether break is mandatory (BK, CR, LF, NL) or optional.
    pub mandatory: bool,
    /// Soft break type for penalty assignment.
    pub break_type: BreakType,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BreakType {
    /// CJK ideographic break (neutral penalty).
    Ideographic,
    /// Space break (free, preferred).
    Space,
    /// Hyphen break (light penalty).
    Hyphen,
    /// Break after (light penalty).
    BreakAfter,
    /// Break before (light penalty, rare).
    BreakBefore,
    /// Emergency fallback (heavy penalty).
    Emergency,
    /// Mandatory break (forced).
    Mandatory,
}

/// Iterates over line break opportunities in text.
/// Uses UAX #14 pair rules to determine break positions.
pub fn line_break_opportunities(text: &str) -> LineBreakIterator<'_> {
    LineBreakIterator::new(text)
}

pub struct LineBreakIterator<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    pos: usize,
    prev_class: Option<LineBreakClass>,
}

impl<'a> Iterator for LineBreakIterator<'a> {
    type Item = LineBreakOpportunity;
    
    fn next(&mut self) -> Option<Self::Item> {
        // Core UAX #14 pair rule logic:
        // 1. Get current char class
        // 2. Apply pair rules with prev_class
        // 3. Emit break opportunity if allowed
        // 4. Update prev_class
        
        loop {
            let c = self.chars.next()?;
            let class = line_break_class(c);
            let char_len = c.len_utf8();
            
            // ... pair rule logic ...
            
            self.pos += char_len;
        }
    }
}
```

### Core Pair Rules

Implement the most impactful rules from UAX #14:

```rust
fn can_break_between(prev: LineBreakClass, next: LineBreakClass) -> bool {
    match (prev, next) {
        // Mandatory breaks
        (BK | CR | LF | NL, _) => true,
        (_, BK | CR | LF | NL) => true,
        
        // ZW creates break opportunity
        (ZW, _) => true,
        
        // Space: break after
        (SP, _) => true,
        (_, SP) => false, // break after space, not before
        
        // Don't break inside QU (quotes)
        (QU, _) if !matches!(next, SP | ZW | BK | CR | LF | NL) => false,
        (_, QU) if !matches!(prev, SP | ZW | BK | CR | LF | NL) => false,
        
        // Don't break before OP (open punctuation)
        (_, OP) => false,
        
        // Don't break after CL (close punctuation)
        (CL, _) => false,
        
        // Don't break after EX (exclamation)
        (EX, _) => false,
        
        // Don't break before EX (exclamation)
        (_, EX) => false,
        
        // Don't break before NS (numeric start)
        (_, NS) => false,
        
        // Don't break after PR (prefix numeric) before numeric
        (PR, NU) => false,
        
        // Don't break after PO (postfix numeric) before numeric
        (PO, NU) => false,
        
        // Don't break before PR after numeric
        (NU, PR) => false,
        
        // Don't break before PO after numeric
        (NU, PO) => false,
        
        // ID + ID: break allowed between ideographs
        (ID, ID) => true,
        (ID, AL | NU) => true,
        (AL | NU, ID) => true,
        
        // Korean syllable formation: don't break
        (JL, JL | JV | H2 | H3) => false,
        (JV | H2, JV | JT) => false,
        (JT | H3, JT) => false,
        
        // B2: break both sides
        (B2, _) => true,
        (_, B2) => true,
        
        // HY/BA: break after
        (HY | BA, _) => true,
        
        // BB: break before
        (_, BB) => true,
        
        // Default: allow break (XX, AL, etc.)
        _ => true,
    }
}
```

### Penalty Mapping

Map `BreakType` to Knuth-Plass penalty values:

| BreakType | Penalty | Reason |
|-----------|---------|--------|
| Mandatory | -∞ (forced) | Forced break |
| Space | 0 | Preferred break point |
| Ideographic | 0 | Neutral, natural CJK break |
| Hyphen | 50 | Slight cost (hyphen renders) |
| BreakAfter | 80 | Less preferred |
| BreakBefore | 100 | Rare, stylistic |
| Emergency | 2000 | Last resort |

## Integration

### pdf.rs Changes

Replace current word break logic in `flush_pdf_word`:

```rust
// BEFORE: pdf_word_break_points() with simple CJK detection
// AFTER: line_break_opportunities() with UAX #14 rules

fn flush_pdf_word(built: &mut BuiltParagraph, word: &mut Vec<Tok>, cx: PdfWordContext<'_>) {
    // ... existing stats computation ...
    
    let chars = pdf_word_chars(word, stats.char_len);
    let text: String = chars.iter().collect();
    
    // Get UAX #14 break opportunities
    let uax_breaks: Vec<LineBreakOpportunity> = line_break_opportunities(&text).collect();
    
    // Convert to PdfBreakPoint with penalties
    let mut points: Vec<PdfBreakPoint> = uax_breaks
        .into_iter()
        .filter(|b| !b.mandatory) // skip mandatory breaks within words (shouldn't happen)
        .map(|b| PdfBreakPoint {
            at: b.pos,
            penalty: penalty_for_break_type(b.break_type),
            hyphen: matches!(b.break_type, BreakType::Hyphen),
        })
        .collect();
    
    // Merge with dictionary hyphenation (if applicable)
    if needs_dictionary {
        let dict_points = /* ... */;
        points.extend(dict_points);
    }
    
    // Fill emergency breaks for gaps
    points = fill_pdf_emergency_break_points(stats.char_len, points);
    
    // ... existing token splitting logic ...
}
```

### layout.rs Changes

Add optional UAX #14-aware word iterator:

```rust
/// Iterator over words with UAX #14 break awareness.
/// Falls back to whitespace-based breaking for simple cases.
pub fn breakable_words_uax14(text: &str) -> BreakableWordsUax14<'_> {
    BreakableWordsUax14::new(text)
}

pub struct BreakableWordsUax14<'a> {
    text: &'a str,
    // ... UAX #14 state ...
}

impl<'a> Iterator for BreakableWordsUax14<'a> {
    type Item = &'a str;
    
    fn next(&mut self) -> Option<Self::Item> {
        // Use line_break_opportunities() to find word boundaries
        // while preserving whitespace for interword glue
    }
}
```

**Decision**: Keep existing `breakable_words` for backward compatibility, add `breakable_words_uax14` as opt-in.

## Data Generation

Generate `LB_CLASS_RANGES` from Unicode Character Database:

```bash
# Download LineBreak.txt from Unicode UCD
curl -O https://www.unicode.org/Public/15.1.0/ucd/LineBreak.txt

# Generate Rust table (script to write)
python scripts/generate_lb_ranges.py LineBreak.txt > src/line_break_generated.rs
```

The generator script:
1. Parses LineBreak.txt
2. Groups contiguous code points by class
3. Sorts ranges, merges adjacent same-class ranges
4. Emits `static LB_CLASS_RANGES: &[(u32, u32, LineBreakClass)]`
5. Includes `#[cfg(test)]` for table validation

## Testing Strategy

### Unit Tests

1. **Classification tests**: Verify `line_break_class()` for key characters
2. **Break opportunity tests**: Verify `can_break_between()` pair rules
3. **Integration tests**: Full text → break opportunities comparison
4. **Table validation**: Ranges sorted, non-overlapping

### Golden Tests

1. CJK text: "你好世界这是一个测试" → break after each character
2. CJK + Latin: "Hello世界测试" → breaks at CJK boundaries
3. Korean: "한글테스트" → respect syllable formation rules
4. Quotes: `"hello world"` → no break inside quotes
5. Numbers: "1,234.56" → no break inside number
6. URL: "https://example.com" → break after punctuation

### PDF Output Tests

Render PDFs with CJK text and verify:
- No overfull boxes
- Line breaks occur at correct positions
- Hyphenation works for Latin words in CJK context

## Implementation Order

1. **Phase 1**: Core infrastructure
   - Add `src/line_break.rs` with `LineBreakClass` enum
   - Implement `line_break_class()` with ASCII fast path
   - Generate `LB_CLASS_RANGES` table
   - Add table validation tests

2. **Phase 2**: Break opportunity logic
   - Implement `LineBreakIterator`
   - Implement core pair rules in `can_break_between()`
   - Add unit tests for pair rules

3. **Phase 3**: pdf.rs integration
   - Modify `flush_pdf_word` to use `line_break_opportunities()`
   - Update `PdfBreakPoint` generation
   - Add integration tests

4. **Phase 4**: layout.rs integration
   - Add `breakable_words_uax14` iterator
   - Add feature flag if needed for backward compat
   - Update documentation

## Success Criteria

1. CJK text renders without overflow
2. Line breaks follow UAX #14 rules
3. Korean syllable boundaries preserved
4. No regression in Latin text rendering
5. Binary size increase < 50KB
6. All existing tests pass
7. New UAX #14 tests pass

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Large data table size | Use range compression, only include needed classes |
| Performance regression | ASCII fast path, profile with flamegraph |
| Incorrect break rules | Extensive testing, compare with reference implementations |
| Breaking change | Keep existing `breakable_words`, add new iterator |

## References

- [UAX #14: Unicode Line Breaking Algorithm](https://www.unicode.org/reports/tr14/)
- [LineBreak.txt (Unicode 15.1.0)](https://www.unicode.org/Public/15.1.0/ucd/LineBreak.txt)
- Existing `unicode_punct.rs` as data table pattern
