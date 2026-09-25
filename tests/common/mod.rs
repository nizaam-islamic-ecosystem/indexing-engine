//! Shared integration-test support for the `nizaam-indexing` repository tests.
//!
//! This module is test infrastructure only. It groups the reusable neutral
//! fixtures from [`helpers`] and exposes them to individual repository-level
//! integration/conformance test crates.
//!
//! Typical use from a test file:
//!
//! ```ignore
//! mod common;
//!
//! use common::{engine_registry, operation_context, test_engine};
//! ```
//!
//! The module intentionally contains no production Indexing logic and does not
//! hide lifecycle behavior that a test is expected to verify.

#[allow(dead_code)]
mod helpers;

pub use helpers::*;
