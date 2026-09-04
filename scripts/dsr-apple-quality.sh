#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root/ios"

build_root="${FRANKEN_APPLE_BUILD_ROOT:-${DSR_QUALITY_RUN_DIR:-$repo_root/ios/build/dsr-apple-quality}}"
mkdir -p "$build_root"
sbh check --need 20G "$build_root"
command -v xcodegen >/dev/null
command -v jq >/dev/null
xcodegen generate --spec project.yml
git diff --exit-code -- FrankenMarkdown.xcodeproj Sources/Info.plist
git ls-files -z -- '*.swift' | xargs -0 xcrun swiftc -parse
plutil -lint Sources/Info.plist
plutil -lint Sources/PrivacyInfo.xcprivacy
plutil -lint FrankenMarkdown.entitlements
/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
xcodebuild -project FrankenMarkdown.xcodeproj -scheme FrankenMarkdown \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath "$build_root/derived-data" \
  CODE_SIGNING_ALLOWED=NO build
xcodebuild -project FrankenMarkdown.xcodeproj -scheme FrankenMarkdown \
  -destination 'platform=macOS,variant=Mac Catalyst' \
  -derivedDataPath "$build_root/derived-data" \
  CODE_SIGNING_ALLOWED=NO test -only-testing:FrankenMarkdownTests

/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
simulator_json="$(xcrun simctl list devices available --json)"
iphone_id="${FRANKENMARKDOWN_IPHONE_SIMULATOR_ID:-$(
  jq -r '
    [.devices[][] | select(.name | contains("iPhone"))] as $devices
    | (($devices | map(select(.name | test("^FrankenMarkdown iPhone"; "i"))))
        + ($devices | map(select((.name | test("FrankenMarkdown"; "i")) and .state == "Booted")))
        + ($devices | map(select(.name | test("FrankenMarkdown"; "i"))))
        + ($devices | map(select(.state == "Booted")))
        + $devices)
    | .[0].udid // empty
  ' <<< "$simulator_json"
)}"
if [[ -z "$iphone_id" ]]; then
  echo "FrankenMarkdown DSR requires one available iPhone Simulator" >&2
  exit 1
fi

/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
xcodebuild -project FrankenMarkdown.xcodeproj -scheme FrankenMarkdown \
  -destination "platform=iOS Simulator,id=$iphone_id" \
  -derivedDataPath "$build_root/derived-data" \
  -resultBundlePath "$build_root/frankenmarkdown-iphone-ui.xcresult" \
  -parallel-testing-enabled NO \
  CODE_SIGNING_ALLOWED=NO test \
  -only-testing:FrankenMarkdownUITests
