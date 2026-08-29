#compdef fmd

# Zsh completion for fmd (franken_markdown CLI)
# Generated and maintained for fmd. Pinned by tests/completions_drift_test.rs.

_fmd() {
    local context state state_descr line
    typeset -A opt_args

    local -a global_opts=(
        '--json[Emit stable machine-readable JSON for command metadata/status]'
        '--no-color[Disable human color/decorative terminal output]'
        '--no-config[Ignore native config files for this invocation]'
        '--robot-triage[Print one machine-readable triage envelope]'
        '(-h --help)'{-h,--help}'[Print help]'
        '(-V --version)'{-V,--version}'[Print version]'
    )

    _arguments -C \
        $global_opts \
        '1: :->command' \
        '*:: :->args' && return 0

    case $state in
        command)
            local -a subcommands=(
                'render:Render a Markdown file (or stdin) to HTML and/or PDF'
                'capabilities:Print the stable machine-readable command and feature contract'
                'robot-docs:Print in-tool documentation written for coding agents'
                'verify:Check a rendered document text layer, anchors, warnings, and overflow'
                'watch:Rebuild when the Markdown file, --css, or referenced assets change'
                'doctor:Check local build/runtime capabilities and report implementation status'
                'config:Read or edit native fmd config'
                'stats:Analyze document intelligence, word counts, outline, and health checks'
                'diff:Compare two Markdown documents and render semantic visual diff'
                'book:Assemble a directory of Markdown files into an HTML site and/or PDF book'
                'batch:Render many Markdown inputs in parallel under a bounded worker budget'
                'help:Print help for a subcommand'
            )
            _describe -t subcommands 'fmd subcommand' subcommands
            ;;
        args)
            case $line[1] in
                render)
                    _arguments -s \
                        $global_opts \
                        '--text=[Raw Markdown text to render directly]:text:' \
                        '--to=[Which output(s) to produce]:target:(html pdf both epub svg)' \
                        '(-o --out)'{-o,--out}'=[Output path]:output file:_files' \
                        '--font=[Override the configured/default body font]:font:(sans serif)' \
                        '--css=[Path to a custom stylesheet]:CSS file:_files -g "*.css"' \
                        '--title=[Document title]:title:' \
                        '--author=[Document author metadata for PDF]:author:' \
                        '--lang=[Document language tag]:language:' \
                        '--profile=[Markdown authoring profile]:profile:' \
                        '--allow-html[Pass raw HTML in source through instead of escaping]' \
                        '--toc[Generate a table of contents]' \
                        '--toc-depth=[Maximum heading depth for table of contents]:depth:' \
                        '--html-font-format=[Font container format for HTML subsets]:format:(woff1 ttf)' \
                        '(--interactive-html --self-hosting)'{--interactive-html,--self-hosting}'[Generate an interactive, self-hosting single-file HTML workspace]' \
                        '(--font-scale --type-size)'{--font-scale,--type-size}'=[Typographic scale factor or preset]:scale:(xs sm compact md normal default lg xl 2xl huge 100% 125% 150%)' \
                        '--search-index=[Write a deterministic JSON search index]:search index file:_files' \
                        '(--fit-to-pages --target-pages)'{--fit-to-pages,--target-pages}'=[Adaptive page budgeting solver]:pages:' \
                        '--microtype=[Opt-in microtypography for PDF body paragraphs]:microtype:(off protrusion expansion all)' \
                        '--typography-homogeneous[Enable gradual adjacent demerits in Knuth-Plass breaker]' \
                        '--pdf-line-numbers[Render muted line numbers in PDF fenced code blocks]' \
                        '--pdf-page-numbers[Render running page numbers in bottom margin of PDF pages]' \
                        '--pdf-base-font-size=[Base body font size override in points]:points:' \
                        '--pdf-heading-scale=[Per-step heading geometric scale ratio]:ratio:' \
                        '--pdf-table-font-size=[Nominal table cell font size override in points]:points:' \
                        '*--pdf-image=[Provide or override a local PDF image asset]:DEST=PATH:' \
                        '*--pdf-font=[Host TrueType face for a renderer slot]:SLOT=PATH:' \
                        '*--pdf-font-weight=[Pin CSS font-weight for a host font slot]:WEIGHT:' \
                        '--max-pdf-image-bytes=[Maximum bytes accepted per PDF image]:bytes:' \
                        '--no-remote-images[Do not fetch remote http(s) images for PDF]' \
                        '--pdf-a=[Emit PDF/A-2b identification]:profile:(2b off)' \
                        '--pdf-a-strict[Fail closed on non-conformable PDF/A-2b constructs]' \
                        '--remote-image-timeout-secs=[Per-image timeout for remote PDF images]:seconds:' \
                        '--max-input-bytes=[Maximum Markdown input bytes accepted]:bytes:' \
                        '1:input file:_files'
                    ;;
                capabilities)
                    _arguments $global_opts
                    ;;
                robot-docs)
                    _arguments $global_opts '1:command:(guide)'
                    ;;
                verify)
                    _arguments \
                        $global_opts \
                        '--a11y[Restrict findings to accessibility audit]' \
                        '--links[Check external http(s) links]' \
                        '--links-cache=[Cache file for link results]:cache file:_files' \
                        '--links-timeout-secs=[Per-link timeout in seconds]:seconds:' \
                        '--links-ttl-secs=[Cache entry age to accept in seconds]:seconds:' \
                        '1:input file:_files'
                    ;;
                watch)
                    _arguments \
                        $global_opts \
                        '--to=[Which output(s) to produce]:target:(html pdf both epub svg)' \
                        '(-o --out)'{-o,--out}'=[Output path]:output file:_files' \
                        '--font=[Override body font]:font:(sans serif)' \
                        '--css=[Custom stylesheet to watch]:CSS file:_files -g "*.css"' \
                        '--interval=[Poll and debounce window in milliseconds]:ms:' \
                        '--verbose[Extra stderr detail on each rebuild]' \
                        '--serve[Serve a loopback-only HTML preview with live-reload]' \
                        '--measure=[Take N samples, print p95 timings, and exit]:samples:' \
                        '1:input file:_files'
                    ;;
                doctor)
                    _arguments \
                        $global_opts \
                        '--corpus=[Markdown corpus directory to audit]:directory:_files -/' \
                        '1:command:(fonts)'
                    ;;
                config)
                    _arguments \
                        $global_opts \
                        '1:command:(show get set path)' \
                        '2:key:(font dark_mode custom_css page_size margin_top_pt margin_right_pt margin_bottom_pt margin_left_pt emoji_strategy)' \
                        '3:value:'
                    ;;
                stats)
                    _arguments \
                        $global_opts \
                        '--text=[Raw Markdown text to analyze]:text:' \
                        '--max-input-bytes=[Maximum Markdown input bytes]:bytes:' \
                        '1:input file:_files'
                    ;;
                diff)
                    _arguments \
                        $global_opts \
                        '(-o --out)'{-o,--out}'=[Output file path]:output file:_files' \
                        '--max-input-bytes=[Maximum Markdown input bytes]:bytes:' \
                        '1:old file:_files' \
                        '2:new file:_files'
                    ;;
                book)
                    _arguments \
                        $global_opts \
                        '(-o --out-dir)'{-o,--out-dir}'=[Output directory]:directory:_files -/' \
                        '--to=[Which output(s) to produce]:target:(html pdf both epub svg)' \
                        '--max-input-bytes=[Maximum Markdown input bytes per file]:bytes:' \
                        '1:book directory:_files -/'
                    ;;
                batch)
                    _arguments \
                        $global_opts \
                        '--to=[Which output(s) to produce]:target:(html pdf both epub svg)' \
                        '--out-dir=[Directory for outputs]:directory:_files -/' \
                        '--workers=[Worker cap]:workers:' \
                        '--batch-mode=[Sizing mode]:mode:(interactive throughput)' \
                        '--mem-budget=[Soft memory ceiling in bytes]:bytes:' \
                        '--timeout=[Wall-clock deadline in seconds]:seconds:' \
                        '*:input files:_files'
                    ;;
            esac
            ;;
    esac
}

_fmd "$@"
