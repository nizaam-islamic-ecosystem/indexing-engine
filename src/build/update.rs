//! Logical incremental update orchestration for Phase 3.
//!
//! This module owns the Indexing-local mechanics for applying logical
//! create/update/delete mutations to an unpublished candidate and recording
//! those mutations in a monotonic logical update journal.
//!
//! The intended boundary is:
//!
//! ```text
//! existing candidate
//!       ↓
//! logical mutation
//!       ↓
//! validate mutation
//!       ↓
//! new candidate state
//!       ↓
//! journal successful mutation
//! ```
//!
//! Incremental updates deliberately accumulate in candidates. This module
//! never changes an active version and never performs publication.
//!
//! The update journal records logical Indexing mutations only. It is not a
//! source-domain mutation log, a Core operation/attempt identity system, or a
//! physical storage log.

use super::builder::{BuildCandidate, BuildError, IndexBuilder};
use crate::error::IndexingResult;
use crate::index::{
    IndexEntry, IndexVersion, IndexVersionId, KeyMaterial, ObjectReference, SchemaVersion,
    SourceVersion, Uniqueness,
};
use core::fmt;
use nizaam_core::contracts::Version as CoreVersion;
use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
};
use nizaam_core::status::Retryability;
use std::error::Error;

/// Monotonic logical sequence assigned to accepted indexing mutations.
///
/// This is a construction/versioning sequence used by the update journal. It
/// is intentionally distinct from Core's `OperationId` and `AttemptId` and is
/// not an execution identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UpdateSequence(u64);

impl UpdateSequence {
    /// The initial sequence before the first accepted mutation.
    pub const INITIAL: Self = Self(0);

    /// Constructs an explicit sequence value.
    ///
    /// Explicit construction is useful when restoring or bridging logical
    /// journal state. The monotonicity guarantee of a journal remains the
    /// journal's responsibility.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric sequence value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Returns the next representable sequence value.
    ///
    /// Sequence exhaustion is treated as an error rather than wrapping back
    /// to an earlier sequence, because replay ordering depends on monotonicity.
    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

impl fmt::Display for UpdateSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Logical selector identifying the indexed state to remove.
///
/// A target selector removes every logical entry associated with the supplied
/// source-owned reference. A key selector removes entries whose complete
/// logical key material is equal to the supplied material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeleteSelector {
    /// Remove all logical entries targeting this source-owned object.
    Target(ObjectReference),

    /// Remove all logical entries carrying this exact logical key material.
    Key(KeyMaterial),
}

impl DeleteSelector {
    /// Creates a target-reference deletion selector.
    #[must_use]
    pub fn target(reference: ObjectReference) -> Self {
        Self::Target(reference)
    }

    /// Creates a key-material deletion selector.
    #[must_use]
    pub fn key(key: KeyMaterial) -> Self {
        Self::Key(key)
    }

    /// Returns the selected target reference, when this is a target selector.
    #[must_use]
    pub fn target_reference(&self) -> Option<&ObjectReference> {
        match self {
            Self::Target(reference) => Some(reference),
            Self::Key(_) => None,
        }
    }

    /// Returns the selected key material, when this is a key selector.
    #[must_use]
    pub fn key_material(&self) -> Option<&KeyMaterial> {
        match self {
            Self::Target(_) => None,
            Self::Key(key) => Some(key),
        }
    }
}

/// One logical incremental mutation accepted by the Indexing Engine.
///
/// The mutation describes only the logical index representation. It contains
/// no source-domain mutation, provider operation, database command, or query
/// execution instruction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexMutation {
    /// Add a new logical index entry.
    Insert(IndexEntry),

    /// Replace the logical representation associated with one target.
    ///
    /// All entries whose target equals `target` are replaced by `replacement`.
    /// The replacement entry's target must identify the same logical target so
    /// that an update cannot silently move an object to another reference.
    Update {
        /// Existing source-owned target being updated.
        target: ObjectReference,

        /// New logical indexed representation for the target.
        replacement: IndexEntry,
    },

    /// Remove logical index entries selected by target or key identity.
    Delete(DeleteSelector),
}

impl IndexMutation {
    /// Returns the mutation's logical target reference when one is directly
    /// identified by the mutation.
    #[must_use]
    pub fn target(&self) -> Option<&ObjectReference> {
        match self {
            Self::Insert(entry) => Some(entry.target()),
            Self::Update { target, .. } => Some(target),
            Self::Delete(DeleteSelector::Target(reference)) => Some(reference),
            Self::Delete(DeleteSelector::Key(_)) => None,
        }
    }

