//! Snapshots: tracked files only, from the working tree, the index or a commit, with git blob ids.

use gate_host::{Git, Snapshot, Source};

use crate::fixture::Repo;

fn open(repo: &Repo, source: Source) -> Snapshot {
    let git = Git::discover(repo.path()).expect("git");
    Snapshot::open(&git, source).expect("snapshot")
}

fn text(snapshot: &Snapshot, path: &str) -> Option<String> {
    snapshot.text(path).expect("read")
}

#[test]
fn only_tracked_paths_exist() {
    let repo = Repo::new();
    repo.write(".gitignore", "build/\n")
        .write("src/a.ts", "a\n")
        .write("src/b/c.ts", "c\n");
    repo.commit("initial");
    repo.write("build/out.js", "ignored\n")
        .write("new.ts", "untracked\n");

    let snapshot = open(&repo, Source::WorkTree);
    assert_eq!(
        snapshot.paths().collect::<Vec<_>>(),
        [".gitignore", "src/a.ts", "src/b/c.ts"]
    );
    assert_eq!(snapshot.under("src/b").collect::<Vec<_>>(), ["src/b/c.ts"]);
    assert_eq!(snapshot.under("src").count(), 2);
    assert!(text(&snapshot, "new.ts").is_none());
    assert!(text(&snapshot, "build/out.js").is_none());
}

