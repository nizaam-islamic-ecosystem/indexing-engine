//! Indexing diagnostics integration with the Nizaam Core observability system.
//!
//! Phase 6 does not implement a second diagnostic or error framework for the
//! Indexing Engine. Core remains authoritative for diagnostic construction,
//! diagnostic classification, bounded structured details, correlation,
//! subjects, and shared error references.
//!
//! This module provides only the Indexing-facing construction boundary needed
//! to attach Indexing-specific operational information to Core diagnostics.
//!
//! The intended boundary is:
//!
//! ```text
//! Indexing failure / operational condition
//!                  │
//!                  ▼
//!        IndexingDiagnostics
//!                  │
//!                  ▼
//!          Core Diagnostic
//!                  │
//!        ┌─────────┼─────────┐
//!        ▼         ▼         ▼
//!     subject    details   correlation
//!        │         │
//!        │         └── Indexing-specific context
//!        │
//!        └── Core subject model
//! ```
//!
//! Diagnostics remain observations. They do not execute recovery, decide
//! retries, change lifecycle state, evaluate health, or replace Core's
//! universal error infrastructure.
//!
//! Indexing-specific diagnostic information is stored only as bounded Core
//! diagnostic details. This module does not introduce an Indexing diagnostic
//! schema or persistent diagnostic store.

use nizaam_core::observability::{
    Diagnostic, DiagnosticCondition, DiagnosticDetails, DiagnosticError, DiagnosticKind,
    DiagnosticSubject,
};

use crate::identity::IndexId;
use crate::index::IndexVersion;
use crate::recovery::failure::FailureClass;

/// Stateless Indexing-facing facade over Core diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IndexingDiagnostics;

impl IndexingDiagnostics {
    /// Creates a stateless diagnostics facade.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Creates a Core diagnostic for an Indexing operational condition.
    ///
    /// Core remains responsible for validating the diagnostic message and
    /// constructing the diagnostic observation.
    pub fn create(
        &self,
        kind: DiagnosticKind,
        condition: DiagnosticCondition,
        message: impl Into<String>,
    ) -> Result<Diagnostic, DiagnosticError> {
        Diagnostic::new(kind, condition, message)
    }

    /// Adds an Indexing index identifier to Core diagnostic details.
    ///
    /// `IndexId` is intentionally opaque and does not expose a human-readable
    /// encoding. Its existing Debug representation is therefore used rather
    /// than inventing a new IndexId serialization format for diagnostics.
    pub fn with_index_id(
        &self,
        diagnostic: Diagnostic,
        index_id: &IndexId,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("index_id", format!("{index_id:?}"))
    }

    /// Adds an Indexing logical version identifier to Core diagnostic details.
    pub fn with_index_version(
        &self,
        diagnostic: Diagnostic,
        version: &IndexVersion,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("index_version", version.id().as_str())
    }

    /// Adds an Indexing failure classification to Core diagnostic details.
    ///
    /// This records the classification only. It does not perform recovery,
    /// authorize a retry, or replace Core's error infrastructure.
    pub fn with_failure_class(
        &self,
        diagnostic: Diagnostic,
        failure: FailureClass,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("failure_class", failure.as_str())
    }

    /// Adds a bounded operation identifier to Core diagnostic details.
    ///
    /// The caller supplies the already-established Core operation identity.
    /// This method does not create or interpret operation identifiers.
    pub fn with_operation_id(
        &self,
        diagnostic: Diagnostic,
        operation_id: impl Into<String>,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("operation_id", operation_id)
    }

    /// Adds a bounded attempt identifier to Core diagnostic details.
    ///
    /// The caller supplies the already-established Core attempt identity.
    /// This method does not create or interpret attempt identifiers.
    pub fn with_attempt_id(
        &self,
        diagnostic: Diagnostic,
        attempt_id: impl Into<String>,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("attempt_id", attempt_id)
    }

    /// Adds a provider-boundary observation to Core diagnostic details.
    ///
    /// The value identifies the logical provider boundary involved in the
    /// observation. This method does not select, instantiate, or inspect a
    /// physical provider implementation.
    pub fn with_provider(
        &self,
        diagnostic: Diagnostic,
        provider: impl Into<String>,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("provider", provider)
    }

    /// Adds the observed Indexing consistency state to Core diagnostic
    /// details.
    ///
    /// The state is recorded as supplied by the Indexing consistency layer.
    /// This method does not evaluate or change consistency policy.
    pub fn with_consistency_state(
        &self,
        diagnostic: Diagnostic,
        state: impl Into<String>,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("consistency_state", state)
    }

    /// Adds the observed Indexing lifecycle state to Core diagnostic details.
    ///
    /// The state is observational only. This method does not mutate the
    /// Indexing lifecycle or Core engine lifecycle.
    pub fn with_lifecycle_state(
        &self,
        diagnostic: Diagnostic,
        state: impl Into<String>,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail("lifecycle_state", state)
    }

    /// Adds one additional bounded diagnostic detail through Core.
    ///
    /// This is intentionally generic so future Indexing diagnostics can carry
    /// useful bounded context without requiring a new diagnostic subsystem.
    pub fn with_detail(
        &self,
        diagnostic: Diagnostic,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Diagnostic, DiagnosticError> {
        diagnostic.with_detail(key, value)
    }

    /// Attaches an existing Core diagnostic subject.
    ///
    /// Subject interpretation remains owned by Core.
    #[must_use]
    pub fn with_subject(&self, diagnostic: Diagnostic, subject: DiagnosticSubject) -> Diagnostic {
        diagnostic.with_subject(subject)
    }

    /// Returns the Core detail container associated with a diagnostic.
    ///
    /// This is an inspection convenience only. The diagnostic itself remains
    /// the authoritative immutable observation.
    #[must_use]
    pub fn details<'a>(&self, diagnostic: &'a Diagnostic) -> &'a DiagnosticDetails {
        diagnostic.details()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracer() -> IndexingDiagnostics {
        IndexingDiagnostics::new()
    }

