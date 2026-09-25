//! Source-to-Indexing logical requirements.
//!
//! `IndexRequirement` is the source-facing declaration of what a source
//! engine needs indexed. It is intentionally distinct from
//! [`crate::index::IndexDefinition`], which is the canonical logical
//! definition produced by Indexing after validation and normalization.
//!
//! This module contains no physical provider configuration, storage
//! configuration, query execution, build/rebuild workflow, publication
//! workflow, partitioning, sharding, embedding configuration, or domain
//! semantics.

use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
use crate::index::{
    ConsistencyRequirement, IndexDefinition, IndexFamily, KeyDefinition, SchemaVersion,
    SourceVersion, TargetReferenceType, Uniqueness,
};
use core::fmt;

/// A source engine's declaration of one logical indexing need.
///
/// The requirement is caller/source-facing. It describes the logical shape
/// that the source needs Indexing to represent, but it does not become the
/// canonical definition merely by being constructed.
///
/// A requirement deliberately does not contain an `IndexDefinitionId`.
/// Definition identity is supplied by Indexing at normalization time so that
/// the caller's declaration and Indexing's canonical definition remain
/// separate concepts.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexRequirement {
    namespace: IndexNamespace,
    family: IndexFamily,
    key_definition: KeyDefinition,
    target_reference_type: TargetReferenceType,
    uniqueness: Uniqueness,
    consistency_requirement: ConsistencyRequirement,
    source_version: Option<SourceVersion>,
    schema_version: Option<SchemaVersion>,
}

impl IndexRequirement {
    /// Constructs a validated logical indexing requirement.
    ///
    /// All component types are already strongly validated by their own
    /// constructors. This constructor additionally validates the complete
    /// requirement as a logical contract.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        namespace: IndexNamespace,
        family: IndexFamily,
        key_definition: KeyDefinition,
        target_reference_type: TargetReferenceType,
        uniqueness: Uniqueness,
        consistency_requirement: ConsistencyRequirement,
        source_version: Option<SourceVersion>,
        schema_version: Option<SchemaVersion>,
    ) -> Result<Self, IndexRequirementValidationError> {
        let requirement = Self {
            namespace,
            family,
            key_definition,
            target_reference_type,
            uniqueness,
            consistency_requirement,
            source_version,
            schema_version,
        };

        requirement.validate()?;
        Ok(requirement)
    }

    /// Returns the logical namespace requested by the source engine.
    #[must_use]
    pub fn namespace(&self) -> &IndexNamespace {
        &self.namespace
    }

    /// Returns the requested broad indexing family.
    #[must_use]
    pub fn family(&self) -> IndexFamily {
        self.family
    }

    /// Returns the generic logical key definition.
    #[must_use]
    pub fn key_definition(&self) -> &KeyDefinition {
        &self.key_definition
    }

    /// Returns the opaque logical target-reference type.
    #[must_use]
    pub fn target_reference_type(&self) -> &TargetReferenceType {
        &self.target_reference_type
    }

    /// Returns the requested logical uniqueness behavior.
    #[must_use]
    pub fn uniqueness(&self) -> Uniqueness {
        self.uniqueness
    }

    /// Returns the requested logical consistency requirement.
    #[must_use]
    pub fn consistency_requirement(&self) -> &ConsistencyRequirement {
        &self.consistency_requirement
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

    /// Validates the complete logical requirement.
    ///
    /// Validation is limited to the logical contract. There is deliberately
    /// no provider, storage, database, partition, shard, algorithm, or
    /// execution validation here.
    pub fn validate(&self) -> Result<(), IndexRequirementValidationError> {
        self.key_definition
            .validate()
            .map_err(IndexRequirementValidationError::InvalidKeyDefinition)?;

        // The remaining logical component types are validated when they are
        // constructed. They do not expose mutable internal state, so there is
        // no second validation path to duplicate here. Keeping their
        // validated newtypes intact also prevents invalid logical values from
        // entering the requirement through this boundary.
        Ok(())
    }

    /// Normalizes this source requirement into Indexing's canonical logical
    /// definition.
    ///
    /// `definition_id` is supplied explicitly because Phase 2 does not
    /// silently freeze the deferred definition/index hash-generation
    /// algorithm. The supplied identifier is incorporated into the existing
    /// Phase 1 [`IndexDefinitionIdentity`].
    ///
    /// This method performs only logical normalization. It does not select a
    /// physical provider, build an index, allocate storage, publish a
    /// version, or execute a query.
    pub fn normalize(
        &self,
        definition_id: IndexDefinitionId,
    ) -> Result<IndexDefinition, IndexRequirementValidationError> {
        self.validate()?;

        let identity =
            IndexDefinitionIdentity::new(definition_id, self.namespace.clone(), self.family);

        IndexDefinition::new(
            identity,
            self.key_definition.clone(),
            self.target_reference_type.clone(),
            self.uniqueness,
            self.consistency_requirement.clone(),
            self.source_version.clone(),
            self.schema_version.clone(),
        )
        .map_err(IndexRequirementValidationError::DefinitionNormalization)
    }

    /// Consumes the requirement and returns its logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        IndexNamespace,
        IndexFamily,
        KeyDefinition,
        TargetReferenceType,
        Uniqueness,
        ConsistencyRequirement,
        Option<SourceVersion>,
        Option<SchemaVersion>,
    ) {
        (
            self.namespace,
            self.family,
            self.key_definition,
            self.target_reference_type,
            self.uniqueness,
            self.consistency_requirement,
            self.source_version,
            self.schema_version,
        )
    }
}

