# Pure B/I/M decision contract: illustrative test

This proposed Rust fragment targets crates/vestrace-domain/tests/source_sync.rs; it is not an existing public API. Only a trusted comparison service creates SyncFacts after authorized reads of exact source/memory revisions. HTTP cannot supply these flags. This example tests a pure rule, not authorization, durability, or byte equality.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncDecision { Unchanged, Rename, Update, Conflict, Missing, Blocked }
#[derive(Clone, Copy)]
struct SyncFacts {
    same_content: bool,
    locator_changed: bool,
    manual_override: bool,
    source_missing: bool,
    scan_complete: bool,
    label_changed: bool,
}
fn classify_sync(f: SyncFacts) -> SyncDecision {
    if f.label_changed { return SyncDecision::Blocked; }
    if f.source_missing {
        return if f.scan_complete { SyncDecision::Missing } else { SyncDecision::Unchanged };
    }
    if f.same_content {
        return if f.locator_changed { SyncDecision::Rename } else { SyncDecision::Unchanged };
    }
    if f.manual_override { SyncDecision::Conflict } else { SyncDecision::Update }
}
#[test]
fn changed_source_does_not_overwrite_manual_text() {
    let f = SyncFacts { same_content: false, locator_changed: false,
        manual_override: true, source_missing: false, scan_complete: true,
        label_changed: false };
    assert_eq!(classify_sync(f), SyncDecision::Conflict);
}
#[test]
fn incomplete_scan_does_not_mark_unseen_sources_missing() {
    let f = SyncFacts { same_content: false, locator_changed: false,
        manual_override: false, source_missing: true, scan_complete: false,
        label_changed: false };
    assert_eq!(classify_sync(f), SyncDecision::Unchanged);
}
```

In production, manual_override derives from the durable binding and current-memory comparison with the last imported revision. A client boolean is not evidence. Conflict requires an exact B/I/M resolution through a separate mutation, never automatic semantic merging. Revalidate the comparison against pinned base versions under locks before writing.
