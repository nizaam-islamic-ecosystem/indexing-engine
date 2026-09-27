//! Logical index-version contracts.
//!
//! `IndexVersion` represents the logical version identity and compatibility
//! metadata associated with an Indexing index.
//!
//! Phase 2 deliberately keeps version identity separate from:
//! - source version,
//! - schema version,
//! - Core contract version.
//!
//! The logical `IndexVersion` value remains separate from Phase 3 lifecycle
//! state. Phase 3 adds a small lifecycle wrapper around the value without
//! turning the version itself into a runtime manager. Candidate construction,
//! rebuild orchestration, publication coordination, persistence, and
//! synchronization remain outside this value-level module.

use super::key::KeyMaterial;
use crate::error::IndexingResult;
use core::fmt;
use nizaam_core::contracts::Version as CoreVersion;
use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
};
use nizaam_core::status::Retryability;

/// Opaque logical identity of one Indexing index version.
///
/// This is intentionally distinct from [`SourceVersion`] and
/// [`SchemaVersion`]. It identifies logical index state rather than source
/// dataset state or structural schema compatibility.
///
/// The representation is deliberately an opaque validated string. Phase 2
/// does not prescribe how a later construction/versioning subsystem generates
/// version identifiers.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexVersionId(String);

impl IndexVersionId {
    /// Constructs a logical index-version identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, IndexVersionIdValidationError> {
        let value = value.into();
        validate_version_value(&value).map_err(IndexVersionIdValidationError::InvalidValue)?;
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the number of bytes in the identifier.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the identifier is empty.
    ///
    /// This is unreachable for successfully constructed values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<str> for IndexVersionId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for IndexVersionId {
    type Error = IndexVersionIdValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for IndexVersionId {
    type Error = IndexVersionIdValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Debug for IndexVersionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("IndexVersionId")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for IndexVersionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Opaque source-state version associated with an [`IndexVersion`].
///
/// This identifies source dataset/state information supplied by the source
/// engine. Indexing does not interpret its semantics.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SourceVersion(String);

impl SourceVersion {
    /// Constructs a source-version value.
    pub fn new(value: impl Into<String>) -> Result<Self, SourceVersionValidationError> {
        let value = value.into();
        validate_version_value(&value).map_err(SourceVersionValidationError::InvalidValue)?;
        Ok(Self(value))
    }

    /// Returns the source version as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the number of bytes in the source version.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the source version is empty.
    ///
    /// This is unreachable for successfully constructed values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<str> for SourceVersion {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for SourceVersion {
    type Error = SourceVersionValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for SourceVersion {
    type Error = SourceVersionValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Debug for SourceVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SourceVersion")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for SourceVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Opaque structural schema version associated with an [`IndexVersion`].
///
/// This identifies schema/structure compatibility information supplied by the
/// relevant source or contract owner. Indexing does not interpret the schema.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SchemaVersion(String);

impl SchemaVersion {
    /// Constructs a schema-version value.
    pub fn new(value: impl Into<String>) -> Result<Self, SchemaVersionValidationError> {
        let value = value.into();
        validate_version_value(&value).map_err(SchemaVersionValidationError::InvalidValue)?;
        Ok(Self(value))
    }

    /// Returns the schema version as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the number of bytes in the schema version.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the schema version is empty.
    ///
    /// This is unreachable for successfully constructed values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<str> for SchemaVersion {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for SchemaVersion {
    type Error = SchemaVersionValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for SchemaVersion {
    type Error = SchemaVersionValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Debug for SchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SchemaVersion")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Lifecycle of one logical Indexing version.
///
/// This lifecycle is intentionally narrower than the Core artifact lifecycle
/// and the Core engine lifecycle. It describes only the state of an Indexing
/// version while it is being constructed and published:
///
/// ```text
/// Building → Validating → Ready → Published
///     │            │          │
///     └────────────┴──────────┴→ Failed / Cancelled
/// ```
///
/// `Published` means the version has crossed the Indexing publication
/// boundary. Which published version is the current active version is owned by
/// the surrounding Indexing lifecycle manager, not by this value type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum VersionLifecycle {
    /// The candidate version is being constructed or populated.
    Building,

    /// The candidate version is undergoing logical validation.
    Validating,

    /// Validation completed successfully and the candidate is eligible for
    /// the publication boundary.
    Ready,

    /// The version has successfully crossed the publication boundary.
    ///
    /// Publication does not make this value the unique current active version;
    /// the lifecycle owner coordinates active ownership across candidates.
    Published,

    /// Construction or validation failed. The version remains unpublished.
    Failed,

    /// Construction, validation, or related work was cancelled. The version
    /// remains unpublished.
    Cancelled,
}

impl VersionLifecycle {
    /// Returns whether the lifecycle state represents an unpublished
    /// candidate, including terminal failed/cancelled candidates.
    #[must_use]
    pub const fn is_candidate(self) -> bool {
        matches!(
            self,
            Self::Building | Self::Validating | Self::Ready | Self::Failed | Self::Cancelled
        )
    }

    /// Returns whether the version has crossed the Indexing publication
    /// boundary.
    #[must_use]
    pub const fn is_published(self) -> bool {
        matches!(self, Self::Published)
    }

    /// Returns whether the lifecycle state is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Published | Self::Failed | Self::Cancelled)
    }

    /// Returns whether a direct transition between two lifecycle states is
    /// valid. Repeating the same state is a valid no-op.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Building, Self::Building)
                | (Self::Validating, Self::Validating)
                | (Self::Ready, Self::Ready)
                | (Self::Published, Self::Published)
                | (Self::Failed, Self::Failed)
                | (Self::Cancelled, Self::Cancelled)
                | (Self::Building, Self::Validating)
                | (Self::Validating, Self::Ready)
                | (Self::Building, Self::Failed)
                | (Self::Building, Self::Cancelled)
                | (Self::Validating, Self::Failed)
                | (Self::Validating, Self::Cancelled)
                | (Self::Ready, Self::Failed)
                | (Self::Ready, Self::Cancelled)
                | (Self::Ready, Self::Published)
        )
    }
}

/// One logical [`IndexVersion`] together with its Phase 3 lifecycle state.
///
/// This wrapper keeps the existing `IndexVersion` value contract intact while
/// adding the lifecycle meaning required by Phase 3. It does not own an
/// active-version registry, candidate collection, publication coordinator,
/// update journal, synchronization primitive, storage provider, or runtime
/// context. Those concerns remain outside this value-level model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexVersionState {
    version: IndexVersion,
    lifecycle: VersionLifecycle,
}

impl IndexVersionState {
    /// Wraps a logical version as a newly constructed candidate.
    #[must_use]
    pub fn new(version: IndexVersion) -> Self {
        Self {
            version,
            lifecycle: VersionLifecycle::Building,
        }
    }

