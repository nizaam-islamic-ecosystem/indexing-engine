//! Public requirement boundary for the Indexing Engine.
//!
//! This module exposes the source-to-Indexing logical requirement contract.
//! The implementation remains in [`requirement`]; this file is intentionally
//! limited to module composition and public re-exports.

#[allow(clippy::module_inception)]
pub mod requirement;

pub use requirement::{IndexRequirement, IndexRequirementValidationError};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::IndexNamespace;
    use crate::index::{
        ConsistencyRequirement, IndexFamily, KeyDefinition, SchemaVersion, SourceVersion,
        TargetReferenceType, Uniqueness,
    };

    #[test]
    fn public_requirement_boundary_exposes_the_logical_contract() {
        let namespace = IndexNamespace::new("lexical").expect("test namespace must be valid");
        let key_definition =
            KeyDefinition::new(["term"]).expect("test key definition must be valid");
        let target_reference_type = TargetReferenceType::new("source.object")
            .expect("test target-reference type must be valid");
        let consistency = ConsistencyRequirement::new("logical")
            .expect("test consistency requirement must be valid");
        let source_version =
            SourceVersion::new("source-v1").expect("test source version must be valid");
        let schema_version =
            SchemaVersion::new("schema-v1").expect("test schema version must be valid");

        let requirement = IndexRequirement::new(
            namespace,
            IndexFamily::Inverted,
            key_definition,
            target_reference_type,
            Uniqueness::NonUnique,
            consistency,
            Some(source_version),
            Some(schema_version),
        )
        .expect("requirement should be valid");

        assert_eq!(requirement.family(), IndexFamily::Inverted);
        assert_eq!(requirement.namespace().as_str(), "lexical");
    }
}