    /// Returns the replacement entry for an update mutation.
    #[must_use]
    pub fn replacement(&self) -> Option<&IndexEntry> {
        match self {
            Self::Update { replacement, .. } => Some(replacement),
            Self::Insert(_) | Self::Delete(_) => None,
        }
    }
}

/// One successful logical update together with its replay sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateRecord {
    sequence: UpdateSequence,
    mutation: IndexMutation,
}

impl UpdateRecord {
    /// Creates a logical update record with an explicit sequence.
    #[must_use]
    pub fn new(sequence: UpdateSequence, mutation: IndexMutation) -> Self {
        Self { sequence, mutation }
    }

    /// Returns the monotonic sequence assigned to this mutation.
    #[must_use]
    pub fn sequence(&self) -> UpdateSequence {
        self.sequence
    }

    /// Returns the logical mutation represented by this record.
    #[must_use]
    pub fn mutation(&self) -> &IndexMutation {
        &self.mutation
    }

    /// Consumes the record and returns its logical components.
    #[must_use]
    pub fn into_parts(self) -> (UpdateSequence, IndexMutation) {
        (self.sequence, self.mutation)
    }
}

/// Logical append-only update history used by construction/rebuild workflows.
///
/// A journal assigns strictly increasing sequence values to successful
/// mutations. The journal is intentionally provider-neutral; physical
/// durability, persistence, retention, and synchronization are outside this
/// module's contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateJournal {
    next_sequence: UpdateSequence,
    records: Vec<UpdateRecord>,
}

impl Default for UpdateJournal {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateJournal {
    /// Creates an empty logical update journal.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_sequence: UpdateSequence::INITIAL,
            records: Vec::new(),
        }
    }

    /// Returns the latest accepted sequence, or zero before any mutation has
    /// been recorded.
    #[must_use]
    pub const fn current_sequence(&self) -> UpdateSequence {
        self.next_sequence
    }

    /// Returns the complete logical mutation history in sequence order.
    #[must_use]
    pub fn records(&self) -> &[UpdateRecord] {
        &self.records
    }

    /// Returns whether the journal contains no accepted mutations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Returns the number of accepted logical mutations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Appends a successful logical mutation and assigns the next sequence.
    ///
    /// The returned record is also retained by the journal and can therefore
    /// be handed to a rebuild/replay workflow without generating a second
    /// sequence identity.
    pub fn append(&mut self, mutation: IndexMutation) -> Result<&UpdateRecord, UpdateJournalError> {
        let sequence = self
            .next_sequence
            .next()
            .ok_or(UpdateJournalError::SequenceExhausted)?;

        self.records.push(UpdateRecord::new(sequence, mutation));
        self.next_sequence = sequence;

        Ok(self
            .records
            .last()
            .expect("journal contains the record appended immediately above"))
    }

    /// Returns mutations whose sequence is strictly greater than `after`.
    ///
    /// This provides the deterministic replay boundary needed by rebuild
    /// orchestration while keeping replay execution itself outside this file.
    pub fn records_after(&self, after: UpdateSequence) -> impl Iterator<Item = &UpdateRecord> {
        self.records
            .iter()
            .filter(move |record| record.sequence() > after)
    }

    /// Consumes the journal and returns its next sequence and records.
    #[must_use]
    pub fn into_parts(self) -> (UpdateSequence, Vec<UpdateRecord>) {
        (self.next_sequence, self.records)
    }
}

/// Failures raised by the logical update journal itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateJournalError {
    /// No later sequence value can be represented by `u64`.
    SequenceExhausted,
}

impl fmt::Display for UpdateJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SequenceExhausted => formatter.write_str("index update sequence is exhausted"),
        }
    }
}

impl Error for UpdateJournalError {}

/// Failures produced while applying a logical incremental update.
#[derive(Debug, Eq, PartialEq)]
pub enum UpdateError {
    /// A supplied candidate or one of its logical records failed validation.
    Build(BuildError),

    /// An update attempted to target a source-owned reference not present in
    /// the candidate.
    TargetNotFound(ObjectReference),

    /// A unique index already contains another target with the same logical
    /// key material.
    UniqueKeyConflict {
        /// Logical key that conflicts with an existing entry.
        key: KeyMaterial,
    },

    /// An insert would create an exact duplicate of an existing logical entry.
    DuplicateEntry(IndexEntry),

