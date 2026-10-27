//! Mirrors the SDK assertions in tools/probe_compat.c. Build failure is preferable
//! to silently passing an incompatible directory/event buffer across libc.
use std::mem::{offset_of, size_of};
const _: () = {
    assert!(size_of::<libc::stat>() == 120);
    assert!(size_of::<libc::dirent>() == 264);
    assert!(offset_of!(libc::dirent,d_fileno) == 0);
    assert!(offset_of!(libc::dirent,d_reclen) == 4);
    assert!(offset_of!(libc::dirent,d_type) == 6);
    assert!(offset_of!(libc::dirent,d_namlen) == 7);
    assert!(offset_of!(libc::dirent,d_name) == 8);
    assert!(size_of::<libc::kevent>() == 32);
    assert!(offset_of!(libc::kevent,ident) == 0);
    assert!(offset_of!(libc::kevent,filter) == 8);
    assert!(offset_of!(libc::kevent,flags) == 10);
    assert!(offset_of!(libc::kevent,fflags) == 12);
    assert!(offset_of!(libc::kevent,data) == 16);
    assert!(offset_of!(libc::kevent,udata) == 24);
};
