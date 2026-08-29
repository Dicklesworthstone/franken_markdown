# Bash completion for fmd (franken_markdown CLI)
# Generated and maintained for fmd. Pinned by tests/completions_drift_test.rs.

_fmd() {
    local cur prev words cword
    _init_completion || return

    local subcommands="render capabilities robot-docs verify watch doctor config stats diff book batch mcp help"
    local global_flags="--json --no-color --no-config --robot-triage --help -h --version -V"

    # Find the current subcommand if one has already been specified
    local cmd=""
    local i=1
    while [[ $i -lt $cword ]]; do
        local word="${words[i]}"
        case "$word" in
            render|capabilities|robot-docs|verify|watch|doctor|config|stats|diff|book|batch|mcp|help)
                cmd="$word"
                break
                ;;
            --*)
                # Skip flags
                ;;
            -*)
                # Skip short flags
                ;;
        esac
        ((i++))
    done

    # If no subcommand given yet, complete global flags or subcommands
    if [[ -z "$cmd" ]]; then
        case "$cur" in
            -*)
                COMPREPLY=($(compgen -W "$global_flags" -- "$cur"))
                return 0
                ;;
            *)
                COMPREPLY=($(compgen -W "$subcommands" -- "$cur"))
                return 0
                ;;
        esac
    fi

    # Subcommand specific completion
    case "$cmd" in
        render)
            case "$prev" in
                --to)
                    COMPREPLY=($(compgen -W "html pdf both epub svg" -- "$cur"))
                    return 0
                    ;;
                --font)
                    COMPREPLY=($(compgen -W "sans serif" -- "$cur"))
                    return 0
                    ;;
                --html-font-format)
                    COMPREPLY=($(compgen -W "woff1 ttf" -- "$cur"))
                    return 0
                    ;;
                --font-scale|--type-size)
                    COMPREPLY=($(compgen -W "xs sm compact md normal default lg xl 2xl huge 100% 125% 150%" -- "$cur"))
                    return 0
                    ;;
                --microtype)
                    COMPREPLY=($(compgen -W "off protrusion expansion all" -- "$cur"))
                    return 0
                    ;;
                --pdf-a)
                    COMPREPLY=($(compgen -W "2b off" -- "$cur"))
                    return 0
                    ;;
                --out|-o|--css|--search-index)
                    _filedir
                    return 0
                    ;;
                --pdf-font|--pdf-image|--title|--author|--lang|--profile|--toc-depth|--fit-to-pages|--target-pages|--pdf-base-font-size|--pdf-heading-scale|--pdf-table-font-size|--pdf-font-weight|--max-input-bytes|--max-pdf-image-bytes|--remote-image-timeout-secs)
                    return 0
                    ;;
            esac
            case "$cur" in
                -*)
                    local render_flags="--text --to --out -o --font --css --title --author --lang --profile --allow-html --toc --toc-depth --html-font-format --interactive-html --self-hosting --font-scale --type-size --search-index --fit-to-pages --target-pages --microtype --typography-homogeneous --pdf-line-numbers --pdf-page-numbers --pdf-base-font-size --pdf-heading-scale --pdf-table-font-size --pdf-image --pdf-font --pdf-font-weight --max-pdf-image-bytes --no-remote-images --pdf-a --pdf-a-strict --remote-image-timeout-secs --max-input-bytes --json --no-color --no-config --robot-triage --help -h --version -V"
                    COMPREPLY=($(compgen -W "$render_flags" -- "$cur"))
                    return 0
                    ;;
                *)
                    _filedir
                    return 0
                    ;;
            esac
            ;;
        capabilities)
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--json --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
            esac
            ;;
        robot-docs)
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--json --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
                *)
                    COMPREPLY=($(compgen -W "guide" -- "$cur"))
                    return 0
                    ;;
            esac
            ;;
        verify)
            case "$prev" in
                --links-cache)
                    _filedir
                    return 0
                    ;;
                --links-timeout-secs|--links-ttl-secs)
                    return 0
                    ;;
            esac
            case "$cur" in
                -*)
                    local verify_flags="--json --a11y --links --links-cache --links-timeout-secs --links-ttl-secs --no-color --no-config --robot-triage --help -h --version -V"
                    COMPREPLY=($(compgen -W "$verify_flags" -- "$cur"))
                    return 0
                    ;;
                *)
                    _filedir
                    return 0
                    ;;
            esac
            ;;
        watch)
            case "$prev" in
                --to)
                    COMPREPLY=($(compgen -W "html pdf both epub svg" -- "$cur"))
                    return 0
                    ;;
                --font)
                    COMPREPLY=($(compgen -W "sans serif" -- "$cur"))
                    return 0
                    ;;
                --out|-o|--css)
                    _filedir
                    return 0
                    ;;
                --interval|--measure)
                    return 0
                    ;;
            esac
            case "$cur" in
                -*)
                    local watch_flags="--to --out -o --font --css --interval --verbose --json --serve --measure --no-color --no-config --robot-triage --help -h --version -V"
                    COMPREPLY=($(compgen -W "$watch_flags" -- "$cur"))
                    return 0
                    ;;
                *)
                    _filedir
                    return 0
                    ;;
            esac
            ;;
        doctor)
            case "$prev" in
                --corpus)
                    _filedir -d
                    return 0
                    ;;
            esac
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--json --corpus --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
                *)
                    COMPREPLY=($(compgen -W "fonts" -- "$cur"))
                    return 0
                    ;;
            esac
            ;;
        config)
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--json --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
                *)
                    COMPREPLY=($(compgen -W "show get set path" -- "$cur"))
                    return 0
                    ;;
            esac
            ;;
        stats)
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--text --json --max-input-bytes --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
                *)
                    _filedir
                    return 0
                    ;;
            esac
            ;;
        diff)
            case "$prev" in
                --out|-o)
                    _filedir
                    return 0
                    ;;
                --max-input-bytes)
                    return 0
                    ;;
            esac
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--out -o --json --max-input-bytes --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
                *)
                    _filedir
                    return 0
                    ;;
            esac
            ;;
        book)
            case "$prev" in
                --out-dir|-o)
                    _filedir -d
                    return 0
                    ;;
                --to)
                    COMPREPLY=($(compgen -W "html pdf both epub svg" -- "$cur"))
                    return 0
                    ;;
                --max-input-bytes)
                    return 0
                    ;;
            esac
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--out-dir -o --to --json --max-input-bytes --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
                *)
                    _filedir -d
                    return 0
                    ;;
            esac
            ;;
        batch)
            case "$prev" in
                --out-dir)
                    _filedir -d
                    return 0
                    ;;
                --to)
                    COMPREPLY=($(compgen -W "html pdf both epub svg" -- "$cur"))
                    return 0
                    ;;
                --batch-mode)
                    COMPREPLY=($(compgen -W "interactive throughput" -- "$cur"))
                    return 0
                    ;;
                --workers|--mem-budget|--timeout)
                    return 0
                    ;;
            esac
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--to --out-dir --workers --batch-mode --mem-budget --timeout --json --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
                *)
                    _filedir
                    return 0
                    ;;
            esac
            ;;
        mcp)
            case "$cur" in
                -*)
                    COMPREPLY=($(compgen -W "--max-input-bytes --json --no-color --no-config --robot-triage --help -h --version -V" -- "$cur"))
                    return 0
                    ;;
            esac
            ;;
    esac
}

complete -F _fmd fmd
