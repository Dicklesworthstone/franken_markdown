//! Source-shape checks for repo helper scripts.

use std::fs;
use std::io;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const RUN_ID_HELPER: &str = "scripts/validate-run-id.sh";
const RUN_ID_CALL: &str = "fmd_validate_run_id";

fn assert_run_id_validation_before(
    script_path: &str,
    artifact_marker: &str,
    before_marker: &str,
) -> TestResult {
    let script = fs::read_to_string(script_path)?;
    let validation = script.find(RUN_ID_CALL).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "script should call the shared run-id validator before using artifact paths",
        )
    })?;
    let guarded_use = script.find(before_marker).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "script is missing guarded use marker",
        )
    })?;

    assert!(
        script.contains(RUN_ID_HELPER),
        "artifact scripts should source the shared run-id policy helper"
    );
    assert!(
        script.contains(artifact_marker),
        "artifact paths should be rooted under the intended artifact directory"
    );
    assert!(
        validation < guarded_use,
        "run id validation must happen before artifact path use or cleanup"
    );

    Ok(())
}

struct TestArtifactDir {
    path: std::path::PathBuf,
}

impl Drop for TestArtifactDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn unique_run_id(label: &str) -> io::Result<String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    Ok(format!("fmd-{label}-{}-{nanos}", std::process::id()))
}

#[test]
fn shared_run_id_policy_helper_documents_the_project_grammar() -> TestResult {
    let helper = fs::read_to_string(RUN_ID_HELPER)?;
    assert!(
        helper.contains("FMD_RUN_ID_PATTERN='^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$'"),
        "helper should define the single project run-id grammar"
    );
    assert!(
        helper.contains("fmd_validate_run_id()"),
        "helper should expose the shared validator function"
    );
    assert!(
        helper.contains("must match ${FMD_RUN_ID_PATTERN}"),
        "helper error should report the single project grammar variable"
    );
    assert!(
        helper.contains("exit 64"),
        "invalid run ids should use the documented usage-error exit code"
    );
    Ok(())
}

#[test]
fn shared_run_id_policy_rejects_blank_ids_with_stable_usage_exit() -> TestResult {
    let output = Command::new("bash")
        .args([
            "-c",
            "source scripts/validate-run-id.sh; fmd_validate_run_id demo \"\"",
        ])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(64),
        "blank run ids should use the documented usage exit; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("demo: run-id must match"),
        "blank run ids should get the same actionable grammar error as other invalid ids: {stderr}",
    );
    Ok(())
}

#[test]
fn test_all_discovers_shell_helpers_instead_of_hard_coding_a_drift_prone_list() -> TestResult {
    let script = fs::read_to_string("scripts/test-all.sh")?;
    assert!(
        script.contains("find scripts -type f -name '*.sh' | LC_ALL=C sort"),
        "test-all should lint every checked-in helper script instead of relying on a stale manual list"
    );
    assert!(
        script.contains("printf '%s\\n' install.sh"),
        "test-all should include the root Unix installer alongside scripts/**/*.sh"
    );
    assert!(
        script.contains("shellcheck --severity=warning \"${scripts[@]}\""),
        "test-all should shellcheck the discovered helper set"
    );
    assert!(
        !script.contains("HELPER_SHELL_SCRIPTS=("),
        "a static helper list is easy to forget when new helper scripts are added"
    );
    Ok(())
}

