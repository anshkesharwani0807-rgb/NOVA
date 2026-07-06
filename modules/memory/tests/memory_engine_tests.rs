//! Comprehensive tests for the encrypted SQLite Memory Engine (Milestone 4).

use std::sync::Arc;

use nova_kernel::{HealthStatus, KernelModule};
use nova_memory::{MemoryCategory, MemoryEngine, MemoryOp, MemoryRecord, Query, SortBy};

/// Build an engine over a fresh temp directory. Returns (engine, tempdir) — keep the
/// tempdir alive for the duration of the test.
fn engine() -> (MemoryEngine, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mem.db");
    let key = dir.path().join("mem.key");
    let eng = MemoryEngine::with_paths(&db, &key);
    eng.open().unwrap();
    (eng, dir)
}

fn record(cat: MemoryCategory, title: &str, content: &str) -> MemoryRecord {
    MemoryRecord::new(cat, title, content)
}

#[test]
fn initialization_creates_schema_v1() {
    let (eng, _dir) = engine();
    assert!(eng.is_open());
    // Empty database, migrated to v1.
    assert_eq!(eng.total().unwrap(), 0);
}

#[test]
fn insert_find_and_exists() {
    let (eng, _dir) = engine();
    let rec = record(MemoryCategory::Knowledge, "Rust", "NOVA core is Rust")
        .with_tags(["lang", "core"])
        .with_importance(80);
    let id = rec.id.clone();
    eng.insert(&rec).unwrap();

    assert!(eng.exists(&id).unwrap());
    let got = eng.find_by_id(&id).unwrap().unwrap();
    assert_eq!(got.title, "Rust");
    assert_eq!(got.content, "NOVA core is Rust");
    assert_eq!(got.tags, vec!["lang".to_string(), "core".to_string()]);
    assert_eq!(got.importance, 80);
    assert_eq!(eng.total().unwrap(), 1);
}

#[test]
fn update_changes_fields() {
    let (eng, _dir) = engine();
    let mut rec = record(MemoryCategory::Reminder, "Call", "Call Sara");
    eng.insert(&rec).unwrap();
    rec.content = "Call Sara at 5pm".to_string();
    eng.update(&rec).unwrap();
    let got = eng.find_by_id(&rec.id).unwrap().unwrap();
    assert_eq!(got.content, "Call Sara at 5pm");
    assert!(got.updated_at >= got.created_at);
}

#[test]
fn soft_delete_hides_but_restore_recovers() {
    let (eng, _dir) = engine();
    let rec = record(MemoryCategory::Contact, "Ravi", "family");
    eng.insert(&rec).unwrap();

    eng.delete(&rec.id).unwrap();
    assert!(!eng.exists(&rec.id).unwrap());
    // Excluded from default queries...
    assert_eq!(eng.find(&Query::new()).unwrap().len(), 0);
    // ...but still present when including deleted.
    assert_eq!(
        eng.find(&Query::new().include_deleted(true)).unwrap().len(),
        1
    );

    eng.restore_record(&rec.id).unwrap();
    assert!(eng.exists(&rec.id).unwrap());
    assert_eq!(eng.find(&Query::new()).unwrap().len(), 1);
}

#[test]
fn purge_deleted_removes_permanently() {
    let (eng, _dir) = engine();
    let rec = record(MemoryCategory::Custom, "temp", "junk");
    eng.insert(&rec).unwrap();
    eng.delete(&rec.id).unwrap();
    assert_eq!(eng.purge_deleted().unwrap(), 1);
    assert_eq!(eng.total().unwrap(), 0);
    assert!(eng.find_by_id(&rec.id).unwrap().is_none());
}