    /// Returns the underlying logical index version.
    #[must_use]
    pub fn version(&self) -> &IndexVersion {
        &self.version
    }

    /// Returns the logical version identifier.
    #[must_use]
    pub fn id(&self) -> &IndexVersionId {
        self.version.id()
    }

    /// Returns the current Indexing lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> VersionLifecycle {
        self.lifecycle
    }

    /// Returns whether this version is still an unpublished candidate.
    #[must_use]
    pub const fn is_candidate(&self) -> bool {
        self.lifecycle.is_candidate()
    }

    /// Returns whether this version has crossed the publication boundary.
    #[must_use]
    pub const fn is_published(&self) -> bool {
        self.lifecycle.is_published()
    }

    /// Advances the version through one valid Phase 3 lifecycle transition.
    ///
    /// No transition from `Published`, `Failed`, or `Cancelled` to another
    /// state is permitted. In particular, a published version cannot be
    /// turned back into an unpublished candidate by this value model.
    pub fn transition_to(
        &mut self,
        next: VersionLifecycle,
    ) -> Result<(), VersionLifecycleTransitionError> {
        let current = self.lifecycle;
        if !current.can_transition_to(next) {
            return Err(VersionLifecycleTransitionError::new(
                self.id().clone(),
                current,
                next,
            ));
        }

        self.lifecycle = next;
        Ok(())
    }

