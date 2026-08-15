//! Every recorded fact must be readable.
//!
//! # The defect this exists to prevent
//!
//! Four consecutive slices found the same shape and fixed it one instance at a
//! time: a `pub struct` with private fields, validated on the way in, and no
//! accessor. `DataExportPlan::object_revisions`, the recovery types' evidence,
//! `QualificationBaseline`'s invalidation reason, the external effect receipt's
//! response digest — each was written, each was checked on construction, and
//! none could be read.
//!
//! It is a specific kind of dishonesty because the type *looks* like it records
//! something. It hurts EVIDENCE-class requirements most: the whole requirement
//! is that the record survives, the field satisfies it to the author's eye, and
//! nothing outside the type can ask whether it did. `AuditIntegrityEntry` was
//! the extreme case — a hash-chained audit record with nine private fields and
//! no `impl` block at all, so nothing outside one file could verify the chain or
//! say what an entry was about.
//!
//! A sweep found **100 such fields across 26 types**. Fixing them again by hand
//! next time is not a plan, so this is the check.
//!
//! # What it does not claim
//!
//! Reading a field is not the same as the field being meaningful, and a getter
//! is not an invariant. This only asserts that what a type records can be asked
//! for. The exception list below is for state that is genuinely internal, and
//! every entry on it names why.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Fields that are deliberately unreadable, with the reason.
///
/// Each of these is machinery rather than a record of something that happened.
const INTERNAL_STATE: &[(&str, &str, &str)] = &[
    (
        "ConformanceRunner",
        "cases",
        "the registered case list is the runner's own machinery; it is exercised \
         by running it, not by reading it",
    ),
    (
        "RepairBudget",
        "max_attempts",
        "configuration the caller supplied; the budget's observable behaviour is \
         whether it grants an attempt",
    ),
    (
        "RepairBudget",
        "cooldown",
        "as above — the cooldown shows up in the refusal, which carries the time \
         it lasts until",
    ),
    (
        "ClassificationPolicy",
        "allow_unclassified",
        "a policy switch, not a recorded fact; its effect is visible in what the \
         policy admits",
    ),
];

fn domain_sources(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.join("src")];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).expect("the source tree is readable") {
            let path = entry.expect("a directory entry is readable").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files
}

/// The body of the brace-delimited block starting at `open`.
fn block(text: &str, open: usize) -> &str {
    let bytes = text.as_bytes();
    let mut depth = 1usize;
    let mut index = open + 1;
    while index < bytes.len() && depth > 0 {
        match bytes[index] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    &text[open + 1..index.saturating_sub(1)]
}

fn private_fields(body: &str) -> Vec<String> {
    let mut fields = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        if line.starts_with("pub ") || line.starts_with("pub(") {
            continue;
        }
        let Some((name, _)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            continue;
        }
        fields.push(name.to_string());
    }
    fields
}

#[test]
fn every_recorded_field_can_be_read() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exceptions: BTreeSet<(&str, &str)> = INTERNAL_STATE
        .iter()
        .map(|(type_name, field, _)| (*type_name, *field))
        .collect();

    let mut unreadable: Vec<String> = Vec::new();

    for path in domain_sources(&root) {
        let text = fs::read_to_string(&path).expect("a source file is readable");
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");

        let mut search = 0usize;
        while let Some(offset) = text[search..].find("\npub struct ") {
            let start = search + offset + 1;
            let rest = &text[start..];
            let Some(brace) = rest.find('{') else { break };
            // A tuple struct or a unit struct has no named fields to hide.
            if rest[..brace].contains(';') {
                search = start + brace;
                continue;
            }
            let name: String = rest["pub struct ".len()..brace]
                .split(|character: char| character == '<' || character.is_whitespace())
                .next()
                .unwrap_or_default()
                .to_string();
            let body = block(&text, start + brace);
            search = start + brace + body.len();

            let fields = private_fields(body);
            if fields.is_empty() {
                continue;
            }

            // Every impl block for the type, in this file.
            let mut methods = String::new();
            let needle = format!("\nimpl {name} ");
            let generic_needle = format!("\nimpl<");
            for (index, _) in text.match_indices(&needle) {
                methods.push_str(&text[index..]);
            }
            for (index, _) in text.match_indices(&generic_needle) {
                let candidate = &text[index..];
                if candidate
                    .lines()
                    .next()
                    .is_some_and(|line| line.contains(&format!("> {name} ")))
                {
                    methods.push_str(candidate);
                }
            }

            for field in fields {
                if exceptions.contains(&(name.as_str(), field.as_str())) {
                    continue;
                }
                // A reader is any public method whose name contains the field's,
                // which allows `recovery_provenance` to read `provenance` where
                // the plain name would collide.
                let readable = methods
                    .match_indices("pub ")
                    .filter_map(|(index, _)| methods[index..].lines().next())
                    .filter(|line| line.contains("fn "))
                    .any(|line| line.contains(&field));
                if !readable {
                    unreadable.push(format!("{name}.{field} ({relative})"));
                }
            }
        }
    }

    assert!(
        unreadable.is_empty(),
        "these fields are recorded and cannot be read, so anything that depends on them \
         being kept is unverifiable from outside the type:\n  {}\n\nAdd a reader, or add the \
         field to INTERNAL_STATE with the reason it is machinery rather than a record.",
        unreadable.join("\n  ")
    );
}

/// The exception list is a list of decisions, and a decision with no reason is
/// a hole with a comment over it.
#[test]
fn every_exception_says_why() {
    for (type_name, field, reason) in INTERNAL_STATE {
        assert!(
            reason.split_whitespace().count() >= 8,
            "{type_name}.{field} is excepted without explaining itself"
        );
    }
}
