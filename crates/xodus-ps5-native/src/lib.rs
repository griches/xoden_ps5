//! Bounded C interface for the native Playden Xbox downloader.
//! This first boundary inspects a package header only; it performs no IO.
// The engine is opt-in while its PS5 IO/network/runtime adapters are audited.
// Reuse the same account/entitlement logic as the proven worker.
#[cfg(feature = "job-engine")]
pub use xodus_cli::{Client, TokenManager, init_file_secrets, run_playden_job, run_playden_job_observed, DownloadProgress, JobObserver, Observer};

use msixvc::models::xvd::XvdHeader;
use msixvc_common::parse::BinaryTryParse;

pub const ABI_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackageHeader {
    pub abi_version: u32,
    pub struct_size: u32,
    pub drive_bytes: u64,
    pub user_data_bytes: u64,
    pub content_id: [u8; 16],
    pub format_version: u32,
    pub volume_flags: u32,
}

fn inspect(bytes: &[u8]) -> Result<PackageHeader, ()> {
    if bytes.len() != XvdHeader::SIZE {
        return Err(());
    }
    // The upstream parser represents FILETIME as signed nanoseconds. Reject
    // values that would overflow its arithmetic, including in debug builds.
    let timestamp = i64::from_le_bytes(bytes[0x210..0x218].try_into().map_err(|_| ())?);
    timestamp
        .checked_sub(116_444_736_000_000_000)
        .and_then(|value| value.checked_mul(100))
        .ok_or(())?;
    // Upstream enum parsing narrows u32 to u8. Do not accept wrapped values.
    for offset in [0x280, 0x284] {
        let value = u32::from_le_bytes(bytes[offset..offset + 4].try_into().map_err(|_| ())?);
        if value > u8::MAX as u32 {
            return Err(());
        }
    }
    let header = XvdHeader::try_from_slice(bytes).map_err(|_| ())?;
    Ok(PackageHeader {
        abi_version: ABI_VERSION,
        struct_size: std::mem::size_of::<PackageHeader>() as u32,
        drive_bytes: header.drive_size.0,
        user_data_bytes: header.user_data_length.0,
        content_id: *header.vduid.as_bytes(),
        format_version: header.format_version,
        volume_flags: header.volume_flags.bits(),
    })
}

/// Inspect exactly one 4096-byte header without opening files or networking.
///
/// # Safety
/// `bytes` must point to `length` readable bytes, and `output` must point to an
/// aligned, writable PackageHeader. The two buffers must not overlap and must
/// remain valid for this call. Null pointers and incorrect lengths are rejected.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xodus_inspect_package_header(
    bytes: *const u8,
    length: usize,
    output: *mut PackageHeader,
    output_size: usize,
    abi_version: u32,
) -> i32 {
    if bytes.is_null()
        || output.is_null()
        || length != XvdHeader::SIZE
        || output_size != std::mem::size_of::<PackageHeader>()
        || abi_version != ABI_VERSION
    {
        return -1;
    }
    let input = unsafe { std::slice::from_raw_parts(bytes, length) };
    match inspect(input) {
        Ok(value) => {
            unsafe {
                output.write(value);
            }
            0
        }
        Err(()) => -2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut data = vec![0; 4096];
        data[0x200..0x208].copy_from_slice(b"msft-xvd");
        data[0x210..0x218].copy_from_slice(&116_444_736_000_000_000i64.to_le_bytes());
        data[0x218..0x220].copy_from_slice(&4096u64.to_le_bytes());
        data[0x28c..0x290].copy_from_slice(&8192u32.to_le_bytes());
        data
    }
    fn call(data: &[u8], out: &mut PackageHeader) -> i32 {
        unsafe {
            xodus_inspect_package_header(
                data.as_ptr(),
                data.len(),
                out,
                std::mem::size_of::<PackageHeader>(),
                ABI_VERSION,
            )
        }
    }
    #[test]
    fn parses_fixed_size_header() {
        assert_eq!(std::mem::size_of::<PackageHeader>(), 48);
        let mut out = PackageHeader::default();
        assert_eq!(call(&fixture(), &mut out), 0);
        assert_eq!(out.drive_bytes, 4096);
        assert_eq!(out.user_data_bytes, 8192);
        assert_eq!(out.abi_version, 1);
    }
    #[test]
    fn rejects_bad_headers_without_mutating_output() {
        for (offset, replacement) in [
            (0x200, vec![0]),
            (0x210, i64::MIN.to_le_bytes().to_vec()),
            (0x280, 256u32.to_le_bytes().to_vec()),
            (0x284, 255u32.to_le_bytes().to_vec()),
            (0x218, u64::MAX.to_le_bytes().to_vec()),
        ] {
            let mut data = fixture();
            data[offset..offset + replacement.len()].copy_from_slice(&replacement);
            let mut out = PackageHeader {
                drive_bytes: 123,
                ..Default::default()
            };
            let before = out;
            assert_eq!(call(&data, &mut out), -2);
            assert_eq!(out, before);
        }
    }
    #[test]
    fn rejects_bad_abi_lengths_and_nulls() {
        let data = fixture();
        let mut out = PackageHeader::default();
        assert_eq!(call(&data[..4095], &mut out), -1);
        unsafe {
            assert_eq!(
                xodus_inspect_package_header(std::ptr::null(), 4096, &mut out, 48, 1),
                -1
            );
            assert_eq!(
                xodus_inspect_package_header(data.as_ptr(), 4096, std::ptr::null_mut(), 48, 1),
                -1
            );
            assert_eq!(
                xodus_inspect_package_header(data.as_ptr(), 4096, &mut out, 47, 1),
                -1
            );
            assert_eq!(
                xodus_inspect_package_header(data.as_ptr(), 4096, &mut out, 48, 2),
                -1
            );
        }
    }
}

#[cfg(feature = "job-engine")]
pub mod job;

#[cfg(feature = "job-engine")]
mod job_ffi;

#[cfg(target_os = "freebsd")]
mod abi_checks;
