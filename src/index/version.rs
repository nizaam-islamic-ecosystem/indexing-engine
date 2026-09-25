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
//! This module establishes only the logical version model. It does not
//! implement candidate construction, rebuilds, publication, active-version
//! switching, persistence, or synchronization workflows.

use super::key::KeyMaterial;
use core::fmt;

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
    fn does_not_model_lifecycle_state() {
        let version = IndexVersion::new(index_version_id("candidate-v1"));

        // A version is metadata, not a state machine. There is intentionally
        // no active/candidate/ready/published state in this Phase 2 contract.
        assert_eq!(version.id().as_str(), "candidate-v1");
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
