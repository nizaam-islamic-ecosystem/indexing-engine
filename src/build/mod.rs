//! Public build orchestration boundary for Phase 3.
//!
//! This module composes the five logical build subsystems introduced by Phase 3:
//!
//! - [`builder`] constructs isolated logical candidates from source snapshots.
//! - [`update`] applies incremental logical mutations and records their journal
//!   sequence.
//! - [`batch`] executes bounded transactional mutation chunks.
//! - [`rebuild`] reconstructs an isolated candidate and replays concurrent
//!   logical updates to a consistency point.
//! - [`publication`] validates and prepares the explicit `READY → ACTIVE`
//!   publication boundary.
//!
//! The module root re-exports the public contracts of those child modules so
//! callers can use a single `build` boundary without depending on the internal
//! file layout. The child modules remain public for explicit submodule access.
//!
//! This boundary owns no additional build state, lifecycle registry,
//! synchronization primitive, provider implementation, storage, query
//! execution, or runtime. Active-version ownership remains with the later
//! Indexing lifecycle coordination layer, while Core remains responsible for
//! execution context, cancellation, deadlines, and capability/runtime
//! mechanisms.
//!
//! The module-level tests intentionally exercise interactions across the child
//! modules rather than duplicating their file-local unit tests. They therefore
//! serve as Phase 3 Level 2 tests for the public build boundary.

pub mod batch;
pub mod builder;
pub mod publication;
pub mod rebuild;
pub mod update;

pub use batch::{
    BatchChunkError, BatchError, BatchExecutor, BatchOptions, BatchOptionsError, BatchResult,
};
pub use builder::{BuildCandidate, BuildError, BuildInput, BuildSnapshot, IndexBuilder};
pub use publication::{IndexPublisher, PublicationCandidate, PublicationError, PublicationResult};
pub use rebuild::{IndexRebuilder, RebuildError, RebuildInput, RebuildProgress, RebuildResult};
pub use update::{
    CandidateUpdater, DeleteSelector, IndexMutation, UpdateError, UpdateJournal,
    UpdateJournalError, UpdateRecord, UpdateSequence,
};
