//! A migration that writes rows must be able to see them.
//!
//! # The defect this exists to prevent
//!
//! Migration 0150 forced row level security on 28 tables. Migrations run as the
//! runtime role, which **owns** those tables — and a *forced* policy applies to
//! the owner too. The policy admits only rows matching `vestrace.workspace_id`,
//! which a migration has no single value for, because it is fixing every
//! workspace at once.
//!
//! So a backfill in a later migration sees no rows at all. The first version of
//! 0151 hit this and reported:
//!
//! ```text
//! UPDATE 0
//! ERROR:  column "reconciled_at" of relation "external_reconciliations"
//!         contains null values
//! ```
//!
//! `UPDATE 0` against a table with rows in it. That one failed loudly only
//! because it went on to add a `NOT NULL` the un-backfilled rows violated. A
//! migration that normalised a column, repaired bad values, or populated a
//! nullable one would have **reported success having changed nothing**, and
//! there would be no error anywhere to find afterwards.
//!
//! The fix in a migration is to lift `FORCE` around the write and restore it,
//! which is safe there and nowhere else: `ALTER TABLE` takes an ACCESS EXCLUSIVE
//! lock, a migration is one transaction, and DDL in Postgres is transactional —
//! so no other session reads the table while the policy is lifted, and a failure
//! rolls back to `FORCE` rather than leaving it open.
//!
//! Remembering that every time is not a plan. This is the check.
//!
//! # What it does not claim
//!
//! This reads SQL as text. It does not parse it, so it recognises the shapes
//! this repository actually writes — `UPDATE <table>`, `INSERT INTO <table>`,
//! `DELETE FROM <table>` — and would miss a write hidden behind a CTE, a
//! function, or `EXECUTE`. It also cannot tell whether a lifted `FORCE` was
//! restored correctly; [`force_is_restored`] checks that separately by counting.
//!
//! A passing run means no migration writes to a forced table without saying so.
//! It does not mean the writes are right.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

