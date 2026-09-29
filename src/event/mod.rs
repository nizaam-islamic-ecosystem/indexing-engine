//! Phase 5 event boundary for the Indexing Engine.
//!
//! This module is intentionally a thin composition facade over the two typed
//! event-boundary contracts:
//!
//! ```text
//! Core UniversalRequest
//!        |
//!        v
//!   IndexEvent
//!        |
//!        v
//!   Indexing capability
//!        |
//!        v
//! IndexEventResponse
//!        |
//!        v
//! Core UniversalResponse
//! ```
//!
//! [`IndexEvent`] owns only Indexing-specific logical event content while Core
//! remains the owner of universal request/event identity, operation/correlation
//! context, transport metadata, and the request boundary. [`IndexEventResponse`]
//! similarly contains only Indexing-owned result content; the surrounding Core
//! capability layer remains responsible for the `UniversalResponse` envelope,
//! response interaction, identity, transport, retry, cancellation, and deadline
//! semantics.
//!
//! No second event protocol, response protocol, correlation mechanism, retry
//! mechanism, or physical-provider instruction boundary is introduced here.

pub mod index_event;
pub mod index_event_response;

pub use index_event::{
    EntityType, EntityTypeValidationError, IndexEvent, IndexEventResult, IndexEventValidationError,
};

pub use index_event_response::IndexEventResponse;
