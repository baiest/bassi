use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use nala::adapters::memory::file::FileMemoryStore;
use nala::ports::memory::MemoryStore;

/// A fresh, non-existent path per test — same pattern as
/// `adapters/metrics/csv_sink.rs`'s `temp_dir`, no external tempfile crate
/// needed since `FileMemoryStore` creates its parent directory itself.
fn temp_memory_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nala_memory_test_{}_{n}_{:?}/memory.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn a_store_over_a_missing_file_starts_empty() {
    let mut store = FileMemoryStore::new(temp_memory_path());

    assert!(store.facts().is_empty());
}

#[test]
fn remembering_a_fact_persists_it_for_a_new_store_over_the_same_path() {
    let path = temp_memory_path();

    let mut store = FileMemoryStore::new(path.clone());
    store
        .remember("nombre".to_string(), "Juan".to_string())
        .unwrap();

    let mut reopened = FileMemoryStore::new(path);

    assert_eq!(
        reopened.facts(),
        vec![("nombre".to_string(), "Juan".to_string())]
    );
}

#[test]
fn a_store_over_a_corrupt_file_starts_empty_instead_of_failing() {
    let path = temp_memory_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json at all").unwrap();

    let mut store = FileMemoryStore::new(path);

    assert!(store.facts().is_empty());
}

#[test]
fn remembering_an_existing_key_overwrites_it_on_disk() {
    let path = temp_memory_path();

    let mut store = FileMemoryStore::new(path.clone());
    store
        .remember("ciudad".to_string(), "Medellín".to_string())
        .unwrap();
    store
        .remember("ciudad".to_string(), "Bogotá".to_string())
        .unwrap();

    let mut reopened = FileMemoryStore::new(path);

    assert_eq!(
        reopened.facts(),
        vec![("ciudad".to_string(), "Bogotá".to_string())]
    );
}