    /// Advances the version to `Ready`.
    pub fn mark_ready(&mut self) -> Result<(), VersionLifecycleTransitionError> {
        self.transition_to(VersionLifecycle::Ready)
    }

    /// Advances the version to `Published`.
    pub fn mark_published(&mut self) -> Result<(), VersionLifecycleTransitionError> {
        self.transition_to(VersionLifecycle::Published)
    }

    /// Marks the version as failed while keeping it unpublished.
    pub fn mark_failed(&mut self) -> Result<(), VersionLifecycleTransitionError> {
        self.transition_to(VersionLifecycle::Failed)
    }

    /// Marks the version as cancelled while keeping it unpublished.
    pub fn mark_cancelled(&mut self) -> Result<(), VersionLifecycleTransitionError> {
        self.transition_to(VersionLifecycle::Cancelled)
    }

    /// Converts the state transition failure into the shared Core error
    /// contract.
    ///
    /// `IndexingResult` intentionally uses Core's `ErrorEvent` as the
    /// crate-wide error occurrence contract. `ErrorEvent` is a relatively
    /// large value, so this narrow lint allowance is preferable to changing
    /// the shared result alias or boxing the error only for this method.
    #[allow(clippy::result_large_err)]
    pub fn transition_to_result(
        &mut self,
        next: VersionLifecycle,
        context: ErrorContext,
    ) -> IndexingResult<()> {
        self.transition_to(next)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Consumes the lifecycle wrapper and returns the logical version and its
    /// lifecycle state.
    #[must_use]
    pub fn into_parts(self) -> (IndexVersion, VersionLifecycle) {
        (self.version, self.lifecycle)
    }

    /// Consumes the wrapper and returns only the logical version.
    #[must_use]
    pub fn into_version(self) -> IndexVersion {
        self.version
    }
}

/// Logical version identity and metadata associated with one index state.
///
/// `IndexVersion` deliberately does not model lifecycle state. Later phases
/// may use this contract when implementing candidate, active, rebuild, and
/// publication workflows, but those workflows do not belong here.
///
/// The optional metadata is generic [`KeyMaterial`] and is not interpreted by
/// this module.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexVersion {
    id: IndexVersionId,
    source_version: Option<SourceVersion>,
    schema_version: Option<SchemaVersion>,
    metadata: Option<KeyMaterial>,
}

impl IndexVersion {
    /// Constructs an index version without source/schema associations or
    /// additional metadata.
    pub fn new(id: IndexVersionId) -> Self {
        Self {
            id,
            source_version: None,
            schema_version: None,
            metadata: None,
        }
    }

    /// Constructs an index version with optional source/schema associations
    /// and generic logical metadata.
    pub fn with_metadata(
        id: IndexVersionId,
        source_version: Option<SourceVersion>,
        schema_version: Option<SchemaVersion>,
        metadata: Option<KeyMaterial>,
    ) -> Result<Self, IndexVersionValidationError> {
        if let Some(metadata) = &metadata {
            metadata
                .validate()
                .map_err(IndexVersionValidationError::InvalidMetadata)?;
        }

        Ok(Self {
            id,
            source_version,
            schema_version,
            metadata,
        })
    }

    /// Returns the logical index-version identity.
    #[must_use]
    pub fn id(&self) -> &IndexVersionId {
        &self.id
    }

    /// Returns the associated source-state version, if supplied.
    #[must_use]
    pub fn source_version(&self) -> Option<&SourceVersion> {
        self.source_version.as_ref()
    }