#[test]
fn search_exact_contains_prefix_and_case_insensitive() {
    let (eng, _dir) = engine();
    eng.insert(&record(
        MemoryCategory::Knowledge,
        "Birthday Photos",
        "coast trip 2019",
    ))
    .unwrap();
    eng.insert(&record(
        MemoryCategory::Knowledge,
        "Passport",
        "travel document",
    ))
    .unwrap();

    // contains, case-insensitive by default
    assert_eq!(
        eng.search(&Query::new().contains("PHOTOS")).unwrap().len(),
        1
    );
    // prefix
    assert_eq!(
        eng.search(&Query::new().prefix("Birthday")).unwrap().len(),
        1
    );
    // exact (whole title)
    assert_eq!(
        eng.search(&Query::new().exact("passport")).unwrap().len(),
        1
    );
    // no match
    assert_eq!(
        eng.search(&Query::new().contains("nonexistent"))
            .unwrap()
            .len(),
        0
    );
    // case sensitive miss
    assert_eq!(
        eng.search(&Query::new().contains("photos").case_sensitive())
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn category_tag_filter_pagination_and_sort() {
    let (eng, _dir) = engine();
    for i in 0..5 {
        let rec = record(MemoryCategory::Music, &format!("Song {i}"), "audio")
            .with_tags(["fav"])
            .with_importance(i);
        eng.insert(&rec).unwrap();
    }
    eng.insert(&record(MemoryCategory::Gallery, "Pic", "image").with_tags(["fav"]))
        .unwrap();

    // Category filter.
    assert_eq!(
        eng.count(&Query::new().category(MemoryCategory::Music))
            .unwrap(),
        5
    );
    // Tag filter across categories.
    assert_eq!(eng.count(&Query::new().tag("fav")).unwrap(), 6);
    // Pagination.
    let page = eng
        .find(
            &Query::new()
                .category(MemoryCategory::Music)
                .limit(2)
                .offset(2),
        )
        .unwrap();
    assert_eq!(page.len(), 2);
    // Sort by importance desc → first is importance 4.
    let sorted = eng
        .find(
            &Query::new()
                .category(MemoryCategory::Music)
                .sort(SortBy::ImportanceDesc),
        )
        .unwrap();
    assert_eq!(sorted.first().unwrap().importance, 4);
}

#[test]
fn transaction_is_atomic_and_rolls_back() {
    let (eng, _dir) = engine();
    let ok = record(MemoryCategory::Knowledge, "ok", "valid");
    // Second op updates a non-existent record → whole transaction must roll back.
    let bad = record(MemoryCategory::Knowledge, "bad", "missing");
    let result = eng.transaction(&[MemoryOp::Insert(ok.clone()), MemoryOp::Update(bad)]);
    assert!(result.is_err());
    assert_eq!(
        eng.total().unwrap(),
        0,
        "failed transaction must persist nothing"
    );

    // A fully valid transaction commits.
    let a = record(MemoryCategory::Knowledge, "a", "1");
    let b = record(MemoryCategory::Knowledge, "b", "2");
    eng.transaction(&[MemoryOp::Insert(a), MemoryOp::Insert(b)])
        .unwrap();
    assert_eq!(eng.total().unwrap(), 2);
}

#[test]
fn concurrent_access_is_race_free() {
    let (eng, _dir) = engine();
    let eng = Arc::new(eng);
    let mut handles = Vec::new();
    for t in 0..4 {
        let e = eng.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..25 {
                let rec = record(MemoryCategory::Automation, &format!("t{t}-{i}"), "x");
                e.insert(&rec).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(eng.total().unwrap(), 100);
}

#[test]
fn data_persists_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mem.db");
    let key = dir.path().join("mem.key");

    let id;
    {
        let eng = MemoryEngine::with_paths(&db, &key);
        eng.open().unwrap();
        let rec = record(MemoryCategory::Preference, "theme", "dark");
        id = rec.id.clone();
        eng.insert(&rec).unwrap();
        eng.close();
    }
    // Fresh engine over the same files → data (and key) survive the "restart".
    let eng2 = MemoryEngine::with_paths(&db, &key);
    eng2.open().unwrap();
    let got = eng2.find_by_id(&id).unwrap().unwrap();
    assert_eq!(got.content, "dark");
}

#[test]
fn content_is_encrypted_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mem.db");
    let key = dir.path().join("mem.key");
    let secret = "SUPER_SECRET_PASSPHRASE_42";
    {
        let eng = MemoryEngine::with_paths(&db, &key);
        eng.open().unwrap();
        eng.insert(&record(MemoryCategory::Knowledge, "note", secret))
            .unwrap();
        eng.close();
    }
    // No file in the directory may contain the plaintext secret.
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("key") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "plaintext secret found in {}",
            path.display()
        );
    }
}

#[test]
fn wrong_key_cannot_decrypt() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mem.db");
    let key = dir.path().join("mem.key");
    let other_key = dir.path().join("other.key");

    let id;
    {
        let eng = MemoryEngine::with_paths(&db, &key);
        eng.open().unwrap();
        let rec = record(MemoryCategory::Knowledge, "secret", "top secret");
        id = rec.id.clone();
        eng.insert(&rec).unwrap();
        eng.close();
    }
    // Opening with a different key file → decryption must fail.
    let eng2 = MemoryEngine::with_paths(&db, &other_key);
    eng2.open().unwrap();
    assert!(eng2.find_by_id(&id).is_err());
}

#[test]
fn backup_and_restore_round_trip() {
    let (eng, dir) = engine();
    for i in 0..3 {
        eng.insert(&record(MemoryCategory::Knowledge, &format!("k{i}"), "v"))
            .unwrap();
    }
    let backup = dir.path().join("backup.db");
    eng.backup(&backup).unwrap();

    // Wipe everything, then restore from the backup.
    let all: Vec<_> = eng.find(&Query::new()).unwrap();
    for r in &all {
        eng.purge(&r.id).unwrap();
    }
    assert_eq!(eng.total().unwrap(), 0);

    eng.restore(&backup).unwrap();
    assert_eq!(eng.total().unwrap(), 3);
}

#[test]
fn vacuum_succeeds() {
    let (eng, _dir) = engine();
    eng.insert(&record(MemoryCategory::Knowledge, "k", "v"))
        .unwrap();
    eng.delete(&{
        let all = eng.find(&Query::new()).unwrap();
        all[0].id.clone()
    })
    .unwrap();
    eng.purge_deleted().unwrap();
    eng.vacuum().unwrap();
    assert_eq!(eng.total().unwrap(), 0);
}

#[test]
fn health_reports_open_and_count() {
    let (eng, _dir) = engine();
    eng.insert(&record(MemoryCategory::Knowledge, "k", "v"))
        .unwrap();
    let health = eng.health();
    assert_eq!(health.status, HealthStatus::Healthy);
    assert!(health.detail.contains('1'));

    eng.close();
    assert_eq!(eng.health().status, HealthStatus::Unhealthy);
}