#[test]
fn each_source_reads_its_own_contents() {
    let repo = Repo::new();
    repo.write("a.txt", "committed\n").write("gone.txt", "x\n");
    let commit = repo.commit("initial");
    repo.write("a.txt", "staged\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "working\n").remove("gone.txt");

    let tree = open(&repo, Source::WorkTree);
    let index = open(&repo, Source::Index);
    let head = open(&repo, Source::Commit("HEAD".to_owned()));

    assert_eq!(text(&tree, "a.txt").as_deref(), Some("working\n"));
    assert_eq!(text(&index, "a.txt").as_deref(), Some("staged\n"));
    assert_eq!(text(&head, "a.txt").as_deref(), Some("committed\n"));
    assert!(
        !tree.contains("gone.txt"),
        "a deleted file is not in the working tree"
    );
    assert!(index.contains("gone.txt"));
    assert_eq!(head.commit(), Some(commit.as_str()));
}

#[test]
fn each_source_answers_its_own_size_without_the_contents() {
    let repo = Repo::new();
    repo.write("a.txt", "committed\n").write("gone.txt", "x\n");
    repo.commit("initial");
    repo.write("a.txt", "staged!\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "the working copy\n")
        .remove("gone.txt")
        .write("new.txt", "untracked\n");

    let size = |snapshot: &Snapshot, path: &str| snapshot.size(path).expect("size");
    let tree = open(&repo, Source::WorkTree);
    let index = open(&repo, Source::Index);
    let head = open(&repo, Source::Commit("HEAD".to_owned()));

    assert_eq!(size(&tree, "a.txt"), Some(17));
    assert_eq!(size(&index, "a.txt"), Some(8));
    assert_eq!(size(&head, "a.txt"), Some(10));
    assert_eq!(size(&head, "gone.txt"), Some(2));
    assert_eq!(size(&tree, "gone.txt"), None, "deleted in the working tree");
    assert_eq!(size(&tree, "new.txt"), None, "untracked");
    assert_eq!(size(&head, "new.txt"), None);
    assert_eq!(
        text(&head, "a.txt").map(|text| text.len()),
        size(&head, "a.txt").and_then(|size| usize::try_from(size).ok()),
        "a size read after the contents is theirs"
    );
}

#[test]
fn blob_ids_are_the_ids_git_add_would_give() {
    let repo = Repo::new();
    repo.write("a.txt", "one\n").write("b.txt", "same\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink("a.txt", repo.file("link")).expect("symlink");
    repo.commit("initial");

    repo.write("a.txt", "two\n");
    #[cfg(unix)]
    {
        std::fs::remove_file(repo.file("link")).expect("remove link");
        std::os::unix::fs::symlink("b.txt", repo.file("link")).expect("symlink");
    }

    let tree = open(&repo, Source::WorkTree);
    repo.add();
    let index = open(&repo, Source::Index);

    for path in index.paths() {
        assert_eq!(tree.blob_id(path), index.blob_id(path), "{path}");
    }
    assert_eq!(
        tree.blob_id("a.txt").map(ToString::to_string),
        Some(repo.git(&["hash-object", "a.txt"]))
    );
    #[cfg(unix)]
    assert_eq!(text(&tree, "link").as_deref(), Some("b.txt"));
}

#[test]
fn an_unknown_revision_is_an_environment_error() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.commit("initial");
    let git = Git::discover(repo.path()).expect("git");
    let error = Snapshot::open(&git, Source::Commit("nowhere".to_owned())).expect_err("unknown");
    assert_eq!(error.kind(), gate_host::ErrorKind::Environment);
    assert!(
        error.message().contains("--rev nowhere names no commit"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn a_path_behind_a_symlinked_directory_is_not_read() {
    let repo = Repo::new();
    repo.write("src/b/c.ts", "inside\n")
        .write("src/a.ts", "a\n");
    repo.commit("initial");

    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("c.ts"), "outside\n").expect("write");
    std::fs::remove_dir_all(repo.path().join("src/b")).expect("remove");
    std::os::unix::fs::symlink(outside.path(), repo.path().join("src/b")).expect("symlink");

    let snapshot = open(&repo, Source::WorkTree);
    assert!(
        text(&snapshot, "src/b/c.ts").is_none(),
        "a file reached through a symlinked directory is outside the tracked tree"
    );
    assert_eq!(text(&snapshot, "src/a.ts").as_deref(), Some("a\n"));
}

#[test]
fn every_file_behind_a_symlinked_directory_is_refused_after_a_sibling_was_read() {
    let repo = Repo::new();
    repo.write("src/b/c.ts", "inside c\n")
        .write("src/b/d.ts", "inside d\n")
        .write("src/a.ts", "a\n");
    repo.commit("initial");
    let snapshot = open(&repo, Source::WorkTree);
    // `src` is asked and found a directory before `src/b` becomes a link.
    assert_eq!(text(&snapshot, "src/a.ts").as_deref(), Some("a\n"));

    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("c.ts"), "outside c\n").expect("write");
    std::fs::write(outside.path().join("d.ts"), "outside d\n").expect("write");
    std::fs::remove_dir_all(repo.path().join("src/b")).expect("remove");
    std::os::unix::fs::symlink(outside.path(), repo.path().join("src/b")).expect("symlink");

    for path in ["src/b/c.ts", "src/b/d.ts"] {
        let error = snapshot.text(path).expect_err(path).to_string();
        assert!(error.contains("symbolic link"), "{path}: {error}");
    }
}

#[test]
fn a_touched_or_modified_filtered_file_has_the_size_of_the_blob_git_would_store() {
    let repo = Repo::new();
    // A clean filter that stores a pointer naming the size, as Git LFS stores a pointer.
    repo.git(&[
        "config",
        "filter.pointer.clean",
        "printf 'pointer %s\\n' \"$(wc -c | tr -d ' ')\"",
    ]);
    repo.git(&["config", "filter.pointer.smudge", "cat"]);
    repo.git(&["config", "filter.pointer.required", "true"]);
    repo.write(".gitattributes", "*.bin filter=pointer\n")
        .write("media.bin", &"x".repeat(3000));
    repo.commit("a filtered file");
    let size = |repo: &Repo| {
        open(repo, Source::WorkTree)
            .size("media.bin")
            .expect("size")
    };
    let stored = u64::try_from("pointer 3000\n".len()).expect("small");
    assert_eq!(size(&repo), Some(stored), "unchanged since the commit");

    // Touched: the same bytes with another modification time, as cp -r or a cache restore leaves.
    let file = std::fs::File::options()
        .write(true)
        .open(repo.file("media.bin"))
        .expect("open");
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(120))
        .expect("set the modification time");
    drop(file);
    assert_eq!(
        repo.git(&["diff-files", "--name-only"]),
        "media.bin",
        "git sees the touched file as changed"
    );
    assert_eq!(size(&repo), Some(stored), "touched");

    // Modified: a pointer the object database does not hold yet, hashed without writing to it.
    repo.write("media.bin", &"y".repeat(5000));
    let objects = repo.git(&["count-objects", "-v"]);
    assert_eq!(
        size(&repo),
        Some(u64::try_from("pointer 5000\n".len()).expect("small")),
        "modified"
    );
    assert_eq!(
        repo.git(&["count-objects", "-v"]),
        objects,
        "the repository's object database is unchanged"
    );
}
