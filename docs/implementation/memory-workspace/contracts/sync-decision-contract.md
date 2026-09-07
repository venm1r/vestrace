# Чистый контракт B/I/M: пример теста для реализации

Это proposed Rust fragment для `crates/vestrace-domain/tests/source_sync.rs`, не уже существующий API. `SyncFacts` создаёт только trusted comparison service после чтения exact source/memory revisions; HTTP не может передать эти flags. Тест проверяет чистое правило, не полномочия/долговечность/сравнение bytes.

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

В продукте `SyncFacts.manual_override` выводится из durable binding и проверки текущей memory revision относительно последней импортной. Клиентский boolean не является доказательством. Данный пример намеренно не выполняет automatic semantic merge: ветка Conflict требует exact B/I/M resolution отдельной mutation. До записи результат чистого сравнения revalidated против pinned base versions под lock.
