//! Owner-only files, written whole: `pk_cli_drive::private_file` writes a
//! temp file beside the target, created exclusively (`O_EXCL`) with mode
//! `0600`, syncs it to disk, and renames it over the target. A reader never
//! sees half a file, even after a crash; nobody else on the machine can
//! read it; a file or symbolic link planted at the target never receives
//! the bytes.
//!
//! `records export -o` streams through `pk_med_records::create_export_file`
//! instead, which keeps the last complete export until a new one finishes.

use std::path::Path;

use pk_cli_core::CliError;
use pk_cli_drive::private_file::{self, Replace};

/// Write `bytes` to `path` as described above, creating its directory
/// owner-only when missing.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        create_private_dir(dir)?;
    }
    private_file::write(path, bytes, Replace::Always)
        .map_err(|e| CliError::Other(format!("writing {}: {e}", path.display())))
}

/// Most links [`resolve_links`] follows, as Linux's `MAXSYMLINKS`.
const MAX_LINKS: usize = 40;

/// The path a write to `path` reaches by following symbolic links: each
/// link's target in turn, a relative one taken from the link's directory,
/// until a path that is not a link. That path need not exist, so a link to
/// a file not yet created resolves to where the file goes. The same rule as
/// cli-common v0.33.1's `private_file::resolve_links`, which its
/// `ConfigStore::save` follows, so the config behaves alike before and
/// after mychart moves to it.
pub fn resolve_links(path: &Path) -> Result<std::path::PathBuf, CliError> {
    let failed = |e: std::io::Error| CliError::Other(format!("resolving {}: {e}", path.display()));
    let mut current = path.to_path_buf();
    for _ in 0..=MAX_LINKS {
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = std::fs::read_link(&current).map_err(failed)?;
                current = match current.parent().filter(|d| !d.as_os_str().is_empty()) {
                    Some(dir) if target.is_relative() => dir.join(target),
                    _ => target,
                };
            }
            Ok(_) => return Ok(current),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(current),
            Err(e) => return Err(failed(e)),
        }
    }
    Err(CliError::Other(format!(
        "resolving {}: more than {MAX_LINKS} symbolic links",
        path.display()
    )))
}

/// Create `dir` (and parents) owner-only.
pub fn create_private_dir(dir: &Path) -> Result<(), CliError> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)
        .map_err(|e| CliError::Other(format!("creating {}: {e}", dir.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_written_whole_and_owner_only_with_no_temp_left() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("records.ndjson");
        write_private(&path, b"one").unwrap();
        write_private(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["records.ndjson"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().unwrap()), 0o700);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_at_the_target_is_replaced_not_written_through() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere.txt");
        std::fs::write(&elsewhere, b"untouched").unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o644)).unwrap();
        let path = dir.path().join("note.html");
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        write_private(&path, b"<html></html>").unwrap();
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"untouched");
        let meta = std::fs::symlink_metadata(&path).unwrap();
        assert!(meta.file_type().is_file());
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn links_resolve_to_their_final_target_which_need_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.json");
        assert_eq!(resolve_links(&plain).unwrap(), plain);
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::os::unix::fs::symlink("b", &a).unwrap();
        std::os::unix::fs::symlink(dir.path().join("sub/final.json"), &b).unwrap();
        assert_eq!(
            resolve_links(&a).unwrap(),
            dir.path().join("sub/final.json")
        );
        let x = dir.path().join("x");
        let y = dir.path().join("y");
        std::os::unix::fs::symlink("y", &x).unwrap();
        std::os::unix::fs::symlink("x", &y).unwrap();
        assert!(resolve_links(&x)
            .unwrap_err()
            .to_string()
            .contains("more than 40"));
    }
}