/// Every migration file, in the order Postgres applies them.
fn migration_files() -> Vec<(String, String)> {
    let mut files: Vec<_> = fs::read_dir(migrations_dir())
        .expect("the migrations directory is readable")
        .filter_map(|entry| {
            let path = entry.expect("a readable directory entry").path();
            if path.extension().and_then(|value| value.to_str()) != Some("sql") {
                return None;
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .expect("a migration filename is valid UTF-8")
                .to_owned();
            let body = fs::read_to_string(&path).expect("a migration file is readable");
            Some((name, body))
        })
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    assert!(
        !files.is_empty(),
        "no migrations were found at {}",
        migrations_dir().display()
    );
    files
}

/// Everything between a pair of dollar quotes, removed.
///
/// A function body is not a migration statement. 0136 defines
/// `vestrace_touch_access_token`, whose body is `UPDATE access_tokens ...` — and
/// that runs at request time, as the caller, under a policy written for exactly
/// that call, with `vestrace.authenticating_token_id` set. It is not a backfill
/// and it sees the row it is aimed at.
///
/// The guard flagged it before this existed, which is the difference between a
/// check that finds defects and one that finds text.
fn strip_dollar_quoted(sql: &str) -> String {
    let bytes: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != '$' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        // A dollar quote is `$tag$` with an alphanumeric (possibly empty) tag.
        let Some(tag_end) = (index + 1..bytes.len())
            .find(|&position| !(bytes[position].is_alphanumeric() || bytes[position] == '_'))
        else {
            out.push(bytes[index]);
            index += 1;
            continue;
        };
        if bytes[tag_end] != '$' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        let delimiter: String = bytes[index..=tag_end].iter().collect();
        let rest: String = bytes[tag_end + 1..].iter().collect();
        match rest.find(&delimiter) {
            Some(close) => {
                out.push(' ');
                index = tag_end + 1 + close + delimiter.len();
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    out
}

/// SQL with comments and function bodies removed, lowercased.
///
/// Comments matter here: every migration in this repository explains itself, and
/// those explanations mention the statements they are about. Without stripping
/// them, the prose describing a fix reads as the defect.
fn statements(body: &str) -> String {
    let without_comments = body
        .lines()
        .map(|line| match line.find("--") {
            Some(index) => &line[..index],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    strip_dollar_quoted(&without_comments).to_lowercase()
}

/// Table names following `pattern`, e.g. `"update "` or `"insert into "`.
fn tables_after(sql: &str, pattern: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = sql;
    while let Some(index) = rest.find(pattern) {
        rest = &rest[index + pattern.len()..];
        let name: String = rest
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }
    found
}

/// Tables that are under a forced policy by the time `up_to` runs.
fn forced_before(files: &[(String, String)], up_to: &str) -> BTreeSet<String> {
    let mut forced = BTreeSet::new();
    for (name, body) in files {
        if name.as_str() >= up_to {
            break;
        }
        let sql = statements(body);
        for table in tables_after(&sql, "alter table ") {
            let force = format!("alter table {table} force row level security");
            let unforce = format!("alter table {table} no force row level security");
            // A file may lift and restore in the same migration; what matters
            // for a *later* one is where it left the table.
            let last_force = sql.rfind(&force);
            let last_unforce = sql.rfind(&unforce);
            match (last_force, last_unforce) {
                (Some(on), Some(off)) => {
                    if on > off {
                        forced.insert(table);
                    } else {
                        forced.remove(&table);
                    }
                }
                (Some(_), None) => {
                    forced.insert(table);
                }
                (None, Some(_)) => {
                    forced.remove(&table);
                }
                (None, None) => {}
            }
        }
    }
    forced
}

/// Data writes in `sql`, by table.
fn writes(sql: &str) -> BTreeMap<String, &'static str> {
    let mut found = BTreeMap::new();
    for (pattern, kind) in [
        ("update ", "UPDATE"),
        ("insert into ", "INSERT"),
        ("delete from ", "DELETE"),
    ] {
        for table in tables_after(sql, pattern) {
            found.entry(table).or_insert(kind);
        }
    }
    found
}

#[test]
fn a_migration_that_writes_to_a_forced_table_lifts_the_policy_first() {
    let files = migration_files();
    let mut offences = Vec::new();

    for (name, body) in &files {
        let sql = statements(body);
        let forced = forced_before(&files, name);
        for (table, kind) in writes(&sql) {
            if !forced.contains(&table) {
                continue;
            }
            let lifted = sql.contains(&format!("alter table {table} no force row level security"));
            if !lifted {
                offences.push(format!(
                    "{name}: {kind} on `{table}`, which is under a forced policy — the runtime \
                     role owns the table and a forced policy applies to owners, so this affects \
                     no rows and reports success"
                ));
            }
        }
    }

    assert!(
        offences.is_empty(),
        "migrations write to forced tables without lifting the policy:\n  {}",
        offences.join("\n  ")
    );
}

/// A migration that lifts `FORCE` must put it back.
///
/// Kept apart from the check above because the failures are opposite: that one
/// catches a write nobody can see, this one catches a table left readable to
/// every workspace afterwards. The second is the worse of the two.
#[test]
fn force_is_restored() {
    let files = migration_files();
    let mut offences = Vec::new();

    for (name, body) in &files {
        let sql = statements(body);
        for table in tables_after(&sql, "alter table ") {
            let unforce = format!("alter table {table} no force row level security");
            let Some(lifted_at) = sql.rfind(&unforce) else {
                continue;
            };
            let force = format!("alter table {table} force row level security");
            // `rfind` on the restore has to look past the lift, since the lift
            // contains the restore as a substring.
            let restored = sql[lifted_at + unforce.len()..].contains(&force);
            if !restored {
                offences.push(format!(
                    "{name}: lifts the forced policy on `{table}` and does not restore it, so \
                     every later read by the owning role sees every workspace's rows"
                ));
            }
        }
    }

    assert!(
        offences.is_empty(),
        "migrations leave a forced policy lifted:\n  {}",
        offences.join("\n  ")
    );
}

/// The guard is only worth having if it is looking at something.
///
/// A path typo, a renamed directory or a comment-stripping bug would leave both
/// checks above passing over nothing at all. This asserts that the inputs they
/// depend on are non-empty and that the case which prompted the guard is
/// actually visible to it.
#[test]
fn the_guard_is_looking_at_the_right_thing() {
    let files = migration_files();
    // Numbering is not contiguous — the count is smaller than the highest
    // number — so this is a floor that a wrong directory would fall through,
    // not a count of anything.
    assert!(
        files.len() > 50,
        "only {} migrations were read; the guard is looking at the wrong place",
        files.len()
    );
    assert!(
        files.iter().any(|(name, _)| name.starts_with("0150_")),
        "the migration that forced row level security is not among the files read"
    );

    // A function body must not read as a migration statement: 0136's
    // `UPDATE access_tokens` runs at request time under its own policy.
    let (_, token_policy) = files
        .iter()
        .find(|(name, _)| name.starts_with("0136_"))
        .expect("0136 defines the access-token functions");
    assert!(
        !writes(&statements(token_policy)).contains_key("access_tokens"),
        "a function body was read as a migration-time write"
    );

    let forced_now = forced_before(&files, "9999_end.sql");
    assert!(
        forced_now.contains("external_reconciliations"),
        "the table whose backfill found this defect is not seen as forced, so the guard would \
         not have caught it"
    );
    assert!(
        forced_now.len() >= 28,
        "only {} tables are seen as forced; 0150 forces 28",
        forced_now.len()
    );

    // And the migration that does write to it is seen as writing to it.
    let (_, backfill) = files
        .iter()
        .find(|(name, _)| name.starts_with("0151_"))
        .expect("0151 is the backfill that found this");
    let sql = statements(backfill);
    assert_eq!(
        writes(&sql).get("external_reconciliations"),
        Some(&"UPDATE"),
        "the backfill's UPDATE is not recognised as a write"
    );
}