    /// Returns the associated schema version, if supplied.
    #[must_use]
    pub fn schema_version(&self) -> Option<&SchemaVersion> {
        self.schema_version.as_ref()
    }

    /// Returns optional generic logical version metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Consumes the version and returns all logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        IndexVersionId,
        Option<SourceVersion>,
        Option<SchemaVersion>,
        Option<KeyMaterial>,
    ) {
        (
            self.id,
            self.source_version,
            self.schema_version,
            self.metadata,
        )
    }

    /// Validates the logical version metadata.
    ///
    /// Validation is structural only. It does not determine whether this
    /// version is active, current, publishable, compatible with a provider,
    /// or a valid successor to another version.
    pub fn validate(&self) -> Result<(), IndexVersionValidationError> {
        if let Some(metadata) = &self.metadata {
            metadata
                .validate()
                .map_err(IndexVersionValidationError::InvalidMetadata)?;
        }

        Ok(())
    }
}

/// Validation failures for [`IndexVersionId`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexVersionIdValidationError {
    /// The identifier is empty or otherwise invalid.
    InvalidValue(VersionValueValidationError),
}

impl fmt::Display for IndexVersionIdValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue(error) => write!(formatter, "invalid index version ID: {error}"),
        }
    }
}

impl std::error::Error for IndexVersionIdValidationError {}

/// Validation failures for [`SourceVersion`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceVersionValidationError {
    /// The source version is empty or otherwise invalid.
    InvalidValue(VersionValueValidationError),
}

impl fmt::Display for SourceVersionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue(error) => write!(formatter, "invalid source version: {error}"),
        }
    }
}

impl std::error::Error for SourceVersionValidationError {}

/// Validation failures for [`SchemaVersion`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaVersionValidationError {
    /// The schema version is empty or otherwise invalid.
    InvalidValue(VersionValueValidationError),
}

impl fmt::Display for SchemaVersionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue(error) => write!(formatter, "invalid schema version: {error}"),
        }
    }
}

impl std::error::Error for SchemaVersionValidationError {}

/// Structural validation failures shared by the logical version values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionValueValidationError {
    /// The version value contains no bytes.
    Empty,

    /// The version value contains a Unicode control character.
    ControlCharacter {
        /// Byte position of the control character.
        index: usize,
    },
}

impl fmt::Display for VersionValueValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("version value must not be empty"),
            Self::ControlCharacter { index } => {
                write!(
                    formatter,
                    "version value contains a control character at byte index {index}"
                )
            }
        }
    }
}

fn validate_version_value(value: &str) -> Result<(), VersionValueValidationError> {
    if value.is_empty() {
        return Err(VersionValueValidationError::Empty);
    }

    for (index, character) in value.char_indices() {
        if character.is_control() {
            return Err(VersionValueValidationError::ControlCharacter { index });
        }
    }

    Ok(())
}

/// Error returned when an [`IndexVersionState`] is asked to perform an
/// illegal lifecycle transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionLifecycleTransitionError {
    version: IndexVersionId,
    from: VersionLifecycle,
    to: VersionLifecycle,
}

impl VersionLifecycleTransitionError {
    fn new(version: IndexVersionId, from: VersionLifecycle, to: VersionLifecycle) -> Self {
        Self { version, from, to }
    }

    /// Returns the logical version whose lifecycle transition was rejected.
    #[must_use]
    pub fn version(&self) -> &IndexVersionId {
        &self.version
    }

    /// Returns the lifecycle state before the rejected transition.
    #[must_use]
    pub const fn from(&self) -> VersionLifecycle {
        self.from
    }

    /// Returns the requested lifecycle state.
    #[must_use]
    pub const fn to(&self) -> VersionLifecycle {
        self.to
    }

