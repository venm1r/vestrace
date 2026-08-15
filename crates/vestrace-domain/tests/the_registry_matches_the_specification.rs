//! The gate must measure the requirements the specification states.
//!
//! # The defect this exists to prevent
//!
//! `docs/specs/vestrace-normative-invariants-v0.2.md` is the authoritative
//! catalogue. `registry.rs` is what the conformance gate actually evaluates, and
//! it is a hand-maintained copy. They had drifted on **122 of 199**
//! requirements.
//!
//! Drift here is not cosmetic, because both fields are load-bearing:
//!
//! - **Class decides what evidence is admissible.** The hard gate admits
//!   `EvidenceOrigin::LocalAttested` — somebody writing down that they read the
//!   code — only where the class is `Static`. Six requirements were `Static` in
//!   the registry and something else in the catalogue, CAP-001 among them: a
//!   MUST/SECURITY requirement that the gate would have accepted an attestation
//!   for. None had actually used one, so the weakening was latent rather than
//!   exercised, which is the only reason this is a near miss rather than an
//!   incident.
//! - **Level decides whether a profile can close without it.** Five MUSTs were
//!   recorded as SHOULD, including IDW-012 — "federation trust recognition must
//!   not by itself permit data disclosure" — and QUAL-014, on invalidating
//!   qualification after a material change. Seven SHOULDs were recorded as MUST,
//!   which is the harmless direction and still not what the specification says.
//! - Seventeen `MUST NOT`s were flattened to `MUST`, losing the polarity of the
//!   obligation.
//!
//! A hand-maintained copy of a specification drifts. That is not a failure of
//! care, it is what hand-maintained copies do, so this checks it mechanically.
//!
//! # What it does not claim
//!
//! Only level and class are compared, because only those are directly
//! comparable: the catalogue states each requirement in Russian and the registry
//! carries an English paraphrase, and no test can tell whether a paraphrase is
//! faithful. **Statement drift is real and this does not catch it** — IDW-010's
//! registry statement said "grant and mount operations must be auditable" while
//! the catalogue says `SharedMemoryRef` must not be substitutable for a local
//! `MemoryId`, two different requirements under one id. Comparing those needs a
//! reader.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use vestrace_domain::conformance::registry::all as requirements;
use vestrace_domain::conformance::{RequirementLevel as L, VerificationClass as V};

/// The catalogue, as `(family, number) -> (level, class)`.
fn catalogue() -> BTreeMap<(String, u16), (String, String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/vestrace-normative-invariants-v0.2.md");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the catalogue at {} is readable: {error}", path.display()));

    let mut found = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // `| ID | LEVEL | CLASS | statement |` — leading and trailing empties.
        if cells.len() < 5 {
            continue;
        }
        let Some((family, number)) = cells[1].split_once('-') else {
            continue;
        };
        let Ok(number) = number.parse::<u16>() else {
            continue;
        };
        if !family.chars().all(|c| c.is_ascii_uppercase()) || family.is_empty() {
            continue;
        }
        let level = cells[2];
        let class = cells[3];
        if !matches!(level, "MUST" | "MUST NOT" | "SHOULD" | "SHOULD NOT" | "MAY") {
            continue;
        }
        let statement = cells.get(4).copied().unwrap_or_default();
        found.insert(
            (family.to_owned(), number),
            (level.to_owned(), class.to_owned(), statement.to_owned()),
        );
    }
    assert!(
        found.len() > 150,
        "only {} catalogue entries parsed; the table format changed and this test is reading \
         nothing",
        found.len()
    );
    found
}

fn level_name(level: L) -> &'static str {
    match level {
        L::Must => "MUST",
        L::MustNot => "MUST NOT",
        L::Should => "SHOULD",
        L::ShouldNot => "SHOULD NOT",
        L::May => "MAY",
    }
}

fn class_name(class: V) -> &'static str {
    match class {
        V::Static => "STATIC",
        V::Domain => "DOMAIN",
        V::Stateful => "STATEFUL",
        V::Security => "SECURITY",
        V::Fault => "FAULT",
        V::Recovery => "RECOVERY",
        V::Interop => "INTEROP",
        V::Evidence => "EVIDENCE",
    }
}

#[test]
fn every_requirement_carries_the_level_and_class_the_catalogue_gives_it() {
    let catalogue = catalogue();
    let mut drift = Vec::new();
    let mut absent = Vec::new();

    for requirement in requirements() {
        let family = requirement.id.family.to_string().to_uppercase();
        let key = (family.clone(), requirement.id.number);
        let Some((level, class, _)) = catalogue.get(&key) else {
            absent.push(format!("{}-{:03}", family, requirement.id.number));
            continue;
        };
        let registry = (level_name(requirement.level), class_name(requirement.class));
        if registry.0 != level || registry.1 != class {
            drift.push(format!(
                "{}-{:03}: catalogue {level}/{class}, registry {}/{}",
                family, requirement.id.number, registry.0, registry.1
            ));
        }
    }

    assert!(
        absent.is_empty(),
        "the registry carries requirements the catalogue does not state: {}",
        absent.join(", ")
    );
    assert!(
        drift.is_empty(),
        "the gate measures {} requirement(s) at a level or class the specification does not \
         give them — class decides whether an attestation is admissible and level decides \
         whether a profile can close without the requirement:\n  {}",
        drift.len(),
        drift.join("\n  ")
    );
}

/// Attestation is only ever correct for `Static`, so the set that admits it is
/// worth stating out loud.
#[test]
fn only_static_requirements_can_be_satisfied_by_an_attestation() {
    let catalogue = catalogue();
    let mut wrong = Vec::new();
    for requirement in requirements() {
        let family = requirement.id.family.to_string().to_uppercase();
        let Some((_, class, _)) = catalogue.get(&(family.clone(), requirement.id.number))
        else {
            continue;
        };
        if matches!(requirement.class, V::Static) && class != "STATIC" {
            wrong.push(format!(
                "{}-{:03} is {class} in the catalogue",
                family, requirement.id.number
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "requirement(s) admit a local attestation that the specification does not classify as \
         STATIC, so reading the code would count as evidence for them:\n  {}",
        wrong.join("\n  ")
    );
}


/// The authoritative wording is carried verbatim, not paraphrased.
///
/// `statement` is an English gloss and glosses drift: an audit found only 61 of
/// 199 registry statements are even the closest match for their own catalogue
/// entry, and several describe a different requirement — registry CAP-012 states
/// the Brain/Face/organ authority intersection, which is a **post-v0.2
/// extension**, while catalogue CAP-012 is about a material change invalidating
/// an approval. `spec_statement` is the catalogue's own text, and this keeps it
/// byte-identical so the gate cannot quietly restate what it measures.
#[test]
fn every_requirement_carries_the_catalogues_own_wording() {
    let catalogue = catalogue();
    let mut wrong = Vec::new();
    for requirement in requirements() {
        let family = requirement.id.family.to_string().to_uppercase();
        let Some((_, _, statement)) = catalogue.get(&(family.clone(), requirement.id.number))
        else {
            continue;
        };
        if requirement.spec_statement != statement {
            wrong.push(format!(
                "{}-{:03}
    catalogue: {statement}
    registry : {}",
                family, requirement.id.number, requirement.spec_statement
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} requirement(s) do not carry the catalogue's wording:
  {}",
        wrong.len(),
        wrong.join("
  ")
    );
}