#[test]
fn perf_counters_summary_json_escapes_counter_names() -> TestResult {
    let run_id = unique_run_id("perf-counters-json")?;
    let out_dir = std::path::PathBuf::from("tests/artifacts/perf").join(&run_id);
    let _cleanup = TestArtifactDir {
        path: out_dir.clone(),
    };
    let output = Command::new("bash")
        .args([
            "scripts/perf-counters.sh",
            "--run-id",
            &run_id,
            "--counters",
            r#"cycles,bad"counter,back\slash"#,
            "--",
            "true",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "perf-counters should be a best-effort profiling aid; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let summary = fs::read_to_string(out_dir.join("hardware_counter_summary.jsonl"))?;
    assert!(
        summary.contains(r#""counter_set":["cycles","bad\"counter","back\\slash"]"#),
        "counter names should be JSON-escaped, got {summary}"
    );

    let parse = Command::new("python3")
        .args([
            "-c",
            "import json,sys\nfor line in open(sys.argv[1], encoding='utf-8'):\n    json.loads(line)",
            out_dir
                .join("hardware_counter_summary.jsonl")
                .to_str()
                .ok_or_else(|| io::Error::other("summary path is not valid utf-8"))?,
        ])
        .output()?;
    assert!(
        parse.status.success(),
        "hardware_counter_summary.jsonl should parse as JSONL; stdout={} stderr={} summary={summary}",
        String::from_utf8_lossy(&parse.stdout),
        String::from_utf8_lossy(&parse.stderr),
    );
    Ok(())
}

#[test]
fn perf_gauntlet_refuses_existing_run_before_building() -> TestResult {
    let run_id = unique_run_id("perf-gauntlet-existing")?;
    let out_dir = std::path::PathBuf::from("tests/artifacts/perf").join(&run_id);
    fs::create_dir_all(&out_dir)?;
    let _cleanup = TestArtifactDir {
        path: out_dir.clone(),
    };

    let output = Command::new("bash")
        .args(["scripts/perf-gauntlet.sh", "--run-id", &run_id])
        .output()?;

    assert_eq!(
        output.status.code(),
        Some(64),
        "existing perf-gauntlet runs should fail with usage exit before build; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing to reuse existing run"),
        "existing run should get an actionable refusal: {stderr}",
    );
    assert!(
        !stdout.contains("building release-perf") && !stderr.contains("building release-perf"),
        "refusal must happen before the expensive build starts; stdout={stdout} stderr={stderr}",
    );

    Ok(())
}

#[test]
fn perf_counters_self_test_covers_partial_tuning_restore() -> TestResult {
    let output = Command::new("bash")
        .args(["scripts/perf-counters.sh", "--self-test"])
        .output()?;
    assert!(
        output.status.success(),
        "perf-counters self-test should pass; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("partial failure restored"),
        "self-test should prove partial tuning failures restore earlier writes; stdout={}",
        String::from_utf8_lossy(&output.stdout),
    );
    Ok(())
}

#[test]
fn cli_output_contract_rejects_unsafe_run_ids_before_cleanup() -> TestResult {
    assert_run_id_validation_before(
        "scripts/cli-output-contract.sh",
        "ART_BASE=\"$PWD/tests/artifacts/cli\"",
        "rm -rf -- \"$WORK\"",
    )
}

#[test]
fn wasm_package_gate_rejects_unsafe_run_ids_before_cleanup() -> TestResult {
    assert_run_id_validation_before(
        "scripts/check-wasm-package.sh",
        "ART_BASE=\"$repo_root/tests/artifacts/wasm\"",
        "rm -rf -- \"$WORK\"",
    )
}

#[test]
fn artifact_scripts_reject_unsafe_run_ids_before_artifact_paths() -> TestResult {
    for (script, marker) in [
        (
            "scripts/pagination-proof.sh",
            "ART=\"tests/artifacts/pagination/${RUN_ID}\"",
        ),
        (
            "scripts/theme-proof.sh",
            "ART=\"tests/artifacts/theme/${RUN_ID}\"",
        ),
        (
            "scripts/check-pdf-ua.sh",
            "ART=\"tests/artifacts/pdf-ua/${RUN_ID}\"",
        ),
        (
            "scripts/check-claim-discipline.sh",
            "ART=\"tests/artifacts/claims/${RUN_ID}\"",
        ),
        (
            "scripts/commonmark-conformance.sh",
            "ART=\"tests/artifacts/conformance/${RUN_ID}\"",
        ),
        ("scripts/coverage.sh", "ART=\"$ART_ROOT/$RUN_ID\""),
        (
            "scripts/mutation.sh",
            "ART=\"tests/artifacts/mutation/${RUN_ID}\"",
        ),
        (
            "scripts/batch-throughput.sh",
            "OUT_DIR=\"tests/artifacts/perf/$RUN_ID\"",
        ),
        (
            "scripts/perf-counters.sh",
            "OUT_DIR=\"tests/artifacts/perf/$RUN_ID\"",
        ),
        (
            "scripts/perf-gauntlet.sh",
            "ARTIFACT_DIR=\"tests/artifacts/perf/$RUN_ID\"",
        ),
        (
            "scripts/verify-showcase-mermaid.sh",
            "ART=\"tests/artifacts/svg-showcase/${RUN_ID}\"",
        ),
        (
            "scripts/parser-perf.sh",
            "ARTIFACT_DIR=\"tests/artifacts/perf/$RUN_ID\"",
        ),
        (
            "scripts/layout-perf-proof.sh",
            "ARTIFACT_DIR=\"tests/artifacts/perf/$RUN_ID\"",
        ),
        (
            "scripts/pdf-perf-proof.sh",
            "ARTIFACT_DIR=\"tests/artifacts/perf/$RUN_ID\"",
        ),
    ] {
        assert_run_id_validation_before(script, marker, marker)?;
    }

    Ok(())
}

#[test]
fn showcase_mermaid_verifier_pins_frankenmermaid_reproduction_contract() -> TestResult {
    let script = fs::read_to_string("scripts/verify-showcase-mermaid.sh")?;
    for needle in [
        "FRANKENMERMAID_BIN",
        "examples/showcase-mermaid.mmd",
        "examples/showcase-frankenmermaid.toml",
        "examples/showcase-mermaid.svg",
        "--no-embed-source-spans",
        "cmp -s \"$EXPECTED_SVG\" \"$GENERATED_SVG\"",
    ] {
        assert!(
            script.contains(needle),
            "showcase Mermaid verifier should contain reproduction contract needle {needle:?}"
        );
    }
    Ok(())
}

#[test]
fn showcase_mermaid_verifier_rejects_unsafe_run_ids_before_tool_resolution() -> TestResult {
    let output = Command::new("bash")
        .args(["scripts/verify-showcase-mermaid.sh", "--run-id", "../bad"])
        .env("FRANKENMERMAID_BIN", "/definitely/missing/fm-cli")
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(64),
        "unsafe run ids should be rejected before invoking frankenmermaid; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("verify-showcase-mermaid: run-id must match"),
        "unsafe run id should get the shared run-id grammar error: {stderr}",
    );
    assert!(
        !stderr.contains("could not find fm-cli"),
        "tool resolution should not run before run-id validation: {stderr}",
    );
    Ok(())
}

#[test]
fn e2e_cleanup_scripts_reject_unsafe_run_ids_before_cleanup() -> TestResult {
    assert_run_id_validation_before(
        "scripts/e2e/run-all.sh",
        "ART=\"tests/artifacts/e2e/${RUN_ID}-all\"",
        "rm -rf -- \"$ART\"",
    )?;
    let run_all = fs::read_to_string("scripts/e2e/run-all.sh")?;
    let aggregate_validation = run_all
        .find("fmd_validate_run_id \"e2e run-all\" \"${RUN_ID}-all\" \"aggregate run-id\"")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "run-all should validate the derived aggregate run id",
            )
        })?;
    let suite_validation = run_all
        .find("fmd_validate_run_id \"e2e run-all\" \"${RUN_ID}-$s\" \"suite run-id\"")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "run-all should validate derived suite run ids",
            )
        })?;
    let cleanup = run_all.find("rm -rf -- \"$ART\"").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "run-all cleanup marker missing")
    })?;
    let build = run_all.find("run-all: building fmd").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "run-all build marker missing")
    })?;
    assert!(
        aggregate_validation < cleanup,
        "aggregate run-id validation must happen before aggregate cleanup"
    );
    assert!(
        suite_validation < build,
        "suite run-id validation must happen before the expensive build"
    );

    assert_run_id_validation_before(
        "scripts/e2e/lib.sh",
        "E2E_ART=\"${E2E_REPO_ROOT}/tests/artifacts/e2e/${E2E_RUN_ID}\"",
        "rm -rf -- \"$E2E_ART\"",
    )
}
