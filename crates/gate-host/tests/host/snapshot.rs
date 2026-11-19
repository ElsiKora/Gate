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
