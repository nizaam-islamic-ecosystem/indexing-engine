//! Level 3 integration tests for Phase 1 index families.
//!
//! These tests exercise the public indexing-family API from an external
//! integration-test crate. They remain intentionally limited to Phase 1:
//! generic index-family classification and logical composition with a
//! namespace. No storage, construction, querying, embeddings, or semantic
//! relationship machinery is introduced here.

use nizaam_indexing::identity::IndexNamespace;
use nizaam_indexing::index::IndexFamily;

fn namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace must satisfy Phase 1 grammar")
}

#[test]
fn all_four_phase_one_index_families_exist() {
    let families = [
        IndexFamily::Identity,
        IndexFamily::Inverted,
        IndexFamily::Relationship,
        IndexFamily::Similarity,
    ];

    assert_eq!(families.len(), 4);
}

#[test]
fn all_four_index_families_are_distinct() {
    let families = [
        IndexFamily::Identity,
        IndexFamily::Inverted,
        IndexFamily::Relationship,
        IndexFamily::Similarity,
    ];

    for (index, family) in families.iter().enumerate() {
        for (other_index, other_family) in families.iter().enumerate() {
            if index == other_index {
                assert_eq!(family, other_family);
            } else {
                assert_ne!(family, other_family);
            }
        }
    }
}

#[test]
fn index_family_and_namespace_compose_as_separate_logical_dimensions() {
    let namespace = namespace("search.documents");

    let identity = (namespace.clone(), IndexFamily::Identity);
    let inverted = (namespace.clone(), IndexFamily::Inverted);
    let relationship = (namespace.clone(), IndexFamily::Relationship);
    let similarity = (namespace, IndexFamily::Similarity);

    assert_eq!(identity.0.as_str(), "search.documents");
    assert_eq!(identity.1, IndexFamily::Identity);

    assert_eq!(inverted.0.as_str(), "search.documents");
    assert_eq!(inverted.1, IndexFamily::Inverted);

    assert_eq!(relationship.0.as_str(), "search.documents");
    assert_eq!(relationship.1, IndexFamily::Relationship);

    assert_eq!(similarity.0.as_str(), "search.documents");
    assert_eq!(similarity.1, IndexFamily::Similarity);
}

#[test]
fn relationship_family_is_generic_and_does_not_encode_semantic_predicates() {
    let relationship = IndexFamily::Relationship;

    assert_eq!(relationship, IndexFamily::Relationship);

    // Phase 1 defines only the generic family classification. A relationship
    // family carries no predicate, relation name, graph edge, or domain
    // semantics.
    let logical_identity = (namespace("graph.relations"), relationship);

    assert_eq!(logical_identity.0.as_str(), "graph.relations");
    assert_eq!(logical_identity.1, IndexFamily::Relationship);
}

#[test]
fn similarity_family_is_generic_and_does_not_select_embedding_implementation() {
    let similarity = IndexFamily::Similarity;

    assert_eq!(similarity, IndexFamily::Similarity);

    // Phase 1 identifies the family only. It does not select an embedding
    // model, vector database, ANN structure, distance metric, or indexing
    // algorithm.
    let logical_identity = (namespace("search.similarity"), similarity);

    assert_eq!(logical_identity.0.as_str(), "search.similarity");
    assert_eq!(logical_identity.1, IndexFamily::Similarity);
}

#[test]
fn same_family_can_exist_in_different_namespaces() {
    let first = (namespace("core.identity"), IndexFamily::Identity);
    let second = (namespace("search.identity"), IndexFamily::Identity);

    assert_ne!(first.0, second.0);
    assert_eq!(first.1, second.1);
}

#[test]
fn different_families_can_exist_in_the_same_namespace() {
    let namespace = namespace("search.documents");

    let identity = (namespace.clone(), IndexFamily::Identity);
    let inverted = (namespace.clone(), IndexFamily::Inverted);
    let similarity = (namespace, IndexFamily::Similarity);

    assert_eq!(identity.0, inverted.0);
    assert_eq!(inverted.0, similarity.0);

    assert_ne!(identity.1, inverted.1);
    assert_ne!(inverted.1, similarity.1);
    assert_ne!(identity.1, similarity.1);
}

#[test]
fn family_display_is_the_public_generic_family_name() {
    assert_eq!(IndexFamily::Identity.to_string(), "identity");
    assert_eq!(IndexFamily::Inverted.to_string(), "inverted");
    assert_eq!(IndexFamily::Relationship.to_string(), "relationship");
    assert_eq!(IndexFamily::Similarity.to_string(), "similarity");
}
