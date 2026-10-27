// Only the modules required by the console job engine. Interactive login,
// desktop launch commands and webview construction stay in the CLI binary.
#[path = "commands/durable_file.rs"]
pub(crate) mod durable_file;
#[path = "commands/profile.rs"]
pub(crate) mod profile;
#[path = "commands/session.rs"]
pub(crate) mod session;
#[path = "commands/subscription.rs"]
pub(crate) mod subscription;
#[path = "commands/library.rs"]
pub(crate) mod library;
#[path = "commands/streaming.rs"]
pub(crate) mod streaming;
#[path = "commands/prepare_ps5.rs"]
pub(crate) mod prepare_ps5;
#[path = "commands/ps5_job.rs"]
pub(crate) mod ps5_job;
