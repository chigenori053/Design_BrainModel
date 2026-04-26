use std::collections::BTreeMap;

use crate::patch_generator::Patch;

pub fn apply_patch(code: &str, patch: &Patch) -> Result<String, ApplyError> {
    if patch
        .edits
        .iter()
        .map(|edit| edit.file.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1
    {
        return Err(ApplyError::MultipleFiles);
    }
    let mut content = code.to_string();
    let mut ranges = patch
        .edits
        .iter()
        .map(|edit| (edit.start, edit.end, edit.replacement.clone()))
        .collect::<Vec<_>>();
    ranges.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
    ensure_non_overlapping(&ranges)?;
    for (start, end, replacement) in ranges.into_iter().rev() {
        if end > content.len() || start > end {
            return Err(ApplyError::OutOfBounds { start, end });
        }
        content.replace_range(start..end, &replacement);
    }
    Ok(content)
}

pub fn apply_patch_to_files(
    files: &BTreeMap<String, String>,
    patch: &Patch,
) -> Result<BTreeMap<String, String>, ApplyError> {
    let mut grouped = BTreeMap::<String, Vec<(usize, usize, String)>>::new();
    for edit in &patch.edits {
        grouped.entry(edit.file.clone()).or_default().push((
            edit.start,
            edit.end,
            edit.replacement.clone(),
        ));
    }

    let mut next = files.clone();
    for (file, mut edits) in grouped {
        edits.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
        ensure_non_overlapping(&edits)?;
        let current = next.get(&file).cloned().unwrap_or_default();
        let mut updated = current;
        for (start, end, replacement) in edits.into_iter().rev() {
            if end > updated.len() || start > end {
                return Err(ApplyError::OutOfBounds { start, end });
            }
            updated.replace_range(start..end, &replacement);
        }
        if updated.is_empty() {
            next.remove(&file);
        } else {
            next.insert(file, updated);
        }
    }
    Ok(next)
}

pub fn rollback(snapshot: &str) -> String {
    snapshot.to_string()
}

fn ensure_non_overlapping(edits: &[(usize, usize, String)]) -> Result<(), ApplyError> {
    for pair in edits.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(ApplyError::OverlappingEdits);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    OverlappingEdits,
    OutOfBounds { start: usize, end: usize },
    MultipleFiles,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OverlappingEdits => write!(f, "overlapping edits are not allowed"),
            Self::OutOfBounds { start, end } => {
                write!(f, "edit range {start}..{end} is out of bounds")
            }
            Self::MultipleFiles => write!(
                f,
                "single-file apply_patch received edits for multiple files"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}
