<#
.SYNOPSIS
    Installer for fmd - the franken_markdown CLI (Markdown -> HTML & PDF).

.DESCRIPTION
    Downloads a prebuilt fmd.exe for Windows and installs it, or builds it from
    source with cargo when no prebuilt binary is available.

    One-liner install:
        irm https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.ps1 | iex

    With explicit options (download the script, then run it):
        irm https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.ps1 -OutFile install.ps1
        .\install.ps1 -FromSource -Verify

    Tagged releases publish prebuilt fmd archives. The installer prefers those
    archives and only builds from source when -FromSource is requested or no
    matching release asset exists for the current platform.

    Proxy support: set HTTPS_PROXY / HTTP_PROXY and downloads honor it.

.PARAMETER Version
    Install a specific version, e.g. v0.1.0 (default: latest release).

.PARAMETER Dest
    Install directory (default: %USERPROFILE%\.local\bin).

.PARAMETER System
    Install machine-wide to %ProgramFiles%\fmd (run from an elevated shell).

.PARAMETER EasyMode
    Add the install directory to your PATH (User scope; Machine scope with -System).

.PARAMETER Verify
    Run a post-install self-test (fmd --version + a tiny render smoke test).

.PARAMETER FromSource
    Build from source with cargo instead of downloading a prebuilt binary.

.PARAMETER Quiet
    Suppress non-error output.

.PARAMETER Force
    Reinstall even if the requested version is already present.

.PARAMETER Help
    Show usage and exit.

.EXAMPLE
    .\install.ps1
    Install the latest fmd into %USERPROFILE%\.local\bin.

.EXAMPLE
    .\install.ps1 -FromSource -Verify -EasyMode
    Build fmd from source, run the self-test, and add it to PATH.

.NOTES
    Repo: https://github.com/Dicklesworthstone/franken_markdown
#>

