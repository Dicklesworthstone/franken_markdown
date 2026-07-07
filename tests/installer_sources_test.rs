//! Source-shape checks for release installers.
//!
//! These tests pin security-relevant checksum behavior without requiring live
//! GitHub downloads or PowerShell availability.

use std::fs;
use std::io;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn marker_index(source: &str, marker: &str, context: &str) -> TestResult<usize> {
    source.find(marker).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{context}: missing marker {marker:?}"),
        )
        .into()
    })
}

#[test]
fn installers_use_exact_checksum_entries_and_sidecar_fallbacks() -> TestResult {
    let unix = fs::read_to_string("install.sh")?;
    let powershell = fs::read_to_string("install.ps1")?;

    assert!(
        unix.contains("checksum_from_sums_file"),
        "Unix installer should parse SHA256SUMS through exact archive-name matching"
    );
    assert!(
        !unix.contains("grep \"  ${TAR_BASENAME}\\$\""),
        "Unix installer must not use regex grep against archive names with dots"
    );
    assert!(
        unix.contains("checksum_from_sidecar_file"),
        "Unix installer should parse sidecar checksums through a bounded SHA-256 token"
    );
    assert!(
        unix.contains("checksum_is_skip"),
        "Unix installer should centralize case-insensitive CHECKSUM=SKIP handling"
    );
    assert!(
        unix.contains("Checksum verification explicitly skipped by CHECKSUM=SKIP"),
        "Unix installer should report explicit checksum skips clearly"
    );
    assert!(
        unix.contains("default_checksum_url"),
        "Unix installer should centralize the default SHA256SUMS URL"
    );
    assert!(
        unix.contains("releases/latest/download/SHA256SUMS"),
        "Unix installer should use the latest-release SHA256SUMS URL when no tag is pinned"
    );
    assert!(
        unix.contains("${DOWNLOADED_URL}.sha256"),
        "Unix installer should fall back to per-archive .sha256 files"
    );

    assert!(
        powershell.contains(r"$checksumPattern = '^\s*([0-9a-fA-F]{64})\s+\*?(.+?)\s*$'"),
        "PowerShell installer should parse SHA256SUMS as checksum/file entries"
    );
    assert!(
        powershell.contains("[IO.Path]::GetFileName($Matches[2].Trim())"),
        "PowerShell installer should compare exact archive basenames from SHA256SUMS"
    );
    assert!(
        powershell.contains("function Get-DefaultChecksumUrl"),
        "PowerShell installer should centralize the default SHA256SUMS URL"
    );
    assert!(
        powershell.contains("releases/latest/download/SHA256SUMS"),
        "PowerShell installer should use the latest-release SHA256SUMS URL when no tag is pinned"
    );
    assert!(
        !powershell.contains(
            "Select-String -LiteralPath $sumsFile -Pattern ([Regex]::Escape($archiveName))"
        ),
        "PowerShell installer must not accept substring checksum matches"
    );
    assert!(
        powershell.contains("$($script:DownloadedUrl).sha256"),
        "PowerShell installer should fall back to per-archive .sha256 files"
    );
    assert!(
        powershell.contains("sidecar.sha256"),
        "PowerShell installer should download and parse the sidecar checksum file"
    );
    assert!(
        powershell.contains("($checksum -ine 'SKIP')"),
        "PowerShell installer should not treat CHECKSUM=SKIP as a literal SHA-256 hash"
    );
    assert!(
        powershell.contains("Checksum verification explicitly skipped by CHECKSUM=SKIP"),
        "PowerShell installer should report explicit checksum skips clearly"
    );

    Ok(())
}

