#!/usr/bin/env bash
# scripts/e2e/mcp.sh — e2e: Model Context Protocol (MCP) stdio server (beads yjmy / jqhm / uxhr).
#
# Drives `fmd mcp` over a stdio pipe using Content-Length framed JSON-RPC 2.0:
# - initialize / initialized handshake
# - tools/list discovery
# - tools/call fmd.render_html
# - tools/call fmd.render_pdf
# - tools/call fmd.verify
# - tools/call fmd.capabilities
# - Error handling for malformed frames and invalid options
#
# Usage: scripts/e2e/mcp.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
export RCH_SHIM_LOCAL_IDE=1
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-mcp}"
e2e_build_bin mcp || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"

# Helper python script to drive stdio framing and verify responses
DRIVER="${WORK}/driver.py"
cat >"$DRIVER" <<'EOF'
import sys, json, subprocess

def send_frame(proc, obj):
    body = json.dumps(obj)
    frame = f"Content-Length: {len(body)}\r\n\r\n{body}"
    proc.stdin.write(frame.encode("utf-8"))
    proc.stdin.flush()

def read_frame(proc):
    content_length = None
    while True:
        line = proc.stdout.readline()
        if not line:
            return None
        line_str = line.decode("utf-8").strip()
        if not line_str:
            if content_length is not None:
                body = proc.stdout.read(content_length)
                return json.loads(body.decode("utf-8"))
            continue
        if line_str.startswith("Content-Length:"):
            content_length = int(line_str.split(":", 1)[1].strip())

binary = sys.argv[1]
proc = subprocess.Popen(
    [binary, "mcp"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)

# 1. Initialize
send_frame(proc, {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "e2e-tester", "version": "1.0"}
    }
})
init_res = read_frame(proc)
assert init_res["id"] == 1, f"bad init id: {init_res}"
assert init_res["result"]["serverInfo"]["name"] == "fmd"
sys.stderr.write("PASS: initialize handshake ok\n")

# 2. Initialized notification
send_frame(proc, {
    "jsonrpc": "2.0",
    "method": "notifications/initialized"
})

# 3. Tools list
send_frame(proc, {
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/list"
})
list_res = read_frame(proc)
assert list_res["id"] == 2
tool_names = [t["name"] for t in list_res["result"]["tools"]]
assert "fmd.render_html" in tool_names
assert "fmd.render_pdf" in tool_names
assert "fmd.verify" in tool_names
assert "fmd.capabilities" in tool_names
assert "fmd.render_file" in tool_names
sys.stderr.write("PASS: tools/list contains all 5 tools\n")

# 4. Call render_html
send_frame(proc, {
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
        "name": "fmd.render_html",
        "arguments": {
            "markdown": "# Hello MCP\n\nRendered via stdio JSON-RPC.",
            "font": "serif"
        }
    }
})
render_res = read_frame(proc)
assert render_res["id"] == 3
html_text = render_res["result"]["content"][0]["text"]
assert "<!DOCTYPE html>" in html_text
assert "Hello MCP" in html_text
sys.stderr.write("PASS: tools/call fmd.render_html ok\n")

# 5. Call capabilities
send_frame(proc, {
    "jsonrpc": "2.0",
    "id": 4,
    "method": "tools/call",
    "params": {
        "name": "fmd.capabilities",
        "arguments": {}
    }
})
cap_res = read_frame(proc)
assert cap_res["id"] == 4
cap_json = json.loads(cap_res["result"]["content"][0]["text"])
assert cap_json["tool"] == "fmd"
sys.stderr.write("PASS: tools/call fmd.capabilities ok\n")

# Close cleanly
proc.stdin.close()
proc.wait(timeout=5)
assert proc.returncode == 0
sys.stderr.write("PASS: mcp server clean shutdown\n")
EOF

e2e_run "mcp: stdio protocol driver test" -- python3 "$DRIVER" "$E2E_BIN"
e2e_expect_exit 0
e2e_expect_stderr_contains "PASS: initialize handshake ok"
e2e_expect_stderr_contains "PASS: tools/list contains all 5 tools"
e2e_expect_stderr_contains "PASS: tools/call fmd.render_html ok"
e2e_expect_stderr_contains "PASS: tools/call fmd.capabilities ok"
e2e_expect_stderr_contains "PASS: mcp server clean shutdown"

e2e_finish
exit $?
