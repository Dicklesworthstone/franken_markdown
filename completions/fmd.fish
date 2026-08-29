# Fish completion for fmd (franken_markdown CLI)
# Generated and maintained for fmd. Pinned by tests/completions_drift_test.rs.

set -l subcommands render capabilities robot-docs verify watch doctor config stats diff book batch help

# Disable file completions by default for the command
complete -c fmd -f

# Global options
complete -c fmd -s h -l help -d "Print help"
complete -c fmd -s V -l version -d "Print version"
complete -c fmd -l json -d "Emit stable machine-readable JSON for command metadata/status"
complete -c fmd -l no-color -d "Disable human color/decorative terminal output"
complete -c fmd -l no-config -d "Ignore native config files for this invocation"
complete -c fmd -l robot-triage -d "Print one machine-readable triage envelope"

# Subcommands
complete -c fmd -n "__fish_use_subcommand" -a render -d "Render a Markdown file (or stdin) to HTML and/or PDF"
complete -c fmd -n "__fish_use_subcommand" -a capabilities -d "Print the stable machine-readable command and feature contract"
complete -c fmd -n "__fish_use_subcommand" -a robot-docs -d "Print in-tool documentation written for coding agents"
complete -c fmd -n "__fish_use_subcommand" -a verify -d "Check a rendered document text layer, anchors, warnings, and overflow"
complete -c fmd -n "__fish_use_subcommand" -a watch -d "Rebuild when the Markdown file, --css, or referenced assets change"
complete -c fmd -n "__fish_use_subcommand" -a doctor -d "Check local build/runtime capabilities and report implementation status"
complete -c fmd -n "__fish_use_subcommand" -a config -d "Read or edit native fmd config"
complete -c fmd -n "__fish_use_subcommand" -a stats -d "Analyze document intelligence, word counts, outline, and health checks"
complete -c fmd -n "__fish_use_subcommand" -a diff -d "Compare two Markdown documents and render semantic visual diff"
complete -c fmd -n "__fish_use_subcommand" -a book -d "Assemble a directory of Markdown files into an HTML site and/or PDF book"
complete -c fmd -n "__fish_use_subcommand" -a batch -d "Render many Markdown inputs in parallel under a bounded worker budget"

# Subcommand: render
complete -c fmd -n "__fish_seen_subcommand_from render" -F
complete -c fmd -n "__fish_seen_subcommand_from render" -l text -d "Raw Markdown text to render directly"
complete -c fmd -n "__fish_seen_subcommand_from render" -l to -a "html pdf both epub svg" -d "Which output(s) to produce"
complete -c fmd -n "__fish_seen_subcommand_from render" -s o -l out -F -d "Output path"
complete -c fmd -n "__fish_seen_subcommand_from render" -l font -a "sans serif" -d "Override body font"
complete -c fmd -n "__fish_seen_subcommand_from render" -l css -F -d "Path to custom stylesheet"
complete -c fmd -n "__fish_seen_subcommand_from render" -l title -d "Document title"
complete -c fmd -n "__fish_seen_subcommand_from render" -l author -d "Document author metadata for PDF"
complete -c fmd -n "__fish_seen_subcommand_from render" -l lang -d "Document language tag"
complete -c fmd -n "__fish_seen_subcommand_from render" -l profile -d "Markdown authoring profile"
complete -c fmd -n "__fish_seen_subcommand_from render" -l allow-html -d "Pass raw HTML in source through"
complete -c fmd -n "__fish_seen_subcommand_from render" -l toc -d "Generate a table of contents"
complete -c fmd -n "__fish_seen_subcommand_from render" -l toc-depth -d "Maximum heading depth for table of contents"
complete -c fmd -n "__fish_seen_subcommand_from render" -l html-font-format -a "woff1 ttf" -d "Font container format for HTML subsets"
complete -c fmd -n "__fish_seen_subcommand_from render" -l interactive-html -d "Generate interactive self-hosting HTML workspace"
complete -c fmd -n "__fish_seen_subcommand_from render" -l self-hosting -d "Generate interactive self-hosting HTML workspace"
complete -c fmd -n "__fish_seen_subcommand_from render" -l font-scale -a "xs sm compact md normal default lg xl 2xl huge 100% 125% 150%" -d "Typographic scale factor or preset"
complete -c fmd -n "__fish_seen_subcommand_from render" -l type-size -a "xs sm compact md normal default lg xl 2xl huge 100% 125% 150%" -d "Typographic scale factor or preset"
complete -c fmd -n "__fish_seen_subcommand_from render" -l search-index -F -d "Write deterministic JSON search index"
complete -c fmd -n "__fish_seen_subcommand_from render" -l fit-to-pages -d "Adaptive page budgeting solver"
complete -c fmd -n "__fish_seen_subcommand_from render" -l target-pages -d "Adaptive page budgeting solver"
complete -c fmd -n "__fish_seen_subcommand_from render" -l microtype -a "off protrusion expansion all" -d "Opt-in microtypography for PDF body paragraphs"
complete -c fmd -n "__fish_seen_subcommand_from render" -l typography-homogeneous -d "Enable gradual adjacent demerits in Knuth-Plass breaker"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-line-numbers -d "Render line numbers in PDF code blocks"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-page-numbers -d "Render running page numbers in PDF bottom margin"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-base-font-size -d "Base body font size override in points"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-heading-scale -d "Per-step heading geometric scale ratio"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-table-font-size -d "Nominal table cell font size override in points"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-image -d "Provide or override local PDF image asset (DEST=PATH)"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-font -d "Host TrueType face for renderer slot (SLOT=PATH)"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-font-weight -d "Pin CSS font-weight for host font slot (WEIGHT)"
complete -c fmd -n "__fish_seen_subcommand_from render" -l max-pdf-image-bytes -d "Maximum bytes accepted per PDF image"
complete -c fmd -n "__fish_seen_subcommand_from render" -l no-remote-images -d "Do not fetch remote http(s) images for PDF"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-a -a "2b off" -d "Emit PDF/A-2b identification"
complete -c fmd -n "__fish_seen_subcommand_from render" -l pdf-a-strict -d "Fail closed on non-conformable PDF/A-2b constructs"
complete -c fmd -n "__fish_seen_subcommand_from render" -l remote-image-timeout-secs -d "Per-image timeout for remote PDF image fetches"
complete -c fmd -n "__fish_seen_subcommand_from render" -l max-input-bytes -d "Maximum Markdown input bytes accepted"