    /// Converts this typed lifecycle failure into the shared Core error
    /// contract.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        let mut error = GlobalError {
            code: ErrorCode::new("INDEXING.VERSION.001")
                .expect("Indexing version error code is statically valid"),
            owner: ErrorOwner::new("INDEXING").expect("Indexing error owner is statically valid"),
            version: CoreVersion::new(1, 0, 0),
            class: ErrorClass::Contract,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message: format!(
                "invalid index version lifecycle transition for {}: {:?} -> {:?}",
                self.version, self.from, self.to
            ),
            details: Vec::new(),
            solution_reference: None,
            context,
            cause: None,
        };

        if let Some(detail) =
            nizaam_core::error::DiagnosticDetail::new("version", self.version.to_string())
        {
            error = error.with_detail(detail);
        }
        if let Some(detail) =
            nizaam_core::error::DiagnosticDetail::new("from", format!("{:?}", self.from))
        {
            error = error.with_detail(detail);
        }
        if let Some(detail) =
            nizaam_core::error::DiagnosticDetail::new("to", format!("{:?}", self.to))
        {
            error = error.with_detail(detail);
        }

        error
    }
}

impl fmt::Display for VersionLifecycleTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid index version lifecycle transition for {}: {:?} -> {:?}",
            self.version, self.from, self.to
        )
    }
}

impl std::error::Error for VersionLifecycleTransitionError {}

/// Validation failures for [`IndexVersion`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexVersionValidationError {
    /// Generic logical metadata is structurally invalid.
    InvalidMetadata(super::key::KeyMaterialValidationError),
}

impl fmt::Display for IndexVersionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid index version metadata: {error}")
            }
        }
    }
}

