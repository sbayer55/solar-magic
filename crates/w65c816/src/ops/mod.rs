//! Instruction implementations, grouped by family. Each function has the
//! shape required by the dispatch table, so most always return `Ok(())`.

#[allow(clippy::unnecessary_wraps, reason = "dispatch table requires a uniform fallible signature")]
pub(crate) mod alu;
#[allow(clippy::unnecessary_wraps, reason = "dispatch table requires a uniform fallible signature")]
pub(crate) mod branch;
#[allow(clippy::unnecessary_wraps, reason = "dispatch table requires a uniform fallible signature")]
pub(crate) mod load_store;
#[allow(clippy::unnecessary_wraps, reason = "dispatch table requires a uniform fallible signature")]
pub(crate) mod misc;
#[allow(clippy::unnecessary_wraps, reason = "dispatch table requires a uniform fallible signature")]
pub(crate) mod stack;
