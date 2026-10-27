//! Versioned polling ABI. All IO, credential setup and runtime work stays on the
//! job worker. Opaque generation IDs avoid exposing Rust pointers to the host.
use crate::{ABI_VERSION, job::{Job, State}};
use std::{io::Read, path::{Component, PathBuf}, sync::Mutex, time::Duration};

#[repr(C)]
pub struct JobConfig {
    pub root: *const u8, pub root_len: usize,
    pub games: *const u8, pub games_len: usize,
    pub trust: *const u8, pub trust_len: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JobSnapshot {
    pub abi_version: u32, pub struct_size: u32, pub state: u32, pub flags: u32,
    pub completed_bytes: u64, pub total_bytes: u64, pub bytes_per_second: u64, pub sequence: u64,
    pub phase: [u8;32], pub error: [u8;32],
}
struct Registry { generation: u64, job: Option<(u64, Job)> }
static REGISTRY: Mutex<Registry> = Mutex::new(Registry { generation: 0, job: None });
fn registry() -> std::sync::MutexGuard<'static, Registry> { REGISTRY.lock().unwrap_or_else(|p| p.into_inner()) }
unsafe fn path(bytes: *const u8, len: usize) -> Result<PathBuf, ()> {
    if bytes.is_null() || len == 0 || len > 4096 { return Err(()); }
    let bytes = unsafe { std::slice::from_raw_parts(bytes,len) };
    if bytes.contains(&0) { return Err(()); }
    let path = PathBuf::from(std::str::from_utf8(bytes).map_err(|_| ())?);
    if !path.is_absolute() || path.parent().is_none() || path.components().any(|c| matches!(c,Component::ParentDir)) { return Err(()); }
    Ok(path)
}
fn copy_text(out: &mut [u8;32], value: &str) {
    // All strings originate in engine enums/static diagnostic codes, not remote
    // responses. Refuse unexpected long values instead of truncating UTF-8.
    out.fill(0);
    if value.len() < out.len() && value.is_ascii() { out[..value.len()].copy_from_slice(value.as_bytes()); }
}

/// # Safety
/// Config and handle must be valid aligned pointers. Each byte slice must be
/// readable for its specified length. Paths are copied before this returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xodus_job_start(config: *const JobConfig, config_size: usize, abi: u32, handle: *mut u64) -> i32 {
    if config.is_null() || handle.is_null() || config_size != std::mem::size_of::<JobConfig>() || abi != ABI_VERSION { return -1; }
    let config = unsafe { &*config };
    let paths = unsafe { (path(config.root,config.root_len), path(config.games,config.games_len), path(config.trust,config.trust_len)) };
    let (Ok(root), Ok(games), Ok(trust)) = paths else { return -1; };
    if root == games { return -1; }
    let mut registry = registry();
    if registry.job.is_some() { return -3; }
    let Some(id) = registry.generation.checked_add(1) else { return -3; };
    let keyring = root.join("xodus-keyring.ron");
    let job = Job::start(root,games,move || {
        let mut pem = Vec::new();
        std::fs::File::open(trust).map_err(|_| "trust-open")?.take(2*1024*1024+1).read_to_end(&mut pem).map_err(|_| "trust-read")?;
        xodus_cli::configure_embedded_trust(&pem)?;
        // Storage roots and the pre-existing credential store are provisioned by
        // Playden. Refuse a missing store: never create a new anonymous session.
        let metadata = std::fs::symlink_metadata(&keyring).map_err(|_| "sign-in-required")?;
        if !metadata.file_type().is_file() { return Err("credential-file"); }
        xodus_cli::init_file_secrets(keyring.to_str().ok_or("credential-path")?).map_err(|_| "credential-store")?;
        let client = xodus_cli::http_client_builder()?.redirect(xodus_cli::RedirectPolicy::none())
            .connect_timeout(Duration::from_secs(20)).read_timeout(Duration::from_secs(30))
            .build().map_err(|_| "http-client")?;
        Ok((client, xodus_cli::TokenManager::with_keychain_and_memory()))
    });
    let Ok(job) = job else { return -4; };
    registry.generation = id;
    registry.job = Some((id,job));
    unsafe { handle.write(id); }
    0
}

/// # Safety
/// Output must be an aligned writable JobSnapshot, valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xodus_job_poll(handle: u64, output: *mut JobSnapshot, size: usize, abi: u32) -> i32 {
    if output.is_null() || size != std::mem::size_of::<JobSnapshot>() || abi != ABI_VERSION { return -1; }
    let registry = registry();
    let Some((id,job)) = &registry.job else { return -2; };
    if *id != handle { return -2; }
    let snapshot = job.snapshot();
    let mut out = JobSnapshot { abi_version: ABI_VERSION, struct_size: size as u32,
        state: match snapshot.state { State::Starting=>1,State::Running=>2,State::Stopping=>3,State::Complete=>4,State::Failed=>5,State::Cancelled=>6 },
        ..Default::default() };
    copy_text(&mut out.phase,&snapshot.phase);
    if let Some(error) = snapshot.error { copy_text(&mut out.error,error); }
    if let Some(progress) = snapshot.progress {
        out.completed_bytes = progress.completed_bytes;
        out.bytes_per_second = progress.bytes_per_second;
        out.sequence = progress.sequence;
        if let Some(total) = progress.total_bytes { out.flags |= 1; out.total_bytes = total; }
        if progress.paused { out.flags |= 2; }
        copy_text(&mut out.phase,progress.phase);
    }
    unsafe { output.write(out); }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn xodus_job_cancel(handle: u64) -> i32 {
    let registry = registry();
    match &registry.job { Some((id,job)) if *id == handle => { job.cancel(); 0 }, _ => -2 }
}
#[unsafe(no_mangle)]
pub extern "C" fn xodus_job_release(handle: u64) -> i32 {
    let mut registry = registry();
    match &registry.job {
        Some((id,job)) if *id == handle => { if !job.snapshot().state.terminal() { return -3; } },
        _ => return -2,
    }
    registry.job = None;
    0
}