    fn index_id() -> IndexId {
        IndexId::from_bytes([7_u8; 64])
    }

    fn index_version() -> IndexVersion {
        IndexVersion::new(crate::index::IndexVersionId::new("v1").unwrap())
    }

    fn diagnostic() -> Diagnostic {
        tracer()
            .create(
                DiagnosticKind::Operational,
                DiagnosticCondition::Degraded,
                "index requires attention",
            )
            .unwrap()
    }

    #[test]
    fn creates_a_stateless_diagnostics_facade() {
        let first = IndexingDiagnostics::new();
        let second = IndexingDiagnostics::new();

        assert_eq!(first, second);
    }

    #[test]
    fn creates_a_core_diagnostic() {
        let diagnostic = diagnostic();

        assert_eq!(diagnostic.kind(), DiagnosticKind::Operational);
        assert_eq!(diagnostic.condition(), DiagnosticCondition::Degraded);
        assert_eq!(diagnostic.message(), "index requires attention");
    }

    #[test]
    fn attaches_index_id_without_creating_an_index_id_format() {
        let diagnostic = tracer().with_index_id(diagnostic(), &index_id()).unwrap();

        let value = diagnostic
            .details()
            .get("index_id")
            .expect("index_id detail should exist");

        assert!(value.starts_with("IndexId("));
    }

    #[test]
    fn attaches_index_version_id() {
        let diagnostic = tracer()
            .with_index_version(diagnostic(), &index_version())
            .unwrap();

        assert_eq!(diagnostic.details().get("index_version"), Some("v1"));
    }

    #[test]
    fn attaches_failure_class_without_replacing_failure_classification() {
        let diagnostic = tracer()
            .with_failure_class(diagnostic(), FailureClass::ProviderFailure)
            .unwrap();

        assert_eq!(
            diagnostic.details().get("failure_class"),
            Some("provider_failure")
        );
    }

    #[test]
    fn attaches_operation_and_attempt_context() {
        let diagnostics = tracer();

        let diagnostic = diagnostics
            .with_operation_id(diagnostic(), "operation-1")
            .unwrap();

        let diagnostic = diagnostics
            .with_attempt_id(diagnostic, "attempt-1")
            .unwrap();

        assert_eq!(
            diagnostic.details().get("operation_id"),
            Some("operation-1")
        );
        assert_eq!(diagnostic.details().get("attempt_id"), Some("attempt-1"));
    }

    #[test]
    fn attaches_provider_and_operational_state() {
        let diagnostics = tracer();

        let diagnostic = diagnostics
            .with_provider(diagnostic(), "index-provider")
            .unwrap();

        let diagnostic = diagnostics
            .with_consistency_state(diagnostic, "current")
            .unwrap();

        let diagnostic = diagnostics
            .with_lifecycle_state(diagnostic, "published")
            .unwrap();

        assert_eq!(diagnostic.details().get("provider"), Some("index-provider"));
        assert_eq!(
            diagnostic.details().get("consistency_state"),
            Some("current")
        );
        assert_eq!(
            diagnostic.details().get("lifecycle_state"),
            Some("published")
        );
    }

    #[test]
    fn arbitrary_details_remain_core_bounded_details() {
        let diagnostic = tracer()
            .with_detail(diagnostic(), "operation", "index.rebuild")
            .unwrap();

        assert_eq!(diagnostic.details().get("operation"), Some("index.rebuild"));
    }

    #[test]
    fn core_rejects_empty_detail_values() {
        let result = tracer().with_detail(diagnostic(), "provider", "");

        assert_eq!(result, Err(DiagnosticError::InvalidDetail));
    }

    #[test]
    fn core_detail_limit_remains_authoritative() {
        let diagnostics = tracer();
        let mut diagnostic = diagnostic();

        let mut detail_index = 0;

        loop {
            let key = format!("detail-{detail_index}");
            match diagnostics.with_detail(diagnostic.clone(), key, "bounded") {
                Ok(updated) => {
                    diagnostic = updated;
                    detail_index += 1;
                }
                Err(DiagnosticError::DetailLimitExceeded) => break,
                Err(error) => panic!("unexpected Core diagnostic detail error: {error:?}"),
            }
        }

        assert_eq!(
            diagnostics.with_detail(diagnostic, "overflow", "rejected",),
            Err(DiagnosticError::DetailLimitExceeded)
        );
        assert!(detail_index > 0);
    }

    #[test]
    fn core_subject_remains_authoritative() {
        let diagnostic = tracer().with_subject(diagnostic(), DiagnosticSubject::Runtime);

        assert_eq!(diagnostic.subject(), Some(&DiagnosticSubject::Runtime));
    }

    #[test]
    fn index_id_is_not_added_as_raw_unbounded_user_text() {
        let diagnostic = tracer().with_index_id(diagnostic(), &index_id()).unwrap();

        let value = diagnostic.details().get("index_id").unwrap();

        assert!(!value.is_empty());
        assert!(value.len() < 512);
    }

    #[test]
    fn diagnostics_remain_independent_values() {
        let first = diagnostic();
        let second = tracer()
            .with_failure_class(diagnostic(), FailureClass::QueryFailure)
            .unwrap();

        assert!(first.details().is_empty());
        assert_eq!(second.details().get("failure_class"), Some("query_failure"));
    }
}