    /// An update attempted to replace a target with an entry carrying a
    /// different source-owned target reference.
    UpdateTargetMismatch {
        /// Target selected by the update operation.
        target: ObjectReference,

        /// Target carried by the replacement entry.
        replacement_target: ObjectReference,
    },

    /// A key selector for deletion contained invalid key material.
    InvalidDeleteKey(crate::index::KeyMaterialValidationError),

    /// The update journal could not allocate a new logical sequence.
    Journal(UpdateJournalError),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(error) => write!(
                formatter,
                "candidate update rejected by build validation: {error}"
            ),
            Self::TargetNotFound(reference) => write!(
                formatter,
                "index update target was not found: {}/{}",
                reference.source(),
                reference.object_reference()
            ),
            Self::UniqueKeyConflict { key } => {
                write!(formatter, "unique index key conflict for {key:?}")
            }
            Self::DuplicateEntry(entry) => {
                write!(formatter, "logical index entry already exists: {entry:?}")
            }
            Self::UpdateTargetMismatch {
                target,
                replacement_target,
            } => write!(
                formatter,
                "update target mismatch: selected {}/{} but replacement targets {}/{}",
                target.source(),
                target.object_reference(),
                replacement_target.source(),
                replacement_target.object_reference()
            ),
            Self::InvalidDeleteKey(error) => {
                write!(formatter, "invalid delete key material: {error}")
            }
            Self::Journal(error) => {
                write!(formatter, "update journal rejected mutation: {error}")
            }
        }
    }
}

impl Error for UpdateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(error) => Some(error),
            Self::InvalidDeleteKey(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::TargetNotFound(_)
            | Self::UniqueKeyConflict { .. }
            | Self::DuplicateEntry(_)
            | Self::UpdateTargetMismatch { .. } => None,
        }
    }
}

impl UpdateJournalError {
    /// Converts a journal failure into the shared Core error contract.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        GlobalError {
            code: ErrorCode::new("INDEXING.UPDATE.007")
                .expect("Indexing error code is statically valid"),
            owner: ErrorOwner::new("INDEXING").expect("Indexing error owner is statically valid"),
            version: CoreVersion::new(1, 0, 0),
            class: ErrorClass::Resource,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message: self.to_string(),
            details: Vec::new(),
            solution_reference: None,
            context,
            cause: None,
        }
    }
}

impl UpdateError {
    /// Converts the typed update failure into the shared Core error contract.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        let (code, class, message, details) = match self {
            Self::Build(error) => (
                "INDEXING.UPDATE.001",
                ErrorClass::Validation,
                format!("candidate update rejected by build validation: {error}"),
                Vec::new(),
            ),
            Self::TargetNotFound(reference) => (
                "INDEXING.UPDATE.002",
                ErrorClass::Contract,
                format!(
                    "index update target was not found: {}/{}",
                    reference.source(),
                    reference.object_reference()
                ),
                vec![
                    ("target_source", reference.source().to_owned()),
                    ("target_reference", reference.object_reference().to_owned()),
                ],
            ),
            Self::UniqueKeyConflict { key } => (
                "INDEXING.UPDATE.003",
                ErrorClass::Contract,
                format!("unique index key conflict for {key:?}"),
                vec![("conflicting_key", format!("{key:?}"))],
            ),
            Self::DuplicateEntry(entry) => (
                "INDEXING.UPDATE.004",
                ErrorClass::Contract,
                format!("logical index entry already exists: {entry:?}"),
                vec![("entry", format!("{entry:?}"))],
            ),
            Self::UpdateTargetMismatch {
                target,
                replacement_target,
            } => (
                "INDEXING.UPDATE.005",
                ErrorClass::Contract,
                format!(
                    "update target mismatch: selected {}/{} but replacement targets {}/{}",
                    target.source(),
                    target.object_reference(),
                    replacement_target.source(),
                    replacement_target.object_reference()
                ),
                vec![
                    ("target_source", target.source().to_owned()),
                    ("target_reference", target.object_reference().to_owned()),
                    (
                        "replacement_target_source",
                        replacement_target.source().to_owned(),
                    ),
                    (
                        "replacement_target_reference",
                        replacement_target.object_reference().to_owned(),
                    ),
                ],
            ),
            Self::InvalidDeleteKey(error) => (
                "INDEXING.UPDATE.006",
                ErrorClass::Validation,
                format!("invalid delete key material: {error}"),
                Vec::new(),
            ),
            Self::Journal(error) => (
                "INDEXING.UPDATE.007",
                ErrorClass::Resource,
                format!("update journal rejected mutation: {error}"),
                Vec::new(),
            ),
        };

        let mut global = GlobalError {
            code: ErrorCode::new(code).expect("Indexing error code is statically valid"),
            owner: ErrorOwner::new("INDEXING").expect("Indexing error owner is statically valid"),
            version: CoreVersion::new(1, 0, 0),
            class,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message,
            details: Vec::new(),
            solution_reference: None,
            context,
            cause: None,
        };

        for (key, value) in details {
            if let Some(detail) = nizaam_core::error::DiagnosticDetail::new(key, value) {
                global = global.with_detail(detail);
            }
        }

        global
    }
}

