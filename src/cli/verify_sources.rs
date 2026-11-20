//! Prepare the same bounded native source graph used by rendering, while
//! protecting every consulted file from the verifier's optional cache writer.

use std::path::Path;

use super::protected_paths::ProtectedInputs;

pub(super) struct PreparedSource {
    pub text: String,
    pub expanded: bool,
    pub inputs: ProtectedInputs,
}

type Failure = (u8, &'static str, String);

pub(super) fn prepare(
    source: String,
    input: &Path,
    max_bytes: u64,
    profile: Option<crate::Profile>,
) -> Result<PreparedSource, Failure> {
    let mut inputs = ProtectedInputs::default();
    let stdin = input == Path::new("-");
    if !stdin {
        inputs.insert(input);
    }
    if !crate::transclude::has_includes(&source) {
        return Ok(PreparedSource {
            text: source,
            expanded: false,
            inputs,
        });
    }
    let text = if stdin {
        let budget = usize::try_from(max_bytes)
            .unwrap_or(usize::MAX)
            .min(crate::transclude::DEFAULT_MAX_EXPANDED_BYTES);
        crate::transclude::expand_includes_with_limit_and_profile(
            &source,
            &|_, _| Err("{{#include}} needs a file input (stdin has no base directory)".into()),
            budget,
            profile,
        )
        .map_err(|error| (64, "usage_error", error.to_string()))?
    } else {
        let expansion =
            crate::watch::expand_file_includes_with_profile(&source, input, max_bytes, profile);
        for path in expansion.input_paths() {
            inputs.insert(path);
        }
        expansion
            .result
            .map_err(|message| (66, "input_error", message))?
    };
    Ok(PreparedSource {
        expanded: text != source,
        text,
        inputs,
    })
}