# Subcommand: robot-docs
complete -c fmd -n "__fish_seen_subcommand_from robot-docs" -a guide -d "Print coding-agent guide"

# Subcommand: verify
complete -c fmd -n "__fish_seen_subcommand_from verify" -F
complete -c fmd -n "__fish_seen_subcommand_from verify" -l a11y -d "Restrict findings to accessibility audit"
complete -c fmd -n "__fish_seen_subcommand_from verify" -l links -d "Check external http(s) links"
complete -c fmd -n "__fish_seen_subcommand_from verify" -l links-cache -F -d "Cache file for link results"
complete -c fmd -n "__fish_seen_subcommand_from verify" -l links-timeout-secs -d "Per-link timeout in seconds"
complete -c fmd -n "__fish_seen_subcommand_from verify" -l links-ttl-secs -d "Cache entry age to accept in seconds"

# Subcommand: watch
complete -c fmd -n "__fish_seen_subcommand_from watch" -F
complete -c fmd -n "__fish_seen_subcommand_from watch" -l to -a "html pdf both epub svg" -d "Which output(s) to produce"
complete -c fmd -n "__fish_seen_subcommand_from watch" -s o -l out -F -d "Output path"
complete -c fmd -n "__fish_seen_subcommand_from watch" -l font -a "sans serif" -d "Override body font"
complete -c fmd -n "__fish_seen_subcommand_from watch" -l css -F -d "Custom stylesheet to watch"
complete -c fmd -n "__fish_seen_subcommand_from watch" -l interval -d "Poll and debounce window in milliseconds"
complete -c fmd -n "__fish_seen_subcommand_from watch" -l verbose -d "Extra stderr detail on each rebuild"
complete -c fmd -n "__fish_seen_subcommand_from watch" -l serve -d "Serve loopback HTML preview with auto-reload"
complete -c fmd -n "__fish_seen_subcommand_from watch" -l measure -d "Take N samples, print p95 timings, and exit"

# Subcommand: doctor
complete -c fmd -n "__fish_seen_subcommand_from doctor" -a fonts -d "Audit Markdown corpus glyph coverage"
complete -c fmd -n "__fish_seen_subcommand_from doctor; and __fish_seen_subcommand_from fonts" -l corpus -d "Corpus directory to audit"

# Subcommand: config
complete -c fmd -n "__fish_seen_subcommand_from config" -a "show get set path" -d "Config operation"

# Subcommand: stats
complete -c fmd -n "__fish_seen_subcommand_from stats" -F
complete -c fmd -n "__fish_seen_subcommand_from stats" -l text -d "Raw Markdown text to analyze"
complete -c fmd -n "__fish_seen_subcommand_from stats" -l max-input-bytes -d "Maximum Markdown input bytes accepted"

# Subcommand: diff
complete -c fmd -n "__fish_seen_subcommand_from diff" -F
complete -c fmd -n "__fish_seen_subcommand_from diff" -s o -l out -F -d "Output file path"
complete -c fmd -n "__fish_seen_subcommand_from diff" -l max-input-bytes -d "Maximum Markdown input bytes accepted"

# Subcommand: book
complete -c fmd -n "__fish_seen_subcommand_from book" -F
complete -c fmd -n "__fish_seen_subcommand_from book" -s o -l out-dir -d "Output directory"
complete -c fmd -n "__fish_seen_subcommand_from book" -l to -a "html pdf both epub svg" -d "Which output(s) to produce"
complete -c fmd -n "__fish_seen_subcommand_from book" -l max-input-bytes -d "Maximum Markdown input bytes per file"

# Subcommand: batch
complete -c fmd -n "__fish_seen_subcommand_from batch" -F
complete -c fmd -n "__fish_seen_subcommand_from batch" -l to -a "html pdf both epub svg" -d "Which output(s) to produce"
complete -c fmd -n "__fish_seen_subcommand_from batch" -l out-dir -d "Directory for outputs"
complete -c fmd -n "__fish_seen_subcommand_from batch" -l workers -d "Worker cap"
complete -c fmd -n "__fish_seen_subcommand_from batch" -l batch-mode -a "interactive throughput" -d "Sizing mode"
complete -c fmd -n "__fish_seen_subcommand_from batch" -l mem-budget -d "Soft memory ceiling in bytes"
complete -c fmd -n "__fish_seen_subcommand_from batch" -l timeout -d "Wall-clock deadline in seconds"
