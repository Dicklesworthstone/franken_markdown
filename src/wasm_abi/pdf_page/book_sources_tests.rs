#![allow(clippy::expect_used)]

use super::{admit_sources, source_bundle};

#[test]
fn source_bundle_admission_preserves_roles_and_rejects_mismatched_arrays_and_limits() {
    let (chapters, resources) = source_bundle(
        vec!["one.md".into()],
        vec!["# One".into()],
        vec!["data.txt".into()],
        vec!["line one\r\nline two\r\n".into()],
        true,
    )
    .expect("explicit source roles");
    assert_eq!(chapters.len(), 1);
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].path, "data.txt");
    assert_eq!(resources[0].source, "line one\r\nline two\r\n");

    for (paths, sources, include_paths, include_sources, expand) in [
        (vec![], vec![], vec!["x".into()], vec!["X".into()], true),
        (vec!["a.md".into()], vec![], vec![], vec![], true),
        (
            vec!["a.md".into()],
            vec!["A".into()],
            vec!["x".into()],
            vec![],
            true,
        ),
        (
            vec!["a.md".into()],
            vec!["A".into()],
            vec![],
            vec!["X".into()],
            true,
        ),
        (
            vec!["a.md".into()],
            vec!["A".into()],
            vec!["x".into()],
            vec!["X".into()],
            false,
        ),
        (
            vec!["a.md".into(); 4096],
            vec!["A".into(); 4096],
            vec!["x".into()],
            vec!["X".into()],
            true,
        ),
        (
            vec!["a.md".into(); 4097],
            vec!["A".into(); 4097],
            vec![],
            vec![],
            true,
        ),
    ] {
        assert!(source_bundle(paths, sources, include_paths, include_sources, expand).is_err());
    }

    let roots = vec!["a.md".to_string()];
    let texts = vec!["é".to_string()];
    let parts = vec!["part.txt".to_string()];
    let part_texts = vec!["中".to_string()];
    // 4 + 2 + 8 + 3 UTF-8 bytes: a combined budget, including resource paths,
    // rather than separate allowances for the two independently valid lists.
    assert!(admit_sources(&roots, &texts, &parts, &part_texts, true, 17).is_ok());
    assert!(admit_sources(&roots, &texts, &parts, &part_texts, true, 16).is_err());
    assert!(admit_sources(&roots, &texts, &[], &[], false, 6).is_ok());
}
