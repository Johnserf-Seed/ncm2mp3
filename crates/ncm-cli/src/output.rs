//! Output files that never write through a symlink and that two workers
//! can't claim at once.
//!
//! `File::create` and `fs::write` follow a symlink at the destination and
//! truncate whatever it points to, and an `exists()` check made first can't
//! stop a link, or a parallel worker's file, from appearing in between.
//! Someone who can write to the input or output directory could use either
//! to make us overwrite a file of the user's, or two inputs that map to one
//! output name could corrupt each other. So every file we write goes through
//! [`OutputFile`]:
//!
//! - [`OutputFile::create_new`] opens with `O_CREAT | O_EXCL` (`CREATE_NEW`
//!   on Windows), which fails if anything at all is at the path, a dangling
//!   symlink included. The check and the creation are one step, so of
//!   several workers after the same name, exactly one gets it.
//! - [`OutputFile::replace`] writes a new file next to the destination and
//!   renames it over the destination on [`OutputFile::commit`]. A rename
//!   replaces the directory entry itself, so a symlink there is swapped out
//!   rather than followed, and the old file stays intact until the new one
//!   is complete.
//!
//! An `OutputFile` dropped without `commit` deletes what it wrote, so a
//! failed conversion leaves no truncated file for a later run's default
//! skip policy to mistake for finished output.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct OutputFile {
    /// Open until `commit` (or drop) closes it.
    file: Option<File>,
    /// Where the bytes go until `commit`: the destination itself for
    /// `create_new`, a temporary file next to it for `replace`.
    staging: PathBuf,
    target: PathBuf,
    committed: bool,
}

impl OutputFile {
    /// Create `path` if nothing exists there yet. `Ok(None)` if something
    /// does, whether a file, a directory or a symlink (even a dangling one).
    pub(crate) fn create_new(path: &Path) -> io::Result<Option<Self>> {
        match open_new(path) {
            Ok(file) => Ok(Some(Self::new(file, path.to_path_buf(), path))),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Start a file that replaces whatever is at `path` once committed.
    pub(crate) fn replace(path: &Path) -> io::Result<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);

        // Same directory as the target, so the final rename can't cross
        // filesystems.
        let dir = match path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        let pid = std::process::id();
        let mut retries = 0;
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let staging = dir.join(format!(".ncm2mp3-{pid}-{id}.tmp"));
            match open_new(&staging) {
                Ok(file) => return Ok(Self::new(file, staging, path)),
                // Left behind by a crashed run that had the same pid.
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && retries < 100 => {
                    retries += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn new(file: File, staging: PathBuf, target: &Path) -> Self {
        Self {
            file: Some(file),
            staging,
            target: target.to_path_buf(),
            committed: false,
        }
    }

    /// Where the file ends up once committed.
    pub(crate) fn path(&self) -> &Path {
        &self.target
    }

    /// The open file, readable and writable.
    pub(crate) fn file(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("an OutputFile stays open until commit")
    }

    /// Close the file and, for [`replace`](Self::replace), rename it over
    /// the destination.
    pub(crate) fn commit(mut self) -> io::Result<()> {
        // Close first: Windows can refuse to rename a file that's still open.
        drop(self.file.take());
        if self.staging != self.target {
            fs::rename(&self.staging, &self.target)?;
        }
        self.committed = true;
        Ok(())
    }

    /// Write all of `data`, then [`commit`](Self::commit).
    pub(crate) fn write_and_commit(mut self, data: &[u8]) -> io::Result<()> {
        self.file().write_all(data)?;
        self.commit()
    }
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        if !self.committed {
            drop(self.file.take());
            let _ = fs::remove_file(&self.staging);
        }
    }
}

/// Create and open `path`, failing if anything exists there. Readable too,
/// because the tagger reads the audio back through the same handle.
fn open_new(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(dir: &Path) -> usize {
        fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn create_new_writes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        let out = OutputFile::create_new(&path).unwrap().unwrap();
        assert_eq!(out.path(), path);
        out.write_and_commit(b"audio").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"audio");
    }

    #[test]
    fn create_new_leaves_an_existing_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        fs::write(&path, b"existing").unwrap();
        assert!(OutputFile::create_new(&path).unwrap().is_none());
        assert_eq!(fs::read(&path).unwrap(), b"existing");
    }

    #[test]
    fn a_name_can_only_be_claimed_once() {
        // Two workers heading for the same output: the second one must see
        // the first's file even before it's finished.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        let first = OutputFile::create_new(&path).unwrap();
        assert!(first.is_some());
        assert!(OutputFile::create_new(&path).unwrap().is_none());
    }

    #[test]
    fn create_new_dropped_without_commit_removes_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        let mut out = OutputFile::create_new(&path).unwrap().unwrap();
        out.file().write_all(b"half an mp").unwrap();
        drop(out);
        assert!(!path.exists());
    }

    #[test]
    fn replace_keeps_the_old_file_until_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.jpg");
        fs::write(&path, b"old").unwrap();

        let mut out = OutputFile::replace(&path).unwrap();
        out.file().write_all(b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"old");
        out.commit().unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(entries(dir.path()), 1, "no temporary file left behind");
    }

    #[test]
    fn replace_dropped_without_commit_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.jpg");
        fs::write(&path, b"old").unwrap();

        let mut out = OutputFile::replace(&path).unwrap();
        out.file().write_all(b"new").unwrap();
        drop(out);

        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(entries(dir.path()), 1, "temporary file removed");
    }

    #[test]
    fn replace_creates_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.png");
        OutputFile::replace(&path)
            .unwrap()
            .write_and_commit(b"png")
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"png");
    }

    #[cfg(unix)]
    mod symlinks {
        use super::*;
        use std::os::unix::fs::symlink;

        #[test]
        fn create_new_refuses_a_symlink() {
            let dir = tempfile::tempdir().unwrap();
            let victim = dir.path().join("victim");
            fs::write(&victim, b"precious").unwrap();
            let link = dir.path().join("song.mp3");
            symlink(&victim, &link).unwrap();

            assert!(OutputFile::create_new(&link).unwrap().is_none());
            assert_eq!(fs::read(&victim).unwrap(), b"precious");
        }

        #[test]
        fn create_new_refuses_a_dangling_symlink() {
            // `exists()` says false here, and `File::create` would create
            // the link's target.
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("planted");
            let link = dir.path().join("song.mp3");
            symlink(&target, &link).unwrap();

            assert!(OutputFile::create_new(&link).unwrap().is_none());
            assert!(!target.exists());
        }

        #[test]
        fn replace_swaps_out_a_symlink_instead_of_writing_through_it() {
            let dir = tempfile::tempdir().unwrap();
            let victim = dir.path().join("victim");
            fs::write(&victim, b"precious").unwrap();
            let link = dir.path().join("cover.jpg");
            symlink(&victim, &link).unwrap();

            OutputFile::replace(&link)
                .unwrap()
                .write_and_commit(b"cover")
                .unwrap();

            assert_eq!(fs::read(&victim).unwrap(), b"precious");
            assert!(fs::symlink_metadata(&link).unwrap().file_type().is_file());
            assert_eq!(fs::read(&link).unwrap(), b"cover");
        }
    }
}