#[test]
fn installers_validate_archive_members_before_extracting() -> TestResult {
    let unix = fs::read_to_string("install.sh")?;
    let powershell = fs::read_to_string("install.ps1")?;

    assert!(
        unix.contains("archive_member_path_is_safe"),
        "Unix installer should centralize archive member path safety checks"
    );
    assert!(
        unix.contains("validate_archive_members"),
        "Unix installer should validate archive members before extraction"
    );
    assert!(
        unix.contains("Archive contains link entry; refusing to extract"),
        "Unix installer should reject tar hardlink/symlink entries"
    );
    assert!(
        unix.contains("Archive contains zip symlink entry; refusing to extract"),
        "Unix installer should reject zip symlink entries"
    );
    assert!(
        unix.contains(r#"zip -qy "$TMP/selftest-link.zip" link-out"#),
        "Unix installer self-test should store the symlink entry itself, not recursively follow the symlink target"
    );
    assert!(
        !unix.contains(r#"zip -qry "$TMP/selftest-link.zip" link-out"#),
        "Unix installer self-test must not use recursive zip without symlink preservation"
    );
    assert!(
        unix.contains("Archive contains unsafe member path"),
        "Unix installer should reject absolute or traversing archive paths"
    );
    let unix_validate = marker_index(
        &unix,
        "validate_archive_members \"$DOWNLOADED_TAR\" \"$TAR_BASENAME\"",
        "Unix installer should call archive validation at the extraction boundary",
    )?;
    let unix_tar_extract = marker_index(
        &unix,
        "tar -xzf \"$DOWNLOADED_TAR\"",
        "Unix installer tar extraction marker should exist",
    )?;
    let unix_zip_extract = marker_index(
        &unix,
        "unzip -qo \"$DOWNLOADED_TAR\"",
        "Unix installer zip extraction marker should exist",
    )?;
    assert!(
        unix_validate < unix_tar_extract && unix_validate < unix_zip_extract,
        "Unix archive validation must run before tar/unzip extraction"
    );
    assert!(
        unix.contains("[ -L \"$BIN\" ]"),
        "Unix installer should reject symlinked archive binaries before install"
    );

    assert!(
        powershell.contains("function Test-ArchiveMemberPathSafe"),
        "PowerShell installer should centralize archive member path safety checks"
    );
    assert!(
        powershell.contains("function Assert-ArchiveSafeToExtract"),
        "PowerShell installer should validate archive members before extraction"
    );
    assert!(
        powershell.contains("Archive contains hardlink or symlink entries; refusing to extract"),
        "PowerShell installer should reject tar hardlink/symlink entries"
    );
    assert!(
        powershell.contains("Archive contains zip symlink entry; refusing to extract"),
        "PowerShell installer should reject zip symlink entries"
    );
    assert!(
        powershell.contains("Archive contains unsafe member path"),
        "PowerShell installer should reject absolute or traversing archive paths"
    );
    let powershell_validate = marker_index(
        &powershell,
        "Assert-ArchiveSafeToExtract -Archive $archive -ArchiveName $archiveName",
        "PowerShell installer should call archive validation at the extraction boundary",
    )?;
    let powershell_zip_extract = marker_index(
        &powershell,
        "Expand-Archive -LiteralPath $archive",
        "PowerShell installer zip extraction marker should exist",
    )?;
    let powershell_tar_extract = marker_index(
        &powershell,
        "tar -xf $archive -C $extractDir",
        "PowerShell installer tar extraction marker should exist",
    )?;
    assert!(
        powershell_validate < powershell_zip_extract
            && powershell_validate < powershell_tar_extract,
        "PowerShell archive validation must run before Expand-Archive/tar extraction"
    );
    assert!(
        powershell.contains("[IO.FileAttributes]::ReparsePoint"),
        "PowerShell installer should reject reparse-point archive binaries before install"
    );
    assert!(
        powershell.contains("function Test-PathInsideDirectory"),
        "PowerShell installer should centralize resolved path containment checks"
    );
    assert!(
        powershell.contains(
            r#"$resolvedPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)"#
        ),
        "PowerShell installer should check resolved archive binaries against explicit directory-boundary prefixes"
    );
    assert!(
        !powershell
            .contains(r#"StartsWith("$resolvedExtract\", [StringComparison]::OrdinalIgnoreCase)"#),
        "PowerShell installer should not hard-code one separator in the extraction-boundary check"
    );
    assert!(
        powershell.contains("resolves outside the extraction directory"),
        "PowerShell installer should verify the selected binary remains under the extraction directory"
    );

    Ok(())
}
