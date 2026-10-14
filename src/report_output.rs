//! Bounded, no-overwrite local report-file output.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_REPORT_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const TEMP_FILE_ATTEMPTS: usize = 32;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Writes a complete report to a new file without replacing an existing path.
/// The temporary file is created beside the destination, then linked into place
/// atomically so readers never observe a partial report.
pub fn write_report(path: &Path, contents: &str) -> io::Result<()> {
    if contents.len() > MAX_REPORT_OUTPUT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rendered report exceeds the 16 MiB output limit",
        ));
    }
    if path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output path needs a file name",
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    for _ in 0..TEMP_FILE_ATTEMPTS {
        let temp_path = parent.join(format!(
            ".netforensics-report-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        match create_private_temp(&temp_path) {
            Ok(mut file) => {
                let write_result = file
                    .write_all(contents.as_bytes())
                    .and_then(|()| file.sync_all());
                if let Err(error) = write_result {
                    drop(file);
                    let _ = fs::remove_file(&temp_path);
                    return Err(error);
                }
                drop(file);
                if let Err(error) = fs::hard_link(&temp_path, path) {
                    let _ = fs::remove_file(&temp_path);
                    return Err(error);
                }
                let _ = fs::remove_file(&temp_path);
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a temporary report file",
    ))
}

fn create_private_temp(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "netforensics-report-output-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn creates_a_new_complete_file_and_never_overwrites() {
        let root = temp_dir();
        let path = root.join("report.txt");
        write_report(&path, "sanitized report\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "sanitized report\n");
        assert_eq!(
            write_report(&path, "replacement").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "sanitized report\n");
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            1,
            "temporary report file should be removed"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn exported_report_has_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir();
        let path = root.join("report.json");
        write_report(&path, "{}\n").unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_replace_a_destination_symlink() {
        use std::os::unix::fs::symlink;
        let root = temp_dir();
        let target = root.join("existing.txt");
        let link = root.join("report.txt");
        fs::write(&target, "keep this file\n").unwrap();
        symlink(&target, &link).unwrap();
        assert_eq!(
            write_report(&link, "new report\n").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep this file\n");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_oversized_reports_without_creating_files() {
        let root = temp_dir();
        let path = root.join("report.txt");
        let oversized = "x".repeat(MAX_REPORT_OUTPUT_BYTES + 1);
        assert_eq!(
            write_report(&path, &oversized).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}