[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$Dest = "$env:USERPROFILE\.local\bin",
    [switch]$System,
    [switch]$EasyMode,
    [switch]$Verify,
    [switch]$FromSource,
    [switch]$Quiet,
    [switch]$Force,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# -----------------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------------
$Owner      = if ($env:OWNER) { $env:OWNER } else { 'Dicklesworthstone' }
$Repo       = if ($env:REPO)  { $env:REPO }  else { 'franken_markdown' }
$BinaryName = 'fmd'
$BinaryExe  = "$BinaryName.exe"

if ($System) { $Dest = "$env:ProgramFiles\$BinaryName" }

# Prefer modern TLS for older PowerShell hosts.
try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch {}

# -----------------------------------------------------------------------------
# Output stack (colored Write-Host with --quiet gate; errors are never silenced)
# -----------------------------------------------------------------------------
function Write-Info { param([string]$m) if (-not $Quiet) { Write-Host "-> $m" -ForegroundColor Cyan } }
function Write-Ok   { param([string]$m) if (-not $Quiet) { Write-Host "[OK] $m" -ForegroundColor Green } }
function Write-WarnMsg { param([string]$m) Write-Host "[!] $m" -ForegroundColor Yellow }
function Write-ErrMsg  { param([string]$m) Write-Host "[X] $m" -ForegroundColor Red }

function Write-Banner {
    param([string]$Title, [string]$Subtitle, [ConsoleColor]$Color = [ConsoleColor]::Cyan)
    if ($Quiet) { return }
    $lines = @($Title, $Subtitle)
    $width = ($lines | Measure-Object -Property Length -Maximum).Maximum
    $border = '=' * ($width + 4)
    Write-Host ("+$border+") -ForegroundColor $Color
    Write-Host ("|  ") -ForegroundColor $Color -NoNewline
    Write-Host ($Title.PadRight($width)) -ForegroundColor Green -NoNewline
    Write-Host ("  |") -ForegroundColor $Color
    Write-Host ("|  ") -ForegroundColor $Color -NoNewline
    Write-Host ($Subtitle.PadRight($width)) -ForegroundColor DarkGray -NoNewline
    Write-Host ("  |") -ForegroundColor $Color
    Write-Host ("+$border+") -ForegroundColor $Color
}

function Show-Usage {
@"
fmd installer - franken_markdown CLI (Markdown -> HTML & PDF)

Usage:
  .\install.ps1 [options]

Options:
  -Version vX.Y.Z   Install a specific version (default: latest release)
  -Dest DIR         Install directory (default: %USERPROFILE%\.local\bin)
  -System           Install to %ProgramFiles%\fmd (elevated shell)
  -EasyMode         Add the install dir to PATH (User; Machine with -System)
  -Verify           Post-install self-test (fmd --version + render smoke test)
  -FromSource       Build from source with cargo instead of downloading
  -Quiet            Suppress non-error output
  -Force            Reinstall even if the requested version is present
  -Help             Show this help and exit

Environment:
  HTTPS_PROXY / HTTP_PROXY   Routed through every download

Examples:
  .\install.ps1
  .\install.ps1 -System
  .\install.ps1 -FromSource -Verify -EasyMode
  .\install.ps1 -Version v0.1.0
"@ | Write-Host
}

if ($Help) { Show-Usage; exit 0 }

# -----------------------------------------------------------------------------
# Proxy support - applied to every Invoke-WebRequest via splatting
# -----------------------------------------------------------------------------
$ProxyArgs = @{}
$proxyUrl = if ($env:HTTPS_PROXY) { $env:HTTPS_PROXY } elseif ($env:HTTP_PROXY) { $env:HTTP_PROXY } else { $null }
if ($proxyUrl) { $ProxyArgs['Proxy'] = $proxyUrl; $ProxyArgs['ProxyUseDefaultCredentials'] = $true }

function Invoke-Download {
    param([string]$Url, [string]$OutFile)
    $reqArgs = @{ Uri = $Url; OutFile = $OutFile; UseBasicParsing = $true } + $ProxyArgs
    Invoke-WebRequest @reqArgs
}

function Test-ArchiveMemberPathSafe {
    param([string]$MemberPath)
    if ([string]::IsNullOrEmpty($MemberPath)) { return $false }
    if (($MemberPath -eq '.') -or ($MemberPath -eq './')) { return $false }
    if ($MemberPath -match '^[A-Za-z]:') { return $false }
    if ([IO.Path]::IsPathRooted($MemberPath)) { return $false }
    if ($MemberPath.Contains('\')) { return $false }
    foreach ($part in ($MemberPath -split '/')) {
        if ($part -eq '..') { return $false }
    }
    return $true
}

function Normalize-DirectoryBoundary {
    param([string]$Path)
    $root = [IO.Path]::GetPathRoot($Path)
    while (($Path.Length -gt $root.Length) -and ($Path.EndsWith('\') -or $Path.EndsWith('/'))) {
        $Path = $Path.Substring(0, $Path.Length - 1)
    }
    return $Path
}

function Test-PathInsideDirectory {
    param([string]$Path, [string]$Directory)
    $resolvedDirectory = Normalize-DirectoryBoundary ((Resolve-Path -LiteralPath $Directory).ProviderPath)
    $resolvedPath = (Resolve-Path -LiteralPath $Path).ProviderPath
    $prefixes = if ($resolvedDirectory.EndsWith('\') -or $resolvedDirectory.EndsWith('/')) {
        @($resolvedDirectory)
    } else {
        @("$resolvedDirectory\", "$resolvedDirectory/")
    }
    foreach ($prefix in $prefixes) {
        if ($resolvedPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return $false
}

function Get-ZipArchiveEntries {
    param([string]$Archive, [string]$ArchiveName)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        foreach ($entry in $zip.Entries) {
            $unixModeType = (($entry.ExternalAttributes -shr 16) -band 0xF000)
            if ($unixModeType -eq 0xA000) {
                Write-ErrMsg "Archive contains zip symlink entry; refusing to extract $ArchiveName"
                exit 1
            }
            $entry.FullName
        }
    } finally {
        $zip.Dispose()
    }
}

function Get-TarArchiveEntries {
    param([string]$Archive)
    $entries = & tar -tf $Archive
    if ($LASTEXITCODE -ne 0) { throw "Failed to inspect archive entries: $Archive" }
    return $entries
}

function Test-TarArchiveHasLinkEntries {
    param([string]$Archive)
    $listing = & tar -tvf $Archive
    if ($LASTEXITCODE -ne 0) { throw "Failed to inspect tar entry types: $Archive" }
    foreach ($line in $listing) {
        if ($line.Length -gt 0) {
            $kind = $line.Substring(0, 1)
            if (($kind -eq 'l') -or ($kind -eq 'h')) { return $true }
        }
    }
    return $false
}

function Assert-ArchiveSafeToExtract {
    param([string]$Archive, [string]$ArchiveName)
    Write-Info "Validating archive member paths"
    if ($ArchiveName -match '\.zip$') {
        $entries = Get-ZipArchiveEntries -Archive $Archive -ArchiveName $ArchiveName
    } elseif ($ArchiveName -match '\.(tar\.gz|tgz|tar\.xz)$') {
        if (Test-TarArchiveHasLinkEntries -Archive $Archive) {
            Write-ErrMsg "Archive contains hardlink or symlink entries; refusing to extract $ArchiveName"
            exit 1
        }
        $entries = Get-TarArchiveEntries -Archive $Archive
    } else {
        Write-ErrMsg "Unknown archive format: $ArchiveName"
        exit 1
    }

    foreach ($entry in $entries) {
        if (-not (Test-ArchiveMemberPathSafe -MemberPath $entry)) {
            Write-ErrMsg "Archive contains unsafe member path: $entry"
            exit 1
        }
    }
}

# -----------------------------------------------------------------------------
# Banner
# -----------------------------------------------------------------------------
Write-Host ""
Write-Banner -Title "fmd installer" -Subtitle "franken_markdown - Markdown to beautiful HTML & tiny PDF"
Write-Host ""

# -----------------------------------------------------------------------------
# Platform detection (arch -> Rust MSVC triple)
# -----------------------------------------------------------------------------
$archRaw = $env:PROCESSOR_ARCHITECTURE
if (-not $archRaw) { $archRaw = (Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1).Architecture }
switch -Wildcard ("$archRaw") {
    'AMD64'  { $Arch = 'x86_64';  $Target = 'x86_64-pc-windows-msvc' }
    'x86_64' { $Arch = 'x86_64';  $Target = 'x86_64-pc-windows-msvc' }
    'ARM64'  { $Arch = 'aarch64'; $Target = 'aarch64-pc-windows-msvc' }
    'x86'    { $Arch = 'x86';     $Target = '' }
    default  { $Arch = "$archRaw"; $Target = '' }
}
if (-not $Target) {
    Write-WarnMsg "No prebuilt target for architecture '$archRaw'; will build from source"
    $FromSource = $true
}
$Ext = 'zip'
$VersionBare = $Version -replace '^v', ''

# -----------------------------------------------------------------------------
# Version resolution: -Version -> GitHub API latest -> redirect. Never throws;
# with no releases yet an empty version simply routes to from-source.
# -----------------------------------------------------------------------------
function Resolve-Version {
    if ($script:Version) { Write-Info "Using requested version: $($script:Version)"; return }
    if ($script:FromSource) { return }
    Write-Info "Resolving latest version..."
    try {
        $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Owner/$Repo/releases/latest" `
                 -Headers @{ 'Accept' = 'application/vnd.github.v3+json'; 'User-Agent' = 'fmd-installer' } `
                 @ProxyArgs -ErrorAction Stop
        if ($rel.tag_name) {
            $script:Version = $rel.tag_name
            $script:VersionBare = $script:Version -replace '^v', ''
            Write-Info "Latest version: $($script:Version)"
            return
        }
    } catch {
        Write-WarnMsg "No published release found; will build from source"
        $script:FromSource = $true
    }
    if (-not $script:Version) { $script:FromSource = $true }
}
Resolve-Version

# -----------------------------------------------------------------------------
# Preflight
# -----------------------------------------------------------------------------
function Test-Preflight {
    Write-Info "Running preflight checks"
    # Create / verify destination is writable.
    if (-not (Test-Path -LiteralPath $Dest)) {
        try { New-Item -ItemType Directory -Path $Dest -Force | Out-Null }
        catch { Write-ErrMsg "Cannot create $Dest. Run an elevated shell or pick a writable -Dest."; exit 1 }
    }
    try {
        $probe = Join-Path $Dest ".fmd-write-test"
        Set-Content -LiteralPath $probe -Value 'ok' -ErrorAction Stop
        Remove-Item -LiteralPath $probe -Force -ErrorAction SilentlyContinue
    } catch {
        Write-ErrMsg "No write permission to $Dest. Run an elevated shell or pick a writable -Dest."
        exit 1
    }
    # Existing install?
    $existing = Join-Path $Dest $BinaryExe
    if (Test-Path -LiteralPath $existing) {
        try { $cur = & $existing --version 2>$null; if ($cur) { Write-Info "Existing install detected: $cur" } } catch {}
    }
}
Test-Preflight

# -----------------------------------------------------------------------------
# Already-installed short-circuit (-Force overrides)
# -----------------------------------------------------------------------------
$existingBin = Join-Path $Dest $BinaryExe
if (-not $Force -and $Version -and (Test-Path -LiteralPath $existingBin)) {
    try {
        $curVer = (& $existingBin --version 2>$null) -replace '.*\s', ''
        if ($curVer -and (($curVer -eq $VersionBare) -or ("v$curVer" -eq $Version))) {
            Write-Ok "$BinaryName $Version is already installed at $existingBin"
            Write-Info "Use -Force to reinstall"
            exit 0
        }
    } catch {}
}

# -----------------------------------------------------------------------------
# Temp workspace + cleanup
# -----------------------------------------------------------------------------
$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("fmd-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null
$script:BuiltFromSource = $false

function Invoke-Cleanup { if (Test-Path -LiteralPath $Tmp) { Remove-Item -LiteralPath $Tmp -Recurse -Force -ErrorAction SilentlyContinue } }

function Install-BinaryFile {
    param([string]$SrcBin)
    if (-not (Test-Path -LiteralPath $SrcBin)) { Write-ErrMsg "Binary not found: $SrcBin"; exit 1 }
    Copy-Item -LiteralPath $SrcBin -Destination (Join-Path $Dest $BinaryExe) -Force
}

# -----------------------------------------------------------------------------
# Build-from-source fallback: cargo build --release --bin fmd
# -----------------------------------------------------------------------------
function Build-FromSource {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-ErrMsg "cargo not found - building $BinaryName from source needs the Rust toolchain."
        Write-ErrMsg "Install Rust via rustup, then re-run this installer:"
        Write-ErrMsg "  winget install --id Rustlang.Rustup -e"
        Write-ErrMsg "  # or download rustup-init.exe from https://rustup.rs and run it"
        Write-ErrMsg "  # then restart your shell so cargo is on PATH"
        exit 1
    }

    $srcIsClone = $false
    if ((Test-Path -LiteralPath ".\Cargo.toml") -and
        (Select-String -LiteralPath ".\Cargo.toml" -Pattern '^name = "franken_markdown"' -Quiet)) {
        $src = (Get-Location).Path
        Write-Info "Building from local checkout: $src"
    } else {
        if (-not (Get-Command git -ErrorAction SilentlyContinue)) { Write-ErrMsg "git not found - required to fetch the source"; exit 1 }
        $src = Join-Path $Tmp 'src'
        $srcIsClone = $true
        Write-Info "Cloning $Owner/$Repo..."
        $cloneUrl = "https://github.com/$Owner/$Repo.git"
        if ($Version) {
            git clone --depth 1 --branch $Version $cloneUrl $src 2>$null
            if ($LASTEXITCODE -ne 0) {
                Write-WarnMsg "Could not clone tag '$Version'; cloning the default branch instead"
                if (Test-Path -LiteralPath $src) { Remove-Item -LiteralPath $src -Recurse -Force }
                git clone --depth 1 $cloneUrl $src
            }
        } else {
            git clone --depth 1 $cloneUrl $src
        }
    }

    # The default `fmd` build never enables `batch`; if a source fallback lands on
    # an older tag with a non-portable optional Asupersync source, neutralize that
    # optional dep in our throwaway clone only. Never mutate the user's checkout.
    $sibling = Join-Path (Split-Path -Parent $src) 'asupersync'
    $cargoToml = Join-Path $src 'Cargo.toml'
    if ($srcIsClone -and -not (Test-Path -LiteralPath $sibling) -and (Test-Path -LiteralPath $cargoToml)) {
        (Get-Content -LiteralPath $cargoToml) |
            Where-Object { $_ -notmatch '^asupersync\s*=\s*{' } |
            ForEach-Object { $_ -replace '^batch\s*=.*$', 'batch = ["cli"]' } |
            Set-Content -LiteralPath $cargoToml
    }

    Write-Info "Building $BinaryName (cargo build --release --bin $BinaryName) - this may take a few minutes"
    Push-Location $src
    try {
        # Unset target redirections so the binary lands at the expected path.
        $savedTargetDir = $env:CARGO_TARGET_DIR
        $savedBuildTarget = $env:CARGO_BUILD_TARGET
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        Remove-Item Env:CARGO_BUILD_TARGET -ErrorAction SilentlyContinue
        cargo build --release --bin $BinaryName
        if ($LASTEXITCODE -ne 0) { Write-ErrMsg "cargo build failed"; exit 1 }
    } finally {
        if ($savedTargetDir) { $env:CARGO_TARGET_DIR = $savedTargetDir }
        if ($savedBuildTarget) { $env:CARGO_BUILD_TARGET = $savedBuildTarget }
        Pop-Location
    }

    $built = Join-Path $src "target\release\$BinaryExe"
    if (-not (Test-Path -LiteralPath $built)) {
        $found = Get-ChildItem -Path (Join-Path $src 'target') -Recurse -Filter $BinaryExe -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($found) { $built = $found.FullName }
    }
    if (-not (Test-Path -LiteralPath $built)) { Write-ErrMsg "Build finished but $BinaryExe was not found under target\"; exit 1 }

    Install-BinaryFile -SrcBin $built
    $script:BuiltFromSource = $true
    Write-Ok "Installed to $(Join-Path $Dest $BinaryExe) (built from source)"
}

# -----------------------------------------------------------------------------
# Binary acquisition: 4-tier fallback ending in build-from-source
# -----------------------------------------------------------------------------
$script:DownloadedArchive = ""
$script:DownloadedUrl = ""

function Get-Binary {
    if ($FromSource -or -not $Target) { Build-FromSource; return }

    $candidates = @()
    if ($env:ARTIFACT_URL) { $candidates += $env:ARTIFACT_URL }
    if ($Version) {
        $candidates += "https://github.com/$Owner/$Repo/releases/download/$Version/$BinaryName-$Version-$Target.$Ext"
        $candidates += "https://github.com/$Owner/$Repo/releases/download/$Version/$BinaryName-$VersionBare-$Target.$Ext"
        $candidates += "https://github.com/$Owner/$Repo/releases/download/$Version/$BinaryName-$Target.$Ext"
    }
    $candidates += "https://github.com/$Owner/$Repo/releases/latest/download/$BinaryName-$Target.$Ext"
    $candidates += "https://github.com/$Owner/$Repo/releases/latest/download/$BinaryName-windows-$Arch.$Ext"

    foreach ($url in $candidates) {
        $archive = Join-Path $Tmp ([IO.Path]::GetFileName($url))
        try {
            Write-Info "Downloading $BinaryName $(if ($Version) { $Version } else { 'latest' })  ($([IO.Path]::GetFileName($url)))"
            Invoke-Download -Url $url -OutFile $archive
            $script:DownloadedArchive = $archive
            $script:DownloadedUrl = $url
            Write-Ok "Downloaded $([IO.Path]::GetFileName($url))"
            return
        } catch {
            Write-WarnMsg "Not available: $([IO.Path]::GetFileName($url))"
        }
    }

    Write-WarnMsg "No prebuilt binary available; falling back to build-from-source"
    Build-FromSource
}

function Get-DefaultChecksumUrl {
    if ($Version) {
        return "https://github.com/$Owner/$Repo/releases/download/$Version/SHA256SUMS"
    }
    return "https://github.com/$Owner/$Repo/releases/latest/download/SHA256SUMS"
}

try {
    Get-Binary

    if (-not $script:BuiltFromSource) {
        $archive = $script:DownloadedArchive
        $archiveName = [IO.Path]::GetFileName($archive)

        # -- Checksum verification (SHA256) --
        $checksum = $env:CHECKSUM
        $checksumPattern = '^\s*([0-9a-fA-F]{64})\s+\*?(.+?)\s*$'
        if (-not $checksum) {
            $sumsUrl = if ($env:CHECKSUM_URL) { $env:CHECKSUM_URL } else { Get-DefaultChecksumUrl }
            $sumsFile = Join-Path $Tmp 'SHA256SUMS'
            try {
                Write-Info "Fetching checksums from $sumsUrl"
                Invoke-Download -Url $sumsUrl -OutFile $sumsFile
                foreach ($rawLine in Get-Content -LiteralPath $sumsFile) {
                    if ($rawLine -match $checksumPattern) {
                        $listedName = [IO.Path]::GetFileName($Matches[2].Trim())
                        if ($listedName -eq $archiveName) {
                            $checksum = $Matches[1]
                            break
                        }
                    }
                }
            } catch { }
        }
        if (-not $checksum) {
            $sidecarUrl = "$($script:DownloadedUrl).sha256"
            $sidecarFile = Join-Path $Tmp 'sidecar.sha256'
            try {
                Write-Info "Fetching sidecar checksum from $sidecarUrl"
                Invoke-Download -Url $sidecarUrl -OutFile $sidecarFile
                foreach ($rawLine in Get-Content -LiteralPath $sidecarFile) {
                    if ($rawLine -match '^\s*([0-9a-fA-F]{64})\b') {
                        $checksum = $Matches[1]
                        break
                    }
                }
            } catch { }
        }
        if ($checksum -and ($checksum -ine 'SKIP')) {
            $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLower()
            if ($actual -eq $checksum.ToLower()) {
                Write-Ok "Checksum verified (SHA256)"
            } else {
                Write-ErrMsg "Checksum mismatch for $archiveName"
                Write-ErrMsg "  expected: $checksum"
                Write-ErrMsg "  actual:   $actual"
                exit 1
            }
        } elseif ($checksum -ieq 'SKIP') {
            Write-WarnMsg "Checksum verification explicitly skipped by CHECKSUM=SKIP"
        } else {
            Write-WarnMsg "Checksum for $archiveName not found; skipping checksum verification"
        }

        # -- Sigstore / cosign (best-effort) --
        if (Get-Command cosign -ErrorAction SilentlyContinue) {
            $bundleUrl = if ($env:SIGSTORE_BUNDLE_URL) { $env:SIGSTORE_BUNDLE_URL } else { "$($script:DownloadedUrl).sigstore.json" }
            $bundleFile = Join-Path $Tmp 'bundle.sigstore.json'
            try {
                Invoke-Download -Url $bundleUrl -OutFile $bundleFile
                $idRe = if ($env:COSIGN_IDENTITY_RE) { $env:COSIGN_IDENTITY_RE } else { "https://github.com/$Owner/$Repo/.*" }
                $issuer = if ($env:COSIGN_OIDC_ISSUER) { $env:COSIGN_OIDC_ISSUER } else { "https://token.actions.githubusercontent.com" }
                cosign verify-blob --bundle $bundleFile --certificate-identity-regexp $idRe --certificate-oidc-issuer $issuer $archive 2>$null
                if ($LASTEXITCODE -eq 0) { Write-Ok "Signature verified (cosign)" }
                else { Write-ErrMsg "cosign signature verification FAILED for $archiveName"; exit 1 }
            } catch {
                Write-WarnMsg "Sigstore bundle not published; skipping signature verification"
            }
        } else {
            Write-WarnMsg "cosign not found; skipping signature verification"
        }

        # -- Extract + install --
        Assert-ArchiveSafeToExtract -Archive $archive -ArchiveName $archiveName
        Write-Info "Extracting"
        $extractDir = Join-Path $Tmp 'extract'
        New-Item -ItemType Directory -Path $extractDir -Force | Out-Null
        if ($archiveName -match '\.zip$') {
            Expand-Archive -LiteralPath $archive -DestinationPath $extractDir -Force
        } elseif ($archiveName -match '\.(tar\.gz|tgz|tar\.xz)$') {
            tar -xf $archive -C $extractDir
            if ($LASTEXITCODE -ne 0) { Write-ErrMsg "Failed to extract $archiveName (tar)"; exit 1 }
        } else {
            Write-ErrMsg "Unknown archive format: $archiveName"; exit 1
        }

        $found = Get-ChildItem -Path $extractDir -Recurse -Filter $BinaryExe -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $found) { Write-ErrMsg "Binary '$BinaryExe' not found inside the archive"; exit 1 }
        if (($found.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            Write-ErrMsg "Archive binary '$BinaryExe' is a reparse point; refusing to install it"
            exit 1
        }
        if (-not (Test-PathInsideDirectory -Path $found.FullName -Directory $extractDir)) {
            Write-ErrMsg "Archive binary '$BinaryExe' resolves outside the extraction directory"
            exit 1
        }
        Install-BinaryFile -SrcBin $found.FullName
        Write-Ok "Installed to $(Join-Path $Dest $BinaryExe)"
    }

    # ---------------------------------------------------------------------------
    # PATH setup (-EasyMode)
    # ---------------------------------------------------------------------------
    $pathNote = ""
    $scope = if ($System) { 'Machine' } else { 'User' }
    $curPath = [Environment]::GetEnvironmentVariable('Path', $scope)
    if (-not $curPath) { $curPath = "" }
    $onPath = @($curPath -split ';' | Where-Object { $_.TrimEnd('\') -ieq $Dest.TrimEnd('\') }).Count -gt 0
    if (-not $onPath) {
        if ($EasyMode) {
            try {
                $newPath = if ($curPath) { "$curPath;$Dest" } else { $Dest }
                [Environment]::SetEnvironmentVariable('Path', $newPath, $scope)
                $env:Path = "$env:Path;$Dest"
                $pathNote = "Added $Dest to your $scope PATH (open a new terminal to pick it up)"
                Write-WarnMsg $pathNote
            } catch {
                $pathNote = "Add $Dest to your PATH to use $BinaryName"
                Write-WarnMsg "$pathNote (could not edit $scope PATH; elevated shell needed for -System)"
            }
        } else {
            $pathNote = "Add $Dest to your PATH to use $BinaryName (or re-run with -EasyMode)"
            Write-WarnMsg $pathNote
        }
    }

    # Shell completions: fmd does not expose a `completions <shell>` subcommand,
    # so there is nothing to install. Intentional clean no-op.

    # ---------------------------------------------------------------------------
    # -Verify self-test
    # ---------------------------------------------------------------------------
    if ($Verify) {
        $binPath = Join-Path $Dest $BinaryExe
        try {
            $verOut = & $binPath --version 2>&1
            Write-Ok "Self-test: $verOut"
        } catch {
            Write-ErrMsg "Self-test failed: '$binPath --version' did not run"
            exit 1
        }
        try {
            $smoke = & $binPath --text '# fmd smoke test' --out - 2>$null
            if ($smoke -match 'smoke test') { Write-Ok "Render smoke test passed (Markdown -> HTML)" }
            else { Write-WarnMsg "Render smoke test did not produce expected HTML (binary still installed)" }
        } catch { Write-WarnMsg "Render smoke test did not run (binary still installed)" }
        try { & $binPath doctor *> $null; if ($LASTEXITCODE -eq 0) { Write-Ok "fmd doctor reports healthy" } } catch {}
    }

    # ---------------------------------------------------------------------------
    # Final summary
    # ---------------------------------------------------------------------------
    if (-not $Quiet) {
        $srcNote = if ($script:BuiltFromSource) { 'built from source' } else { 'prebuilt binary' }
        $displayVersion = $Version
        if (-not $displayVersion) {
            try { $displayVersion = (& (Join-Path $Dest $BinaryExe) --version 2>$null) -replace '.*\s', '' } catch {}
        }
        if (-not $displayVersion) { $displayVersion = '(source build)' }

        Write-Host ""
        Write-Banner -Title "fmd installed!" -Subtitle "Binary: $(Join-Path $Dest $BinaryExe)" -Color ([ConsoleColor]::Green)
        Write-Host ""
        Write-Host "  Version:  $displayVersion  ($srcNote)" -ForegroundColor Gray
        Write-Host ""
        Write-Host "  Quick start:" -ForegroundColor Cyan
        Write-Host "    fmd README.md --out README.html" -ForegroundColor DarkGray
        Write-Host "    fmd README.md --to pdf --out README.pdf" -ForegroundColor DarkGray
        Write-Host "    fmd --text '# Hello' --out -" -ForegroundColor DarkGray
        Write-Host "    fmd capabilities --json" -ForegroundColor DarkGray
        Write-Host ""
        Write-Host "  Uninstall:  Remove-Item '$(Join-Path $Dest $BinaryExe)'" -ForegroundColor DarkGray
        Write-Host ""
    }
} finally {
    Invoke-Cleanup
}

exit 0
