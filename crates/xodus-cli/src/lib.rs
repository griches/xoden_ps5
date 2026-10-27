//! Shared console job engine for embedding in a native host.
//!
//! This does not initialize a GUI, spawn the CLI executable, select a credential
//! store, or install a process-wide signal handler. The host supplies the HTTP
//! client, token manager and approved private roots. The job runner retains its
//! live subscription/entitlement checks and atomic prepared-game publication.
mod job_observer;
mod transport;
#[path = "embedded_commands.rs"]
mod commands;
mod license;
mod package;
mod package_plan;
#[cfg(windows)]
mod ps5_worker;

pub use commands::ps5_job::run as run_playden_job;
pub use reqwest::Client;
pub use xodus::tokens::TokenManager;
pub use xodus::secrets::init_file_secrets;

pub use job_observer::{DownloadProgress, JobObserver, Observer};
pub use commands::ps5_job::run_observed as run_playden_job_observed;

pub use commands::ps5_job::JobLease;

pub use transport::{configure as configure_embedded_trust, builder as http_client_builder};
pub use reqwest::redirect::Policy as RedirectPolicy;
