use std::fs::write;

use memoria_authority::StoreLayout;
use memoria_types::MemoriaError;

#[test]
fn legacy_store_is_rejected_as_unsupported_store_format() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path().join("memory.sqlite"), b"legacy").unwrap();

    let error = StoreLayout::create(directory.path()).unwrap_err();

    assert!(matches!(error, MemoriaError::UnsupportedStoreFormat { .. }));
    assert_eq!(error.code(), "UNSUPPORTED_STORE_FORMAT");
}
