//! Replacing a file so that a reader sees the old contents or the new ones, never a mix.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};

/// Replaces the file at `path` with `contents`. They are written to a new temporary file beside it
/// (`.<name>.<pid>.<n>.tmp`, created only if nothing is there), which gets the old file's
/// permissions, is synced, and is renamed over `path`; on Unix the directory is then synced too, so
/// the rename survives a crash. A reader sees the old file or the new one, and a crash leaves one of
/// them. On an error before the rename the temporary file is removed and the old file is left as it
/// was.
///
/// # Errors
/// Fails when the temporary file cannot be created, written or synced, cannot be renamed over
/// `path`, or the directory cannot be synced after the rename (the new file is then in place).
pub fn replace(path: &Path, contents: &[u8]) -> Result<()> {
    replace_with(path, contents, Mode::Kept)
}

/// Replaces the file at `path` with `contents` as [`replace`] does, and makes it executable
/// (`0o755` on Unix) before it is renamed into place, so that nothing ever runs a half-written file
/// or finds a file it cannot run.
///
/// # Errors
/// As [`replace`].
pub fn replace_executable(path: &Path, contents: &[u8]) -> Result<()> {
    replace_with(path, contents, Mode::Executable)
}

/// The permissions the new file gets.
#[derive(Clone, Copy)]
enum Mode {
    /// Those of the file it replaces, or the default for a new file.
    Kept,
    /// Executable by everyone, writable by the owner.
    Executable,
}

fn replace_with(path: &Path, contents: &[u8], mode: Mode) -> Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let failed = |error: io::Error| {
        Error::environment(format!(
            "cannot write {}: {error}; check that {} is writable and has free space, then run the command again",
            path.display(),
            directory.display()
        ))
    };
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

    let written = write_and_rename(&mut file, &temporary, path, contents, mode);
    drop(file);
    written.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        failed(error)
    })?;
    sync_directory(directory).map_err(failed)
}

fn write_and_rename(
    file: &mut File,
    temporary: &Path,
    path: &Path,
    contents: &[u8],
    mode: Mode,
) -> io::Result<()> {
    file.write_all(contents)?;
    match mode {
        Mode::Kept => match fs::metadata(path) {
            Ok(old) => file.set_permissions(old.permissions())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        },
        Mode::Executable => make_executable(file)?,
    }
    file.sync_all()?;
    fs::rename(temporary, path)
}

#[cfg(unix)]
fn make_executable(file: &File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_file: &File) -> io::Result<()> {
    Ok(())
}

/// Syncs `directory`, so that a rename in it is on disk.
#[cfg(unix)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
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
    fn a_replace_executable_makes_a_new_file_executable_before_it_appears() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("gate");
        replace_executable(&path, b"#!/bin/sh\n").expect("written");

        assert_eq!(fs::read_to_string(&path).expect("read"), "#!/bin/sh\n");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(entries(directory.path()), ["gate"]);
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