/// Logical validation failures for [`IndexRequirement`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexRequirementValidationError {
    /// The generic key definition is invalid.
    InvalidKeyDefinition(crate::index::KeyDefinitionValidationError),

    /// Normalization could not produce the canonical logical definition.
    DefinitionNormalization(crate::index::IndexDefinitionValidationError),
}

impl fmt::Display for IndexRequirementValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyDefinition(error) => {
                write!(formatter, "invalid key definition: {error}")
            }
            Self::DefinitionNormalization(error) => {
                write!(formatter, "requirement normalization failed: {error}")
            }
        }
    }
}

impl std::error::Error for IndexRequirementValidationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::KeyDefinition;

    fn namespace(value: &str) -> IndexNamespace {
        IndexNamespace::new(value).expect("test namespace must be valid")
    }

    fn definition_id(value: &str) -> IndexDefinitionId {
        IndexDefinitionId::new(value).expect("test definition ID must be valid")
    }

    fn key_definition() -> KeyDefinition {
        KeyDefinition::new(["term"]).expect("test key definition must be valid")
    }

    fn target_reference_type() -> TargetReferenceType {
        TargetReferenceType::new("source.object").expect("test target-reference type must be valid")
    }

    fn consistency() -> ConsistencyRequirement {
        ConsistencyRequirement::new("logical").expect("test consistency requirement must be valid")
    }

    fn requirement() -> IndexRequirement {
        IndexRequirement::new(
            namespace("lexical"),
            IndexFamily::Inverted,
            key_definition(),
            target_reference_type(),
            Uniqueness::NonUnique,
            consistency(),
            Some(SourceVersion::new("source-v1").expect("test source version must be valid")),
            Some(SchemaVersion::new("schema-v1").expect("test schema version must be valid")),
        )
        .expect("test requirement must be valid")
    }

    #[test]
    fn valid_requirement_preserves_all_logical_components() {
        let requirement = requirement();

        assert_eq!(requirement.namespace().as_str(), "lexical");
        assert_eq!(requirement.family(), IndexFamily::Inverted);
        assert_eq!(requirement.key_definition().len(), 1);
        assert_eq!(
            requirement.target_reference_type().as_str(),
            "source.object"
        );
        assert_eq!(requirement.uniqueness(), Uniqueness::NonUnique);
        assert_eq!(requirement.consistency_requirement().as_str(), "logical");
        assert_eq!(requirement.source_version().unwrap().as_ref(), "source-v1");
        assert_eq!(requirement.schema_version().unwrap().as_ref(), "schema-v1");
    }

    #[test]
    fn requirement_and_definition_are_distinct_contracts() {
        let requirement = requirement();
        let definition = requirement
            .normalize(definition_id("lexical.v1"))
            .expect("normalization should succeed");

        assert_eq!(definition.namespace().as_str(), "lexical");
        assert_eq!(definition.family(), IndexFamily::Inverted);
        assert_eq!(definition.definition_id().as_str(), "lexical.v1");
        assert_eq!(definition.definition_id().as_str(), "lexical.v1");
        assert_eq!(definition.key_definition(), requirement.key_definition());
        assert_eq!(
            definition.target_reference_type(),
            requirement.target_reference_type()
        );
    }

    #[test]
    fn normalization_supplies_definition_identity_without_collapsing_index_identity() {
        let requirement = requirement();
        let definition = requirement
            .normalize(definition_id("lexical.v1"))
            .expect("normalization should succeed");

        assert_eq!(definition.identity().definition_id().as_str(), "lexical.v1");
        assert_eq!(definition.namespace().as_str(), "lexical");
        assert_eq!(definition.family(), IndexFamily::Inverted);

        // No IndexId is generated by normalization. Concrete index identity
        // remains a separate Phase 1 concept.
    }

    #[test]
    fn normalization_preserves_source_and_schema_versions() {
        let requirement = requirement();
        let definition = requirement
            .normalize(definition_id("lexical.v1"))
            .expect("normalization should succeed");

        assert_eq!(definition.source_version().unwrap().as_ref(), "source-v1");
        assert_eq!(definition.schema_version().unwrap().as_ref(), "schema-v1");
    }

    #[test]
    fn normalization_preserves_key_definition() {
        let requirement = requirement();
        let definition = requirement
            .normalize(definition_id("lexical.v1"))
            .expect("normalization should succeed");

        assert_eq!(definition.key_definition(), requirement.key_definition());
    }

    #[test]
    fn different_families_remain_generic_and_do_not_gain_semantic_models() {
        for family in [
            IndexFamily::Identity,
            IndexFamily::Inverted,
            IndexFamily::Relationship,
            IndexFamily::Similarity,
        ] {
            let requirement = IndexRequirement::new(
                namespace("generic"),
                family,
                key_definition(),
                target_reference_type(),
                Uniqueness::NonUnique,
                consistency(),
                None,
                None,
            )
            .expect("generic requirement should be valid");

            assert_eq!(requirement.family(), family);
        }
    }

    #[test]
    fn provider_specific_configuration_has_no_requirement_representation() {
        // The requirement exposes only logical fields. Physical provider
        // concepts such as BTree/HNSW/Postgres/Mongo/shards/partitions do
        // not have a field or API in this contract.
        let requirement = requirement();
        assert_eq!(requirement.namespace().as_str(), "lexical");
    }

    #[test]
    fn validation_is_repeatable() {
        let requirement = requirement();

        assert_eq!(requirement.validate(), Ok(()));
        assert_eq!(requirement.validate(), Ok(()));
    }

    #[test]
    fn into_parts_round_trips_logical_components() {
        let requirement = requirement();
        let (
            namespace,
            family,
            key_definition,
            target_reference_type,
            uniqueness,
            consistency,
            source_version,
            schema_version,
        ) = requirement.into_parts();

        assert_eq!(namespace.as_str(), "lexical");
        assert_eq!(family, IndexFamily::Inverted);
        assert_eq!(key_definition.len(), 1);
        assert_eq!(target_reference_type.as_str(), "source.object");
        assert_eq!(uniqueness, Uniqueness::NonUnique);
        assert_eq!(consistency.as_str(), "logical");
        assert_eq!(source_version.unwrap().as_ref(), "source-v1");
        assert_eq!(schema_version.unwrap().as_ref(), "schema-v1");
    }
}
