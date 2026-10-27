//! Publish a flushed temporary file using the host's rename/durability APIs.
use std::{io, path::Path};

#[cfg(not(windows))]
pub(super) fn publish(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)?;
    std::fs::File::open(destination.parent().unwrap_or(Path::new(".")))?.sync_all()
}

#[cfg(windows)]
pub(super) fn publish(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in file path"));
        }
        value.push(0);
        Ok(value)
    }
    let source = wide(source)?;
    let destination = wide(destination)?;
    // Windows cannot open a directory with File::open for a POSIX directory fsync.
    // Request write-through publication without enabling cross-volume copy/delete.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
