#!/usr/bin/env bash
# FCB-009/FCB-022 consumer verification document: Shell route.
# Exercises: keywords, builtins, single/double quotes, parameter expansions,
# command substitutions, arithmetic, heredoc operators, escaped newlines,
# and redirection/pipe operators without executing commands.

set -euo pipefail

# Configuration defaults
DEFAULT_PORT=8080
export APP_ENV="${APP_ENV:-production}"
readonly CONFIG_DIR="/etc/app"

log_info() {
    printf "[INFO] %s: %s\n" "$(date +%Y-%m-%d)" "$1"
}

# Quoted strings and parameter expansion
msg="Starting service on port: ${DEFAULT_PORT}"
single_quote_verbatim='No $variable expansion in single quotes: \n \t'
escaped_quote="Escaped \"quotes\" and \$dollar sign"

# Check environment using builtins and keywords
if [ "$APP_ENV" = "production" ] && [ -d "$CONFIG_DIR" ]; then
    log_info "Production mode verified"
elif [ "$APP_ENV" = "development" ]; then
    echo "Development mode active"
else
    echo "Unknown environment: ${APP_ENV}" >&2
    exit 1
fi

# Heredoc content
cat <<EOF
App Configuration Summary:
Env:  $APP_ENV
Port: $DEFAULT_PORT
EOF

cat <<-INDENTED
	Tab-indented heredoc body
	Supports variables: $APP_ENV
INDENTED

# Command substitution and arithmetic
count=$(( 10 + 20 * 2 ))
tool_path=$(command -v bash || echo "/bin/sh")

# Escaped newline continuation
printf "%s\n" \
    "Continued argument across lines"

# Loop and case construct
for item in alpha beta gamma; do
    case "$item" in
        alpha) echo "First item: $item" ;;
        beta|gamma) echo "Next item: $item" ;;
        *) echo "Default branch" ;;
    esac
done