impl CandidateUpdater {
    /// Core-error result adapter for candidate creation.
    #[allow(clippy::result_large_err)]
    pub fn create_candidate_result(
        &self,
        candidate: &BuildCandidate,
        candidate_version_id: IndexVersionId,
        source_version: Option<SourceVersion>,
        schema_version: Option<SchemaVersion>,
        context: ErrorContext,
    ) -> IndexingResult<BuildCandidate> {
        self.create_candidate(
            candidate,
            candidate_version_id,
            source_version,
            schema_version,
        )
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for one logical mutation.
    #[allow(clippy::result_large_err)]
    pub fn apply_result(
        &self,
        candidate: &BuildCandidate,
        mutation: IndexMutation,
        context: ErrorContext,
    ) -> IndexingResult<BuildCandidate> {
        self.apply(candidate, mutation)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for a logical mutation collection.
    #[allow(clippy::result_large_err)]
    pub fn apply_mutations_result<I>(
        &self,
        candidate: &BuildCandidate,
        mutations: I,
        context: ErrorContext,
    ) -> IndexingResult<BuildCandidate>
    where
        I: IntoIterator<Item = IndexMutation>,
    {
        self.apply_mutations(candidate, mutations)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for mutation plus journal recording.
    #[allow(clippy::result_large_err)]
    pub fn apply_and_record_result(
        &self,
        candidate: &BuildCandidate,
        journal: &mut UpdateJournal,
        mutation: IndexMutation,
        context: ErrorContext,
    ) -> IndexingResult<(BuildCandidate, UpdateSequence)> {
        self.apply_and_record(candidate, mutation, journal)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }
}

/// Stateless Indexing-local incremental update operator.
///
/// Updates are functional at this boundary: applying a mutation returns a
/// new unpublished candidate and leaves the supplied candidate unchanged.
/// This gives later lifecycle coordination an explicit publication boundary
/// and prevents an update from mutating an active candidate by alias.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandidateUpdater;

impl CandidateUpdater {
    /// Creates a stateless candidate updater.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Creates a new update candidate from an existing logical candidate.
    ///
    /// Existing entries and the governing definition are copied into the new
    /// candidate. Source/schema metadata can be preserved from the base
    /// version or explicitly replaced for the new source state.
    ///
    /// No active-version state is changed and no publication occurs.
    pub fn create_candidate(
        &self,
        base: &BuildCandidate,
        candidate_version_id: IndexVersionId,
        source_version: Option<SourceVersion>,
        schema_version: Option<SchemaVersion>,
    ) -> Result<BuildCandidate, UpdateError> {
        let source_version = source_version.or_else(|| base.version().source_version().cloned());
        let schema_version = schema_version.or_else(|| base.version().schema_version().cloned());

        let version = IndexVersion::with_metadata(
            candidate_version_id,
            source_version,
            schema_version,
            base.version().metadata().cloned(),
        )
        .map_err(|error| UpdateError::Build(BuildError::InvalidVersion(error)))?;

        let candidate = BuildCandidate::from_parts(
            *base.index_id(),
            base.definition().clone(),
            version,
            base.entries().to_vec(),
        );

        IndexBuilder::validate(&candidate).map_err(UpdateError::Build)?;
        Ok(candidate)
    }

    /// Applies a single logical mutation without publishing it.
    ///
    /// The returned candidate contains the mutation while the supplied
    /// candidate remains unchanged.
    pub fn apply(
        &self,
        candidate: &BuildCandidate,
        mutation: IndexMutation,
    ) -> Result<BuildCandidate, UpdateError> {
        self.apply_mutations(candidate, core::iter::once(mutation))
    }

    /// Applies an iterator of logical mutations to a candidate without
    /// publishing the resulting state.
    ///
    /// The operation is all-or-nothing at this logical update boundary: if any
    /// mutation fails, no candidate is returned and the supplied candidate is
    /// unchanged.
    pub fn apply_mutations<I>(
        &self,
        candidate: &BuildCandidate,
        mutations: I,
    ) -> Result<BuildCandidate, UpdateError>
    where
        I: IntoIterator<Item = IndexMutation>,
    {
        IndexBuilder::validate(candidate).map_err(UpdateError::Build)?;

        let mut entries = candidate.entries().to_vec();

        for mutation in mutations {
            apply_mutation(candidate.definition(), &mut entries, mutation)?;
        }

        let updated = BuildCandidate::from_parts(
            *candidate.index_id(),
            candidate.definition().clone(),
            candidate.version().clone(),
            entries,
        );

        IndexBuilder::validate(&updated).map_err(UpdateError::Build)?;
        Ok(updated)
    }

    /// Applies one logical mutation and records it in the supplied journal
    /// only after the candidate mutation succeeds.
    pub fn apply_and_record(
        &self,
        candidate: &BuildCandidate,
        mutation: IndexMutation,
        journal: &mut UpdateJournal,
    ) -> Result<(BuildCandidate, UpdateSequence), UpdateError> {
        let updated = self.apply(candidate, mutation.clone())?;
        let record = journal.append(mutation).map_err(UpdateError::Journal)?;

        Ok((updated, record.sequence()))
    }
}

fn apply_mutation(
    definition: &crate::index::IndexDefinition,
    entries: &mut Vec<IndexEntry>,
    mutation: IndexMutation,
) -> Result<(), UpdateError> {
    match mutation {
        IndexMutation::Insert(entry) => insert_entry(definition.uniqueness(), entries, entry),
        IndexMutation::Update {
            target,
            replacement,
        } => update_entry(definition.uniqueness(), entries, target, replacement),
        IndexMutation::Delete(selector) => delete_entries(entries, selector),
    }
}

fn insert_entry(
    uniqueness: Uniqueness,
    entries: &mut Vec<IndexEntry>,
    entry: IndexEntry,
) -> Result<(), UpdateError> {
    entry.validate().map_err(|error| {
        UpdateError::Build(BuildError::InvalidEntry {
            position: entries.len(),
            error,
        })
    })?;

    if entries.iter().any(|existing| existing == &entry) {
        return Err(UpdateError::DuplicateEntry(entry));
    }

    if uniqueness == Uniqueness::Unique
        && entries.iter().any(|existing| existing.key() == entry.key())
    {
        return Err(UpdateError::UniqueKeyConflict {
            key: entry.key().clone(),
        });
    }

    entries.push(entry);
    Ok(())
}

fn update_entry(
    uniqueness: Uniqueness,
    entries: &mut Vec<IndexEntry>,
    target: ObjectReference,
    replacement: IndexEntry,
) -> Result<(), UpdateError> {
    replacement.validate().map_err(|error| {
        UpdateError::Build(BuildError::InvalidEntry {
            position: entries.len(),
            error,
        })
    })?;

    if replacement.target() != &target {
        return Err(UpdateError::UpdateTargetMismatch {
            target,
            replacement_target: replacement.target().clone(),
        });
    }

    let positions: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, existing)| (existing.target() == &target).then_some(index))
        .collect();

    if positions.is_empty() {
        return Err(UpdateError::TargetNotFound(target));
    }

    if uniqueness == Uniqueness::Unique
        && entries.iter().enumerate().any(|(index, existing)| {
            !positions.contains(&index) && existing.key() == replacement.key()
        })
    {
        return Err(UpdateError::UniqueKeyConflict {
            key: replacement.key().clone(),
        });
    }

    let first_position = positions[0];

    entries.retain(|existing| existing.target() != &target);

    entries.insert(first_position.min(entries.len()), replacement);
    Ok(())
}

fn delete_entries(
    entries: &mut Vec<IndexEntry>,
    selector: DeleteSelector,
) -> Result<(), UpdateError> {
    match selector {
        DeleteSelector::Target(reference) => {
            let original_len = entries.len();
            entries.retain(|entry| entry.target() != &reference);

            if entries.len() == original_len {
                return Err(UpdateError::TargetNotFound(reference));
            }
        }
        DeleteSelector::Key(key) => {
            key.validate().map_err(UpdateError::InvalidDeleteKey)?;
            entries.retain(|entry| entry.key() != &key);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{ConsistencyRequirement, IndexFamily, KeyDefinition, TargetReferenceType};

    fn index_id(seed: u8) -> crate::identity::IndexId {
        crate::identity::IndexId::from_bytes([seed; 64])
    }

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    fn source_version(value: &str) -> SourceVersion {
        SourceVersion::new(value).expect("test source version should be valid")
    }

    fn schema_version(value: &str) -> SchemaVersion {
        SchemaVersion::new(value).expect("test schema version should be valid")
    }

    fn definition(uniqueness: Uniqueness) -> crate::index::IndexDefinition {
        let identity = IndexDefinitionIdentity::new(
            IndexDefinitionId::new("documents.v1").expect("test definition ID should be valid"),
            IndexNamespace::new("search.documents").expect("test namespace should be valid"),
            IndexFamily::Inverted,
        );

        crate::index::IndexDefinition::new(
            identity,
            KeyDefinition::new(["term"]).expect("test key definition should be valid"),
            TargetReferenceType::new("source.document")
                .expect("test target reference type should be valid"),
            uniqueness,
            ConsistencyRequirement::new("logical-v1")
                .expect("test consistency requirement should be valid"),
            Some(source_version("source-v1")),
            Some(schema_version("schema-v1")),
        )
        .expect("test definition should be valid")
    }

    fn candidate(uniqueness: Uniqueness, entries: Vec<IndexEntry>) -> BuildCandidate {
        let definition = definition(uniqueness);
        BuildCandidate::from_parts(
            index_id(0x11),
            definition,
            IndexVersion::with_metadata(
                version_id("index-v1"),
                Some(source_version("source-v1")),
                Some(schema_version("schema-v1")),
                None,
            )
            .expect("test version should be valid"),
            entries,
        )
    }

    fn entry(key: &str, reference: &str) -> IndexEntry {
        IndexEntry::new(
            KeyMaterial::text(key),
            ObjectReference::new("documents", reference)
                .expect("test object reference should be valid"),
        )
        .expect("test entry should be valid")
    }

    fn reference(value: &str) -> ObjectReference {
        ObjectReference::new("documents", value).expect("test object reference should be valid")
    }

    #[test]
    fn update_sequence_starts_at_zero_and_increments_monotonically() {
        let mut journal = UpdateJournal::new();
        assert_eq!(journal.current_sequence(), UpdateSequence::INITIAL);

        let first_sequence = journal
            .append(IndexMutation::Insert(entry("a", "doc:1")))
            .expect("first journal append should succeed")
            .sequence();

        let second_sequence = journal
            .append(IndexMutation::Insert(entry("b", "doc:2")))
            .expect("second journal append should succeed")
            .sequence();

        assert_eq!(first_sequence, UpdateSequence::new(1));
        assert_eq!(second_sequence, UpdateSequence::new(2));
        assert_eq!(journal.current_sequence(), UpdateSequence::new(2));
    }

    #[test]
    fn journal_preserves_logical_mutation_order() {
        let mut journal = UpdateJournal::new();
        journal
            .append(IndexMutation::Insert(entry("a", "doc:1")))
            .expect("append should succeed");
        journal
            .append(IndexMutation::Update {
                target: reference("doc:1"),
                replacement: entry("b", "doc:1"),
            })
            .expect("append should succeed");

        let sequences: Vec<UpdateSequence> = journal
            .records_after(UpdateSequence::new(0))
            .map(UpdateRecord::sequence)
            .collect();

        assert_eq!(sequences, [UpdateSequence::new(1), UpdateSequence::new(2)]);
    }

    #[test]
    fn insert_adds_entry_without_changing_original_candidate() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let updated = updater
            .apply(&original, IndexMutation::Insert(entry("b", "doc:2")))
            .expect("insert should succeed");

        assert_eq!(original.len(), 1);
        assert_eq!(updated.len(), 2);
        assert_eq!(updated.entries()[1], entry("b", "doc:2"));
    }

    #[test]
    fn duplicate_insert_is_rejected() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let error = updater
            .apply(&original, IndexMutation::Insert(entry("a", "doc:1")))
            .expect_err("exact duplicate insert should fail");

        assert!(matches!(error, UpdateError::DuplicateEntry(_)));
        assert_eq!(original.len(), 1);
    }

    #[test]
    fn unique_index_rejects_same_key_for_another_target() {
        let original = candidate(Uniqueness::Unique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let error = updater
            .apply(&original, IndexMutation::Insert(entry("a", "doc:2")))
            .expect_err("unique key conflict should fail");

        assert!(matches!(error, UpdateError::UniqueKeyConflict { .. }));
        assert_eq!(original.len(), 1);
    }

    #[test]
    fn update_replaces_all_existing_entries_for_a_target() {
        let original = candidate(
            Uniqueness::NonUnique,
            vec![
                entry("a", "doc:1"),
                entry("b", "doc:1"),
                entry("c", "doc:2"),
            ],
        );
        let updater = CandidateUpdater::new();

        let updated = updater
            .apply(
                &original,
                IndexMutation::Update {
                    target: reference("doc:1"),
                    replacement: entry("new", "doc:1"),
                },
            )
            .expect("target update should succeed");

        assert_eq!(original.len(), 3);
        assert_eq!(updated.len(), 2);
        assert_eq!(updated.entries()[0], entry("new", "doc:1"));
        assert_eq!(updated.entries()[1], entry("c", "doc:2"));
    }

    #[test]
    fn update_rejects_missing_target() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let error = updater
            .apply(
                &original,
                IndexMutation::Update {
                    target: reference("doc:2"),
                    replacement: entry("b", "doc:2"),
                },
            )
            .expect_err("missing update target should fail");

        assert!(matches!(error, UpdateError::TargetNotFound(_)));
        assert_eq!(original.len(), 1);
    }

    #[test]
    fn update_rejects_target_mismatch() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let error = updater
            .apply(
                &original,
                IndexMutation::Update {
                    target: reference("doc:1"),
                    replacement: entry("b", "doc:2"),
                },
            )
            .expect_err("replacement target mismatch should fail");

        assert!(matches!(error, UpdateError::UpdateTargetMismatch { .. }));
    }

