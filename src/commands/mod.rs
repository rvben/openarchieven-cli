//! API command modules.
//!
//! Each submodule declares its clap `Args` struct, its schema metadata, and
//! a `run` function that issues the upstream request and produces a
//! `Renderable`.

pub mod archives;
pub mod births;
pub mod cache_cmd;
pub mod census;
pub mod deaths;
pub mod event_records;
pub mod marriages;
pub mod match_record;
pub mod search;
pub mod show;
pub mod stats;
pub mod transcripts;
pub mod weather;
pub mod yearsago;