impl std::error::Error for IndexVersionValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test index version ID should be valid")
    }

    fn source_version(value: &str) -> SourceVersion {
        SourceVersion::new(value).expect("test source version should be valid")
    }

    fn schema_version(value: &str) -> SchemaVersion {
        SchemaVersion::new(value).expect("test schema version should be valid")
    }

    #[test]
    fn constructs_an_index_version() {
        let id = index_version_id("index-v1");
        let version = IndexVersion::new(id.clone());

        assert_eq!(version.id(), &id);
        assert_eq!(version.source_version(), None);
        assert_eq!(version.schema_version(), None);
        assert_eq!(version.metadata(), None);
    }

    #[test]
    fn keeps_index_source_and_schema_versions_distinct() {
        let index_id = index_version_id("index-v7");
        let source = source_version("source-2026-09-25");
        let schema = schema_version("schema-3");

        let version = IndexVersion::with_metadata(
            index_id.clone(),
            Some(source.clone()),
            Some(schema.clone()),
            None,
        )
        .expect("valid version should be constructed");

        assert_eq!(version.id(), &index_id);
        assert_eq!(version.source_version(), Some(&source));
        assert_eq!(version.schema_version(), Some(&schema));

        // The three concepts deliberately have different Rust types.
        // IndexVersionId != SourceVersion != SchemaVersion.
    }

    #[test]
    fn accepts_generic_logical_metadata() {
        let metadata = KeyMaterial::map([
            ("build-source", KeyMaterial::text("source-engine")),
            ("revision", KeyMaterial::Unsigned(4)),
        ])
        .expect("test metadata should be valid");

        let version = IndexVersion::with_metadata(
            index_version_id("index-v4"),
            Some(source_version("source-v4")),
            Some(schema_version("schema-v2")),
            Some(metadata.clone()),
        )
        .expect("valid metadata should be accepted");

        assert_eq!(version.metadata(), Some(&metadata));
    }

    #[test]
    fn rejects_invalid_metadata() {
        let metadata = KeyMaterial::text("invalid\nmetadata");

        let error =
            IndexVersion::with_metadata(index_version_id("index-v1"), None, None, Some(metadata))
                .expect_err("invalid metadata must be rejected");

        assert_eq!(
            error,
            IndexVersionValidationError::InvalidMetadata(
                super::super::key::KeyMaterialValidationError::TextControlCharacter { index: 7 }
            )
        );
    }

    #[test]
    fn rejects_empty_index_version_id() {
        let error = IndexVersionId::new("").expect_err("empty index version ID must be rejected");

        assert_eq!(
            error,
            IndexVersionIdValidationError::InvalidValue(VersionValueValidationError::Empty)
        );
    }

    #[test]
    fn rejects_control_characters_in_version_values() {
        let index_error =
            IndexVersionId::new("index\nv1").expect_err("control characters must be rejected");

        let source_error =
            SourceVersion::new("source\tv1").expect_err("control characters must be rejected");

        let schema_error =
            SchemaVersion::new("schema\r1").expect_err("control characters must be rejected");

        assert_eq!(
            index_error,
            IndexVersionIdValidationError::InvalidValue(
                VersionValueValidationError::ControlCharacter { index: 5 }
            )
        );
        assert_eq!(
            source_error,
            SourceVersionValidationError::InvalidValue(
                VersionValueValidationError::ControlCharacter { index: 6 }
            )
        );
        assert_eq!(
            schema_error,
            SchemaVersionValidationError::InvalidValue(
                VersionValueValidationError::ControlCharacter { index: 6 }
            )
        );
    }

    #[test]
    fn unicode_version_values_are_allowed() {
        let id = IndexVersionId::new("نسخة-١")
            .expect("Unicode should not be rejected merely for being Unicode");

        assert_eq!(id.as_str(), "نسخة-١");
    }

    #[test]
    fn ordering_is_deterministic() {
        let first = index_version_id("index-v1");
        let second = index_version_id("index-v2");

        assert!(first < second);
    }

    #[test]
    fn validation_is_repeatable() {
        let version = IndexVersion::with_metadata(
            index_version_id("index-v9"),
            Some(source_version("source-v9")),
            Some(schema_version("schema-v3")),
            Some(KeyMaterial::Unsigned(9)),
        )
        .expect("valid version should be constructed");

        version.validate().expect("version should be valid");
        version
            .validate()
            .expect("validation should be deterministic");
    }

    #[test]
    fn into_parts_preserves_all_components() {
        let id = index_version_id("index-v3");
        let source = source_version("source-v12");
        let schema = schema_version("schema-v5");
        let metadata = Some(KeyMaterial::Unsigned(3));

        let version = IndexVersion::with_metadata(
            id.clone(),
            Some(source.clone()),
            Some(schema.clone()),
            metadata.clone(),
        )
        .expect("valid version should be constructed");

        let (returned_id, returned_source, returned_schema, returned_metadata) =
            version.into_parts();

        assert_eq!(returned_id, id);
        assert_eq!(returned_source, Some(source));
        assert_eq!(returned_schema, Some(schema));
        assert_eq!(returned_metadata, metadata);
    }

    #[test]
    fn keeps_logical_index_version_separate_from_phase3_lifecycle() {
        let version = IndexVersion::new(index_version_id("candidate-v1"));
        assert_eq!(version.id().as_str(), "candidate-v1");

        let state = IndexVersionState::new(version.clone());
        assert_eq!(state.version(), &version);
        assert_eq!(state.lifecycle(), VersionLifecycle::Building);
        assert!(state.is_candidate());
        assert!(!state.is_published());
    }

    #[test]
    fn follows_the_declared_phase3_lifecycle() {
        let version = IndexVersion::new(index_version_id("index-v1"));
        let mut state = IndexVersionState::new(version);

        state
            .transition_to(VersionLifecycle::Validating)
            .expect("building should transition to validating");
        assert_eq!(state.lifecycle(), VersionLifecycle::Validating);

        state
            .mark_ready()
            .expect("validating should transition to ready");
        assert_eq!(state.lifecycle(), VersionLifecycle::Ready);

        state
            .mark_published()
            .expect("ready should transition to published");
        assert_eq!(state.lifecycle(), VersionLifecycle::Published);
        assert!(!state.is_candidate());
        assert!(state.is_published());
        assert!(state.lifecycle().is_terminal());
    }

    #[test]
    fn failure_and_cancellation_are_terminal_and_unpublished() {
        let failed_version = IndexVersion::new(index_version_id("failed-v1"));
        let mut failed = IndexVersionState::new(failed_version);

        failed
            .mark_failed()
            .expect("building should be allowed to fail");
        assert_eq!(failed.lifecycle(), VersionLifecycle::Failed);
        assert!(failed.is_candidate());
        assert!(!failed.is_published());
        assert!(failed.lifecycle().is_terminal());

        let cancelled_version = IndexVersion::new(index_version_id("cancelled-v1"));
        let mut cancelled = IndexVersionState::new(cancelled_version);

        cancelled
            .mark_cancelled()
            .expect("building should be allowed to cancel");
        assert_eq!(cancelled.lifecycle(), VersionLifecycle::Cancelled);
        assert!(cancelled.is_candidate());
        assert!(!cancelled.is_published());
        assert!(cancelled.lifecycle().is_terminal());
    }

    #[test]
    fn invalid_lifecycle_transition_is_rejected() {
        let version = IndexVersion::new(index_version_id("index-v2"));
        let mut state = IndexVersionState::new(version);

        let error = state
            .mark_published()
            .expect_err("building must not skip validation and ready");

        assert_eq!(error.version().as_str(), "index-v2");
        assert_eq!(error.from(), VersionLifecycle::Building);
        assert_eq!(error.to(), VersionLifecycle::Published);

        state.mark_cancelled().expect("building may be cancelled");
    }

    #[test]
    fn terminal_published_state_cannot_be_reversed() {
        let version = IndexVersion::new(index_version_id("index-v3"));
        let mut state = IndexVersionState::new(version);
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("building should transition to validating");
        state.mark_ready().expect("ready transition should succeed");
        state
            .mark_published()
            .expect("publish transition should succeed");

        let error = state
            .mark_ready()
            .expect_err("published state must not return to ready");

        assert_eq!(error.from(), VersionLifecycle::Published);
        assert_eq!(error.to(), VersionLifecycle::Ready);
    }

    #[test]
    fn lifecycle_error_can_be_adapted_to_core_error() {
        let version = IndexVersion::new(index_version_id("index-v4"));
        let mut state = IndexVersionState::new(version);

        let error = state
            .mark_published()
            .expect_err("invalid shortcut must produce a typed lifecycle error");

        let context = ErrorContext::new(nizaam_core::operation::OperationContext::new(
            nizaam_core::operation::Operation::new(
                nizaam_core::identity::OperationId::new("index-version-test-operation")
                    .expect("test operation ID should be valid"),
                nizaam_core::identity::CorrelationId::new("index-version-test-correlation")
                    .expect("test correlation ID should be valid"),
            ),
        ))
        .from_engine(
            nizaam_core::identity::EngineId::new("index-version-test-engine")
                .expect("test engine ID should be valid"),
        );

        let _global = error.into_global_error(context);
    }

    #[test]
    fn index_version_is_distinct_from_source_and_schema_versions() {
        let index = index_version_id("v1");
        let source = source_version("v1");
        let schema = schema_version("v1");

        let version = IndexVersion::with_metadata(
            index.clone(),
            Some(source.clone()),
            Some(schema.clone()),
            None,
        )
        .expect("valid version should be constructed");

        assert_eq!(version.id().as_str(), "v1");
        assert_eq!(
            version
                .source_version()
                .expect("source version exists")
                .as_str(),
            "v1"
        );
        assert_eq!(
            version
                .schema_version()
                .expect("schema version exists")
                .as_str(),
            "v1"
        );

        // Equal textual values do not collapse the concepts because their
        // types remain distinct.
        assert_ne!(format!("{:?}", index), format!("{:?}", source));
        assert_ne!(format!("{:?}", index), format!("{:?}", schema));
    }
}