    #[test]
    fn delete_by_target_removes_all_representations_for_the_target() {
        let original = candidate(
            Uniqueness::NonUnique,
            vec![
                entry("a", "doc:1"),
                entry("b", "doc:1"),
                entry("c", "doc:2"),
            ],
        );
        let updater = CandidateUpdater::new();

        let updated = updater
            .apply(
                &original,
                IndexMutation::Delete(DeleteSelector::Target(reference("doc:1"))),
            )
            .expect("target deletion should succeed");

        assert_eq!(original.len(), 3);
        assert_eq!(updated.len(), 1);
        assert_eq!(updated.entries()[0], entry("c", "doc:2"));
    }

    #[test]
    fn delete_by_key_removes_matching_logical_entries() {
        let original = candidate(
            Uniqueness::NonUnique,
            vec![
                entry("a", "doc:1"),
                entry("a", "doc:2"),
                entry("b", "doc:3"),
            ],
        );
        let updater = CandidateUpdater::new();

        let updated = updater
            .apply(
                &original,
                IndexMutation::Delete(DeleteSelector::Key(KeyMaterial::text("a"))),
            )
            .expect("key deletion should succeed");

        assert_eq!(original.len(), 3);
        assert_eq!(updated.len(), 1);
        assert_eq!(updated.entries()[0], entry("b", "doc:3"));
    }

