//! Replacing the previous site with the new one, and refusing to replace a directory
//! hatlas did not write.

use crate::Error;
use std::path::{Path, PathBuf};

/// The file whose presence says a directory is a hatlas site.
pub const MARKER: &str = ".hatlas-site";

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io { path: path.to_path_buf(), source }
}

/// Fails with `NotASite` when `out` exists and is not a hatlas site.
pub fn guard(out: &Path) -> Result<(), Error> {
    if out.exists() && !out.join(MARKER).is_file() {
        return Err(Error::NotASite(out.to_path_buf()));
    }
    Ok(())
}

/// The sibling directory a site is written into before it replaces `out`.
pub fn tmp_dir(out: &Path) -> PathBuf {
    let name = out.file_name().map_or("hatlas".into(), |n| n.to_string_lossy().into_owned());
    out.with_file_name(format!("{name}.tmp-{}", std::process::id()))
}

/// Moves `tmp` to `out`, removing a previous site first. `NotASite` leaves `out` untouched.
pub fn swap(tmp: &Path, out: &Path) -> Result<(), Error> {
    guard(out)?;
    if out.exists() {
        std::fs::remove_dir_all(out).map_err(io(out))?;
    }
    std::fs::rename(tmp, out).map_err(io(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hatlas-swap-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // Pins that a directory without the marker is never deleted, because it may be someone's build cache.
    #[test]
    fn refuses_a_directory_without_the_marker() {
        let base = dir("refuse");
        let (tmp, out) = (base.join("new"), base.join("out"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("keep"), "x").unwrap();
        assert!(matches!(swap(&tmp, &out), Err(Error::NotASite(_))));
        assert!(out.join("keep").exists());
    }

    // Pins that a previous site is replaced wholesale, so stale pages do not survive a run.
    #[test]
    fn replaces_a_marked_site() {
        let base = dir("replace");
        let (tmp, out) = (base.join("new"), base.join("out"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join(MARKER), "").unwrap();
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join(MARKER), "").unwrap();
        std::fs::write(out.join("stale"), "x").unwrap();
        swap(&tmp, &out).unwrap();
        assert!(!out.join("stale").exists() && out.join(MARKER).exists());
    }
}
