//! Replacing a file so that a reader sees the old contents or the new ones, never a mix.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};

/// Replaces the file at `path` with `contents`. They are written to a new temporary file beside it
/// (`.<name>.<pid>.<n>.tmp`, created only if nothing is there), which gets the old file's
/// permissions, is synced, and is renamed over `path`: a reader sees the old file or the new one,
/// and a crash leaves one of them. On any error the temporary file is removed and the old file is
/// left as it was.
///
/// # Errors
/// Fails when the temporary file cannot be created, written or synced, or cannot be renamed over
/// `path`.
pub(crate) fn replace(path: &Path, contents: &[u8]) -> Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let failed =
        |error: io::Error| Error::environment(format!("cannot write {}: {error}", path.display()));
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());

    let (temporary, mut file) = loop {
        let candidate = directory.join(format!(
            ".{name}.{}.{}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        match File::options()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(failed(error)),
        }
    };

    let written = write_and_rename(&mut file, &temporary, path, contents);
    drop(file);
    written.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        failed(error)
    })
}

fn write_and_rename(
    file: &mut File,
    temporary: &Path,
    path: &Path,
    contents: &[u8],
) -> io::Result<()> {
    file.write_all(contents)?;
    match fs::metadata(path) {
        Ok(old) => file.set_permissions(old.permissions())?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    file.sync_all()?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names of the entries of `directory`, sorted.
    fn entries(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .expect("directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[cfg(unix)]
    #[test]
    fn a_replace_writes_the_new_contents_and_keeps_the_old_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("run.sh");
        fs::write(&path, "old\n").expect("old file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");

        replace(&path, b"new\n").expect("replaced");

        assert_eq!(fs::read_to_string(&path).expect("read"), "new\n");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(entries(directory.path()), ["run.sh"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_replace_leaves_the_old_file_and_no_temporary_file() {
        use std::os::unix::fs::PermissionsExt as _;

        // The rename fails after the temporary file was written: the target is a directory.
        let directory = tempfile::tempdir().expect("directory");
        let target = directory.path().join("generated");
        fs::create_dir(&target).expect("a directory in the way");
        fs::write(target.join("kept.txt"), "kept\n").expect("its file");
        let error = replace(&target, b"new\n").expect_err("a directory is not replaced");
        assert!(
            error
                .message()
                .starts_with(&format!("cannot write {}", target.display())),
            "{}",
            error.message()
        );
        assert_eq!(entries(directory.path()), ["generated"]);
        assert_eq!(
            fs::read_to_string(target.join("kept.txt")).expect("read"),
            "kept\n"
        );

        // The temporary file cannot be created: the directory is read-only.
        let path = directory.path().join("gate.lock");
        fs::write(&path, "old\n").expect("old file");
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o555))
            .expect("read-only");
        let failed = replace(&path, b"new\n");
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755))
            .expect("writable again");
        assert!(
            failed.is_err(),
            "a read-only directory takes no temporary file"
        );
        assert_eq!(fs::read_to_string(&path).expect("read"), "old\n");
        assert_eq!(entries(directory.path()), ["gate.lock", "generated"]);
    }
}