    #[test]
    fn target_delete_reports_missing_target() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let error = updater
            .apply(
                &original,
                IndexMutation::Delete(DeleteSelector::Target(reference("doc:9"))),
            )
            .expect_err("missing deletion target should fail");

        assert!(matches!(error, UpdateError::TargetNotFound(_)));
        assert_eq!(original.len(), 1);
    }

    #[test]
    fn failed_mutation_does_not_modify_candidate() {
        let original = candidate(Uniqueness::Unique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let result = updater.apply(&original, IndexMutation::Insert(entry("a", "doc:2")));

        assert!(result.is_err());
        assert_eq!(original.len(), 1);
        assert_eq!(original.entries()[0], entry("a", "doc:1"));
    }

    #[test]
    fn multiple_mutations_are_all_applied_when_each_succeeds() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let updated = updater
            .apply_mutations(
                &original,
                vec![
                    IndexMutation::Insert(entry("b", "doc:2")),
                    IndexMutation::Insert(entry("c", "doc:3")),
                ],
            )
            .expect("all logical mutations should succeed");

        assert_eq!(original.len(), 1);
        assert_eq!(updated.len(), 3);
    }

    #[test]
    fn multiple_mutations_fail_as_one_logical_update_boundary() {
        let original = candidate(Uniqueness::Unique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();

        let result = updater.apply_mutations(
            &original,
            vec![
                IndexMutation::Insert(entry("b", "doc:2")),
                IndexMutation::Insert(entry("a", "doc:3")),
            ],
        );

        assert!(result.is_err());
        assert_eq!(original.len(), 1);
        assert_eq!(original.entries()[0], entry("a", "doc:1"));
    }

    #[test]
    fn create_candidate_copies_base_entries_and_changes_version_metadata() {
        // Use an unconstrained definition so this test exercises the documented
        // ability to replace source/schema metadata for a new logical version.
        let definition = crate::index::IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("documents.unconstrained")
                    .expect("test definition ID should be valid"),
                IndexNamespace::new("search.documents").expect("test namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["term"]).expect("test key definition should be valid"),
            TargetReferenceType::new("source.document")
                .expect("test target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("logical-v1")
                .expect("test consistency requirement should be valid"),
            None,
            None,
        )
        .expect("unconstrained test definition should be valid");

        let base = BuildCandidate::from_parts(
            index_id(0x11),
            definition,
            IndexVersion::with_metadata(
                version_id("index-v1"),
                Some(source_version("source-v1")),
                Some(schema_version("schema-v1")),
                None,
            )
            .expect("test version should be valid"),
            vec![entry("a", "doc:1")],
        );

        let updater = CandidateUpdater::new();

        let updated_candidate = updater
            .create_candidate(
                &base,
                version_id("index-v2"),
                Some(source_version("source-v2")),
                Some(schema_version("schema-v1")),
            )
            .expect("update candidate should be created");

        assert_eq!(base.version().id().as_str(), "index-v1");
        assert_eq!(updated_candidate.version().id().as_str(), "index-v2");
        assert_eq!(updated_candidate.len(), 1);
        assert_eq!(
            updated_candidate
                .version()
                .source_version()
                .expect("source version should exist")
                .as_str(),
            "source-v2"
        );
        assert_eq!(
            updated_candidate
                .version()
                .schema_version()
                .expect("schema version should exist")
                .as_str(),
            "schema-v1"
        );
    }

    #[test]
    fn successful_update_can_be_recorded_after_candidate_change() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();
        let mut journal = UpdateJournal::new();

        let (updated, sequence) = updater
            .apply_and_record(
                &original,
                IndexMutation::Update {
                    target: reference("doc:1"),
                    replacement: entry("b", "doc:1"),
                },
                &mut journal,
            )
            .expect("successful update should be recorded");

        assert_eq!(sequence, UpdateSequence::new(1));
        assert_eq!(journal.len(), 1);
        assert_eq!(updated.entries()[0], entry("b", "doc:1"));
        assert_eq!(original.entries()[0], entry("a", "doc:1"));
    }

    #[test]
    fn failed_update_is_not_recorded_in_the_journal() {
        let original = candidate(Uniqueness::Unique, vec![entry("a", "doc:1")]);
        let updater = CandidateUpdater::new();
        let mut journal = UpdateJournal::new();

        let result = updater.apply_and_record(
            &original,
            IndexMutation::Insert(entry("a", "doc:2")),
            &mut journal,
        );

        assert!(result.is_err());
        assert!(journal.is_empty());
        assert_eq!(journal.current_sequence(), UpdateSequence::INITIAL);
    }

    #[test]
    fn journal_replay_boundary_excludes_the_base_sequence() {
        let mut journal = UpdateJournal::new();
        journal
            .append(IndexMutation::Insert(entry("a", "doc:1")))
            .expect("append should succeed");
        journal
            .append(IndexMutation::Insert(entry("b", "doc:2")))
            .expect("append should succeed");
        journal
            .append(IndexMutation::Delete(DeleteSelector::Target(reference(
                "doc:1",
            ))))
            .expect("append should succeed");

        let mutations: Vec<&IndexMutation> = journal
            .records_after(UpdateSequence::new(1))
            .map(UpdateRecord::mutation)
            .collect();

        assert_eq!(mutations.len(), 2);
        assert!(matches!(mutations[0], IndexMutation::Insert(_)));
        assert!(matches!(mutations[1], IndexMutation::Delete(_)));
    }

    #[test]
    fn update_sequence_does_not_wrap_when_at_maximum() {
        let mut journal = UpdateJournal {
            next_sequence: UpdateSequence::new(u64::MAX),
            records: Vec::new(),
        };

        let error = journal
            .append(IndexMutation::Insert(entry("a", "doc:1")))
            .expect_err("sequence overflow must be rejected");

        assert_eq!(error, UpdateJournalError::SequenceExhausted);
        assert!(journal.is_empty());
        assert_eq!(journal.current_sequence(), UpdateSequence::new(u64::MAX));
    }
}
