use dedup_core::{cid, DirMetadata, FileMetadata, StorageFormat, Store};
use tempfile::TempDir;

fn add_file(store: &Store, path: &str, data: &[u8]) -> FileMetadata {
    let file_cid = cid::compute_cid(data);
    let compressed_size = store.content.store(&file_cid, data).unwrap();
    let meta = FileMetadata {
        cid: file_cid.to_bytes(),
        original_size: data.len() as u64,
        compressed_size,
        modified: 12345,
        created: 12300,
        permissions: 0o640,
    };
    store
        .metadata
        .insert_file(path, &meta, &cid::cid_to_string(&file_cid))
        .unwrap();
    meta
}

#[test]
fn root_files_move_without_changing_content_metadata_or_duplicates_in_both_formats() {
    for format in [StorageFormat::Legacy, StorageFormat::Fastcdc] {
        let tmp = TempDir::new().unwrap();
        let store = Store::create(tmp.path(), format).unwrap();
        let meta = add_file(&store, "/first.txt", b"shared content");
        add_file(&store, "/copy.txt", b"shared content");
        let stored_bytes = store.compute_stats().unwrap().5;
        store.create_dir("/Documents").unwrap();
        store
            .move_entries(&["/first.txt".into(), "/copy.txt".into()], "/Documents")
            .unwrap();
        assert!(store.get_file("/first.txt").unwrap().is_none());
        assert_eq!(
            store.read_file("/Documents/first.txt").unwrap(),
            b"shared content"
        );
        assert_eq!(
            bincode::serialize(&store.get_file("/Documents/first.txt").unwrap().unwrap()).unwrap(),
            bincode::serialize(&meta).unwrap()
        );
        assert_eq!(
            store.find_duplicates("/Documents/first.txt").unwrap(),
            vec!["/Documents/copy.txt", "/Documents/first.txt"]
        );
        assert_eq!(store.compute_stats().unwrap().5, stored_bytes);
        drop(store);
        let reopened = Store::open_existing(tmp.path()).unwrap();
        assert_eq!(
            reopened.read_file("/Documents/copy.txt").unwrap(),
            b"shared content"
        );
        assert_eq!(reopened.list_dir("/").unwrap().len(), 1);
    }
}

#[test]
fn moving_directories_preserves_nested_and_empty_folders_and_prefix_siblings() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    store.create_dir("/source").unwrap();
    store.create_dir("/source/empty").unwrap();
    store.create_dir("/destination").unwrap();
    add_file(&store, "/source/nested/file.txt", b"file");
    add_file(&store, "/source-other.txt", b"sibling");
    store
        .move_entries(&["/source".into()], "/destination")
        .unwrap();
    assert!(store.list_dir("/source").unwrap().is_empty());
    assert_eq!(
        store
            .list_dir("/destination/source")
            .unwrap()
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        vec!["empty", "nested"]
    );
    assert_eq!(
        store
            .read_file("/destination/source/nested/file.txt")
            .unwrap(),
        b"file"
    );
    assert_eq!(store.read_file("/source-other.txt").unwrap(), b"sibling");
    store
        .move_entries(&["/destination/source/empty".into()], "/")
        .unwrap();
    assert!(store
        .list_dir("/")
        .unwrap()
        .iter()
        .any(|e| e.name == "empty" && e.is_dir));
}

#[test]
fn implicit_directories_and_their_old_parents_remain_browsable() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    add_file(&store, "/implicit/file.txt", b"file");
    store.create_dir("/destination").unwrap();
    store
        .move_entries(&["/implicit/file.txt".into()], "/destination")
        .unwrap();
    assert!(store
        .list_dir("/")
        .unwrap()
        .iter()
        .any(|e| e.name == "implicit"));
    assert!(store.list_dir("/implicit").unwrap().is_empty());
    store
        .metadata
        .insert_dir(
            "/only-dirs/nested/empty",
            &DirMetadata {
                child_count: 0,
                modified: 42,
            },
        )
        .unwrap();
    assert!(store
        .list_dir("/")
        .unwrap()
        .iter()
        .any(|e| e.name == "only-dirs"));
    store
        .move_entries(&["/only-dirs".into()], "/destination")
        .unwrap();
    assert!(store
        .list_dir("/destination/only-dirs/nested")
        .unwrap()
        .iter()
        .any(|e| e.name == "empty"));
}

#[test]
fn conflicts_and_invalid_moves_leave_the_entire_selection_unchanged() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    add_file(&store, "/a.txt", b"a");
    add_file(&store, "/b.txt", b"b");
    store.create_dir("/dest").unwrap();
    add_file(&store, "/dest/b.txt", b"existing");
    assert!(store
        .move_entries(&["/a.txt".into(), "/b.txt".into()], "/dest")
        .is_err());
    assert_eq!(store.read_file("/a.txt").unwrap(), b"a");
    assert_eq!(store.read_file("/b.txt").unwrap(), b"b");
    assert_eq!(store.read_file("/dest/b.txt").unwrap(), b"existing");
    assert!(store.get_file("/dest/a.txt").unwrap().is_none());
    store.create_dir("/dest/nested").unwrap();
    for (sources, destination) in [
        (vec!["/"], "/dest"),
        (vec!["/dest"], "/dest/nested"),
        (vec!["/dest"], "/dest"),
        (vec!["/a.txt"], "/"),
        (vec!["/missing"], "/dest"),
        (vec!["/a.txt"], "/missing"),
        (vec!["/a.txt"], "/b.txt"),
        (vec!["/a.txt", "/a.txt"], "/dest"),
        (vec!["/dest", "/dest/nested"], "/"),
        (vec![], "/dest"),
    ] {
        assert!(store
            .move_entries(
                &sources.into_iter().map(String::from).collect::<Vec<_>>(),
                destination
            )
            .is_err());
    }
    for invalid in [
        "/",
        "relative",
        "/../escape",
        "/dest/./bad",
        "/dest//bad",
        "/dest/bad/",
        "/missing/folder",
        "/b.txt/folder",
        "/a.txt",
        "/dest",
    ] {
        assert!(store.create_dir(invalid).is_err(), "accepted {invalid}");
    }
    assert!(store.move_entries(&["/a.txt".into()], "/dest/..").is_err());
    assert!(store.move_entries(&["/./a.txt".into()], "/dest").is_err());
}

#[test]
fn errors_while_moving_roll_back_paths_and_duplicate_index_together() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    add_file(&store, "/good.txt", b"good");
    let mut bad = store.get_file("/good.txt").unwrap().unwrap();
    bad.cid = vec![];
    store
        .metadata
        .insert_file("/bad.txt", &bad, "invalid")
        .unwrap();
    store.create_dir("/dest").unwrap();
    assert!(store
        .move_entries(&["/good.txt".into(), "/bad.txt".into()], "/dest")
        .is_err());
    assert_eq!(store.read_file("/good.txt").unwrap(), b"good");
    assert_eq!(
        store.find_duplicates("/good.txt").unwrap(),
        vec!["/good.txt"]
    );
    assert!(store.list_dir("/dest").unwrap().is_empty());
    assert!(store.get_file("/bad.txt").unwrap().is_some());
}
