//! The object reader answers `tree`, `changed-paths`, `read-at` and `is-ancestor` as git does.

use gate_host::git::{Git, Objects};

use crate::fixture::Repo;

/// What `git diff --name-only --no-renames -z` lists between two commits.
fn git_diff(repo: &Repo, old: &str, new: &str) -> Vec<String> {
    repo.git(&["diff", "--name-only", "--no-renames", "-z", old, new])
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

fn chmod(repo: &Repo, path: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(repo.file(path), std::fs::Permissions::from_mode(mode))
        .expect("set permissions");
}

/// A history whose trees hold the orders git sorts specially: a file and a directory of the same
/// name, names around `/` in byte order, a mode change, a symbolic link, a type change both ways.
fn history() -> (Repo, Vec<String>) {
    let repo = Repo::new();
    let mut commits = Vec::new();
    repo.write("a", "1")
        .write("a.txt", "1")
        .write("a-b/x", "1")
        .write("b/c/d.md", "1")
        .write("b/c/e.md", "1")
        .write("tool.sh", "echo")
        .write("ünï.md", "1");
    commits.push(repo.commit("one"));

    repo.write("b/c/d.md", "2").write("a0", "new");
    chmod(&repo, "tool.sh", 0o755);
    commits.push(repo.commit("two"));

    // `a` becomes a directory, `a-b` a file.
    repo.remove("a");
    std::fs::remove_dir_all(repo.file("a-b")).expect("remove a-b");
    repo.write("a/inner.txt", "x").write("a-b", "file now");
    std::os::unix::fs::symlink("a.txt", repo.file("link")).expect("symlink");
    commits.push(repo.commit("three"));

    std::fs::remove_dir_all(repo.file("b")).expect("remove b");
    repo.write("b", "a file").write("z/deep/er/f.md", "1");
    commits.push(repo.commit("four"));
    commits.push(repo.commit("empty"));
    (repo, commits)
}

#[test]
fn changed_paths_list_what_git_diff_lists_in_its_order() {
    let (repo, commits) = history();
    let git = Git::discover(repo.path()).expect("git");
    let mut objects = Objects::new(&git).expect("reader");
    for old in &commits {
        for new in &commits {
            assert_eq!(
                objects.changed_paths(old, new).expect("changed paths"),
                git_diff(&repo, old, new),
                "{old}..{new}"
            );
        }
    }
    assert!(
        objects
            .changed_paths("no-such-commit", &commits[0])
            .is_err()
    );
}

#[test]
fn trees_files_and_ancestry_answer_as_git_does() {
    let (repo, commits) = history();
    // A side branch merged into main, and a branch that main never merges.
    repo.git(&["checkout", "-q", "-b", "side", &commits[1]]);
    repo.write("side.md", "s");
    let side = repo.commit("side");
    repo.git(&["checkout", "-q", "main"]);
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    let merge = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "-b", "other", &commits[0]]);
    repo.write("other.md", "o");
    let other = repo.commit("other");
    repo.git(&["checkout", "-q", "main"]);

    let git = Git::discover(repo.path()).expect("git");
    let mut objects = Objects::new(&git).expect("reader");
    let mut all = commits.clone();
    all.extend([side, merge.clone(), other]);
    for commit in &all {
        assert_eq!(
            objects.tree(commit).expect("tree"),
            Some(repo.git(&["rev-parse", &format!("{commit}^{{tree}}")]))
        );
    }
    assert_eq!(objects.tree("no-such-commit").expect("tree"), None);

    for ancestor in all
        .iter()
        .map(String::as_str)
        .chain(["HEAD", "main", "nothing"])
    {
        for descendant in all.iter().map(String::as_str).chain(["HEAD", "nothing"]) {
            let expected = std::process::Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(["merge-base", "--is-ancestor", ancestor, descendant])
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .status()
                .expect("run git")
                .success();
            // A name of no commit is no answer at all: a fetch may bring the commit.
            let missing = ancestor == "nothing" || descendant == "nothing";
            assert_eq!(
                objects
                    .is_ancestor(ancestor, descendant)
                    .expect("is-ancestor"),
                (!missing).then_some(expected),
                "{ancestor} of {descendant}"
            );
        }
    }

    assert_eq!(
        objects
            .read_at(&merge, "side.md")
            .expect("read")
            .map(|(_, content)| content),
        Some(b"s".to_vec())
    );
    assert_eq!(objects.read_at(&merge, "absent.md").expect("read"), None);
    assert!(
        objects.read_at(&merge, "a").is_err(),
        "a directory is not a file"
    );
}

#[test]
fn merges_through_one_process_answer_as_a_git_process_per_merge() {
    let repo = Repo::new();
    repo.write("f", "base\n");
    let base = repo.commit("base");
    repo.git(&["checkout", "-q", "-b", "side"]);
    repo.write("f", "side\n");
    let side = repo.commit("side");
    repo.git(&["checkout", "-q", "-b", "other", &base]);
    repo.write("g", "other\n");
    let other = repo.commit("other");
    repo.git(&["checkout", "-q", "-b", "main-line", &base]);
    repo.write("f", "main\n");
    let main = repo.commit("main");
    let blob = repo.git(&["rev-parse", "HEAD:f"]).trim().to_owned();

    let git = Git::discover(repo.path()).expect("git");
    let mut merges = git.merges().expect("merge-tree");
    // A conflict between two clean merges: each answer ends where the next begins.
    for (first, second) in [
        (&main, &other),
        (&main, &side),
        (&side, &other),
        (&main, &side),
    ] {
        let alone = git.merge_tree(first, second).expect("git merge-tree");
        assert_eq!(
            merges.merge(first, second).expect("batch"),
            alone,
            "{first} {second}"
        );
    }
    assert!(git.merge_tree(&main, &side).expect("conflict").is_none());
    assert!(git.merge_tree(&main, &other).expect("clean").is_some());

    // An id that names no commit fails both ways; the batch stops and says so.
    assert!(git.merge_tree(&main, &blob).is_err());
    assert!(merges.merge(&main, &blob).is_err());
    assert!(
        merges.merge(&main, &other).is_err(),
        "a stopped batch answers nothing"
    );
}
