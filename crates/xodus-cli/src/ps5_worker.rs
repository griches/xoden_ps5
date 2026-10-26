//! Opt-in console worker diagnostics. No browser, child processes or Mac service.
use std::{fs::OpenOptions, io, os::windows::io::IntoRawHandle};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetStdHandle(which: u32, handle: *mut core::ffi::c_void) -> i32;
}

pub fn initialize() -> io::Result<()> {
    // The parent directory is provisioned privately as the Playden user.
    for (which, name) in [(-11i32, "worker-progress.log"), (-12, "worker-errors.log")] {
        let file = OpenOptions::new().create(true).truncate(true).write(true)
            .open(format!(r"Z:\data\playden\xbox\{name}"))?;
        let handle = file.into_raw_handle();
        // The process owns the handle until exit; Rust stdout/stderr use it.
        if unsafe { SetStdHandle(which as u32, handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
