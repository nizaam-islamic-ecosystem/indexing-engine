# Scope: Nizaam Indexing Engine

`nizaam-indexing` is the first major Nizaam infrastructure engine. It provides semantic/logical indexing between domain/infrastructure engine semantics and physical storage/query infrastructure. It does not own Quran, Arabic, Hadith, KG, Fiqh, Tafsir, or other domain semantics, and it does not expose physical index technology as part of its architectural contract.

The Indexing Engine is a **Nizaam-level indexing system**, not merely a database-index wrapper. It represents indexed objects through stable identity plus independent logical index spaces for identity, inverted/lexical lookup, relationships, locality/similarity, and other declared dimensions.

---

## Architecture / Boundaries

The authoritative architecture is:

```text
Domain / Infrastructure Engine
        │
        │ semantic mapping / index requirement
        ▼
Nizaam Indexing Engine
        │
        ├── Index Identity
        ├── Index Namespace
        ├── Index Definition
        ├── Index Construction / Update
        ├── Relationship / Mapping Dimensions
        ├── Similarity / Locality
        ├── Query / Retrieval
        ├── Versioning / Consistency
        ├── Lifecycle / Maintenance
        ├── Capacity / Resource Management
        ├── Integrity / Validation
        └── Failure Recovery
        │
        ▼
Physical storage / index implementation
```

The Indexing Engine sits between semantic engines and physical storage. It owns **how declared indexing requirements become efficiently retrievable**; the source engine owns **what the indexed relationship means**.

### Hard boundaries

The Indexing Engine must not own:

- Quran knowledge
- Arabic linguistic semantics
- Hadith knowledge
- KG truth or ontology
- Fiqh/Tafsir/Aqeedah/Seerah semantics
- domain business rules
- domain workflows
- domain-specific storage schemas
- domain-specific ranking policy
- domain-specific inference

The KG architecture establishes that KG mapping families are semantic concepts, predicates carry exact relationship meaning, and Indexing converts those requirements into indexing structures.

The common infrastructure implementation architecture establishes the runtime boundary:

```text
Nizaam Control Plane
        ↓
Infrastructure Engine
        ↓
Engine Runtime
        ↓
Capability Dispatch
        ↓
Infrastructure-local Planner
        ↓
Infrastructure-local Workflow
        ↓
Infrastructure-local Execution
```

The Control Plane coordinates globally; Indexing performs its own local planning and execution. It must not contain a second global scheduler/execution engine.

---

## Established Mapping Taxonomy

The KG architecture freezes **12 semantic mapping families**:

```text
IDENTITY
LEXICAL
LINGUISTIC
SEMANTIC
CONCEPTUAL
HIERARCHICAL
PART_WHOLE
REFERENCE
TEMPORAL
CAUSAL
LOGICAL
KNOWLEDGE
```

These are **not** physical index families. They are semantic organizations of relationships. Predicates, context, qualifiers, direction, evidence, provenance, authority, and status are structures around a mapping rather than additional mapping families.

Examples established by the KG architecture:

| KG mapping | Example predicate | Index family |
| --- | --- | --- |
| Identity | `SAME_AS` | Identity |
| Lexical | `LEMMA_OF` | Inverted |
| Linguistic | `HAS_MORPHOLOGY` | Inverted / Relationship |
| Semantic | `RELATED_TO` | Relationship |
| Conceptual | `RELATED_CONCEPT` | Relationship / Similarity |
| Hierarchical | `SUBCLASS_OF` | Relationship |
| Part-Whole | `PART_OF` | Relationship |
| Reference | `MENTIONED_IN` | Relationship |
| Temporal | `BEFORE` | Relationship |
| Causal | `CAUSES` | Relationship |
| Logical | `IMPLIES` | Relationship |
| Knowledge/Evidence | `SUPPORTED_BY` | Relationship / reference-oriented |
| Similarity search | similarity representation | Similarity |

### Mapping granularity

The KG architecture also establishes a textual mapping hierarchy:

```text
Word Mapping
      ↓
Mention / Occurrence Mapping
      ↓
Sentence / Span Mapping
      ↓
Passage Mapping
      ↓
Document Mapping
      ↓
Cross-source Knowledge Mapping
```

The architecture deliberately prefers **Text Span / Passage Mapping** over making "sentence" the universal unit, because Quran, Hadith, Tafsir, Fiqh, and other sources have different textual boundaries.

Therefore Indexing must be capable of representing separate logical indexing spaces for different mapping requirements, while keeping those spaces independent from physical implementation.

### Logical mapping indexes

For the Indexing Engine implementation, each independently addressable mapping/index space may have its own logical namespace/identifier. The intended model is conceptually:

```text
Word-{id}
Mention-{id}
Sentence-{id}
Span-{id}
Passage-{id}
Document-{id}

Identity-{id}
Lexical-{id}
Linguistic-{id}
Semantic-{id}
Conceptual-{id}
Hierarchical-{id}
PartWhole-{id}
Reference-{id}
Temporal-{id}
Causal-{id}
Logical-{id}
Knowledge-{id}
```

These names are **logical index identifiers/namespaces**, not physical table names, database partitions, or new physical index engines. The Indexing architecture explicitly distinguishes index family, dimension, namespace, and physical implementation.

A single physical index family may back multiple logical spaces. For example, several relationship namespaces may use the Relationship index family. The logical separation exists so that Word, Semantic, Temporal, Reference, etc. mappings cannot accidentally collapse into one undifferentiated semantic space.

---

## Identity Principle

The primary Nizaam Index/Object identity is separate from indexing locality.

The architecture establishes:

```text
Stable Nizaam Object Identity
        ≠
Index Namespace
        ≠
Index Definition
        ≠
Physical Index Implementation
        ≠
Relationship Graph
        ≠
Similarity Structure
```

The logical identity is architected as a fixed-width **256-bit / 32-byte identity**, with the exact human-readable encoding deliberately left open. The identity itself must not imply semantic locality.

An object can participate in many logical index spaces without receiving multiple canonical identities.

---

## Index Families

The foundational physical/logical retrieval families established by the architecture are:

```text
Identity Index
Inverted Index
Relationship Index
Similarity Index
```

The Relationship Index is the main workhorse for typed multidimensional mappings; its dimensions can distinguish semantic, temporal, causal, hierarchical, reference, linguistic, and other relationship spaces. Similarity remains a distinct indexing capability rather than being treated as a generic KG relationship.

The Indexing Engine must not force every mapping family into its own physical implementation.

---

## Index Definition

An `IndexDefinition` is the logical contract describing an index space. The architecture establishes these conceptual fields:

```text
IndexDefinition
├── index_definition_id
├── namespace
├── index_family
├── mapping_dimension
├── predicate_scope
├── key_definition
├── target_reference_type
├── directionality
├── cardinality
├── uniqueness
├── consistency_requirement
├── source_version
├── schema_version
└── lifecycle_state
```

This is an architectural model, not a frozen database schema.

Important distinctions:

```text
Namespace ≠ Object Identity
Namespace ≠ Physical Partition
Predicate ≠ Index Family
Mapping Family ≠ Physical Index
```

Namespaces are logical indexing spaces. Exact namespace syntax, prefix alphabet, registry format, physical partitioning, and automatic namespace generation remain implementation decisions until their phase explicitly freezes them.

---

## Engine Communication

Source engines declare **logical requirements**, not physical algorithms.

They may request:

```text
mapping family
predicate
source/target reference types
directionality
retrieval mode
dimensions
constraints
version requirements
```

They must not request:

```text
B-tree
HNSW
FAISS
Lucene
PostgreSQL GIN
specific database table
specific storage partition
```

The flow is:

```text
Engine
  ↓
Index Requirement
  ↓
Index Definition
  ↓
Index Namespace
  ↓
Index Family
  ↓
Physical implementation
```

The Indexing Engine returns generic references/IDs and retrieval metadata rather than domain objects.

Both synchronous query operations and asynchronous/batched index updates are part of the architecture.

---

## Agent Implementation Rules

### 1. Scope is authoritative

This scope is the authoritative implementation specification for the Indexing Engine.

### 2. Ask before proceeding when requirements are unclear

The agent MUST stop and ask before implementing when:

- requirements are ambiguous;
- two architectural decisions conflict;
- a materially different design is required;
- a deferred technology/provider choice becomes necessary;
- functionality may belong in Core instead of Indexing;
- functionality may belong in another engine;
- a phase boundary is unclear.

Default behavior:

```text
STOP → EXPLAIN → ASK → WAIT → IMPLEMENT
```

### 3. Never silently change architecture

Do not silently:

- move responsibilities between Core and Indexing;
- introduce domain semantics;
- turn indexing mechanisms into domain workflows;
- make physical storage technology part of the public contract;
- collapse distinct mapping spaces;
- replace logical namespaces with physical partitions;
- bypass the engine runtime;
- make the Indexing Engine a second Control Plane.

### 4. Protect previously verified phases

Later phases must preserve earlier contracts, behavior, tests, and architectural boundaries.

### 5. Implement only the current phase

Do not implement future-phase features merely because scaffolding exists or because they seem useful.

### 6. Do not invent deferred implementation choices

Technology choices remain open unless explicitly frozen, including physical database, physical index algorithm, storage provider, serialization technology, provider implementation, partitioning strategy, and exact namespace encoding.

### 7. Core/engine boundary

Indexing consumes Core mechanisms. It must not add Indexing semantics to Core.

### 8. Full regression testing

After implementation, run the complete repository verification required by the active workspace. Focused tests do not replace full regression testing.

### 9. Do not weaken tests to hide failures

A failing test must be investigated against the approved architecture. Tests must not be deleted, skipped, or weakened merely to make the suite pass.

### 10. Compilation is not completion

A phase is complete only after its architecture, implementation, tests, boundaries, and completion criteria are verified.

### 11. Do not modify scope.md without permission

This document is the authoritative scope. Changes to it require explicit approval.

---

# Implementation Phases

The Indexing architecture contains many conceptual sections, but they are intentionally grouped into a smaller number of implementation phases. This keeps implementation manageable while preserving the full architectural sequence.

---

## Phase 0: Engine Workspace & Integration Foundation

### Goal

Establish the Indexing Engine as a Nizaam infrastructure engine using the common infrastructure-engine implementation pattern and the existing Core contracts.

### Planned implementation

```text
Core
 ↓
Indexing Engine Runtime
 ↓
Control Plane integration
 ↓
Capability boundary
 ↓
Indexing-local planner
```

Establish:

- engine startup/registration;
- engine identity and instance identity;
- universal request/response boundary;
- operation/context propagation;
- capability registration;
- lifecycle integration;
- health/observability/error boundaries;
- test harness foundation.

The common infrastructure implementation architecture requires the universal engine boundary, Control Plane integration, capability layer, local planner boundary, context propagation, lifecycle, security, observability, testing, and technology-neutral implementation boundary.

### Planned source files / modules

The initial scaffold for this phase is:

```text
src/
├── lib.rs
├── engine/
│   ├── mod.rs
│   ├── registration.rs
│   ├── runtime.rs
│   └── capability.rs
└── error.rs
```

Primary responsibility by file:

- `src/lib.rs` — crate root and public module boundary.
- `src/engine/mod.rs` — engine-facing module exports and integration boundary.
- `src/engine/registration.rs` — Indexing engine registration and instance identity integration.
- `src/engine/runtime.rs` — engine runtime/bootstrap and lifecycle integration.
- `src/engine/capability.rs` — capability registration/dispatch boundary.
- `src/error.rs` — Indexing-specific error types that belong to the engine rather than Core.

These files are the **initial implementation locations**, not a frozen file-level architecture. During implementation, files may be merged, split, renamed, or moved when the actual design requires it.

### Planned test files

```text
tests/
├── common/
│   ├── mod.rs
│   └── helpers.rs
├── engine.rs
├── integration.rs
└── conformance.rs
```

Phase 0 tests establish:

- engine registration;
- engine instance identity;
- runtime startup/shutdown;
- capability registration;
- universal request/context propagation;
- Core boundary usage;
- Control Plane integration boundary;
- negative tests for bypassing the engine runtime or introducing domain semantics.

`integration.rs` and `conformance.rs` may initially contain only Phase 0 coverage and will be extended by later phases rather than duplicated.

### Boundary

This phase does not implement indexing algorithms, physical storage, or mapping semantics.

### Done when

- Indexing can start/register through the common engine runtime.
- Requests can enter through the universal Core boundary.
- Indexing-local planning is a clear extension point.
- No domain semantics have entered Core or generic runtime code.

---

## Phase 1: Index Identity, Families, Namespaces & Mapping Spaces

### Goal

Implement the foundational indexing identity model and separate logical mapping/index spaces.

### Planned implementation

Implement:

```text
Stable object/index identity
Index family
Index dimension
Index namespace
Index definition identity
Mapping-space registration
```

Support the foundational index families:

```text
Identity
Inverted
Relationship
Similarity
```

Support the KG-derived mapping dimensions/families without turning each into a physical implementation:

```text
Identity
Lexical
Linguistic
Semantic
Conceptual
Hierarchical
Part-Whole
Reference
Temporal
Causal
Logical
Knowledge
```

Support textual/granularity-oriented logical spaces:

```text
Word
Mention / Occurrence
Sentence / Span
Passage
Document
```

### Planned source files / modules

```text
src/
├── identity/
│   ├── mod.rs
│   ├── index.rs
│   ├── namespace.rs
│   └── definition.rs
├── mapping/
│   ├── mod.rs
│   ├── family.rs
│   ├── dimension.rs
│   ├── predicate.rs
│   └── space.rs
└── index/
    └── mod.rs
```

Primary responsibility:

- `identity/` — stable indexing/object identity relationships and index-definition identity.
- `mapping/family.rs` — semantic mapping families and foundational index families.
- `mapping/dimension.rs` — indexing dimensions and relationship dimensions.
- `mapping/predicate.rs` — predicate scope/meaning references without owning domain semantics.
- `mapping/space.rs` — logical mapping/index spaces.
- `identity/namespace.rs` — logical namespace representation and namespace separation.
- `identity/definition.rs` — identity aspects of index definitions.
- `index/mod.rs` — initial public index boundary; concrete index data structures are primarily introduced in Phase 2.

The Phase 1 implementation must represent logical spaces such as `Word-{id}`, `Mention-{id}`, `Span-{id}`, `Passage-{id}`, `Document-{id}`, and the established semantic mapping spaces without making those identifiers physical tables or partitions.

### Planned test files

```text
tests/
├── identity.rs
├── mapping.rs
├── index.rs
└── conformance.rs
```

Coverage includes:

- identity/namespace separation;
- namespace uniqueness and validity;
- index-family vs mapping-family separation;
- mapping-dimension separation;
- Word/Mention/Sentence-or-Span/Passage/Document logical spaces;
- all established KG mapping families;
- multiple logical spaces for one canonical object;
- negative tests preventing namespace-as-identity and namespace-as-physical-partition assumptions.

`index.rs` begins as a boundary test file and may remain small until Phase 2 introduces the full index data model.

### Important decisions

- Canonical object identity is independent of index namespace.
- A single object may participate in many index spaces.
- Separate logical index spaces prevent unrelated mapping semantics from being conflated.
- A namespace is not a physical partition.
- A mapping family is not automatically a physical index family.
- Similarity remains a distinct Indexing capability.

### Done when

The engine can represent and distinguish multiple logical indexing spaces without requiring domain-specific physical implementations.

---

## Phase 2: Index Data Model & Engine Contract

### Goal

Implement the generic data structures through which engines describe, register, populate, and query indexes.

### Planned implementation

Implement conceptual structures for:

```text
IndexDefinition
IndexRequirement
IndexEntry
ObjectReference
RelationshipEntry
SimilarityEntry
IndexVersion
QueryRequest
QueryResult
```

Support:

- predicate scope;
- key definitions;
- target reference types;
- directionality;
- cardinality;
- uniqueness;
- consistency requirements;
- schema/source versions;
- lifecycle metadata.

Support generic reference-only results.

### Planned source files / modules

```text
src/
├── index/
│   ├── mod.rs
│   ├── entry.rs
│   ├── reference.rs
│   ├── relationship.rs
│   ├── similarity.rs
│   └── version.rs
└── requirement/
    ├── mod.rs
    └── requirement.rs
```

Primary responsibility:

- `index/entry.rs` — generic index-entry representation.
- `index/reference.rs` — reference-only object/index targets and retrieval references.
- `index/relationship.rs` — relationship index entries, directionality, cardinality, and dimensions.
- `index/similarity.rs` — generic similarity-index representation without freezing an algorithm.
- `index/version.rs` — index version identity and version metadata.
- `requirement/requirement.rs` — source-engine logical indexing requirements.
- `requirement/mod.rs` — public requirement boundary.

This phase is where the conceptual `IndexDefinition`, `IndexRequirement`, `IndexEntry`, `ObjectReference`, `RelationshipEntry`, `SimilarityEntry`, `IndexVersion`, `QueryRequest`, and `QueryResult` models are expected to become concrete. If implementation proves that one of these concepts belongs in a different file, the scope may be revised deliberately rather than forcing an artificial file boundary.

### Planned test files

```text
tests/
├── requirement.rs
├── index.rs
├── identity.rs
├── mapping.rs
├── integration.rs
└── conformance.rs
```

Coverage includes:

- valid/invalid index definitions;
- index requirements;
- reference-only result boundaries;
- relationship directionality and cardinality;
- similarity separation;
- version metadata;
- source/schema version compatibility;
- generic engine-to-indexing contract;
- rejection of physical algorithm/provider requests;
- negative tests preventing domain-object hydration ownership.

### Boundary

No domain object hydration is owned by Indexing.

### Done when

A KG/domain/infrastructure engine can express an indexing requirement without knowing the physical index implementation.

---

## Phase 3: Index Construction, Update, Batch & Version Publication

### Goal

Implement index creation and update mechanics without corrupting the active index.

### Planned implementation

Support:

```text
create
build
populate
batch update
incremental update
delete/update
rebuild
validate
publish
```

The architecture explicitly separates query operations from build/update operations. A major rebuild must construct and validate a new version before replacing the active version.

### Planned source files / modules

```text
src/
├── build/
│   ├── mod.rs
│   ├── builder.rs
│   ├── update.rs
│   ├── batch.rs
│   ├── rebuild.rs
│   └── publication.rs
├── index/
│   └── version.rs
└── consistency/
    └── versioning.rs
```

Primary responsibility:

- `build/builder.rs` — index construction orchestration.
- `build/update.rs` — incremental create/update/delete operations.
- `build/batch.rs` — bounded batch ingestion/update operations.
- `build/rebuild.rs` — major rebuild workflow.
- `build/publication.rs` — validation-to-publication transition.
- `index/version.rs` — active/candidate index version model.
- `consistency/versioning.rs` — version compatibility and publication consistency rules.

The intended implementation flow is:

```text
source snapshot
    ↓
build candidate
    ↓
validate
    ↓
ready
    ↓
publish
    ↓
active
```

### Planned test files

```text
tests/
├── build.rs
├── consistency.rs
├── index.rs
├── fault_injection.rs
├── stress.rs
├── integration.rs
└── conformance.rs
```

Coverage includes:

- create/build;
- incremental updates;
- batch updates;
- deletion/update behavior;
- rebuild isolation;
- validation before publication;
- atomic/controlled publication;
- failed rebuild preserving the last known-good active version;
- version mismatch handling;
- bounded batch pressure;
- deterministic build/update failure injection;
- concurrent update/rebuild boundaries.

`fault_injection.rs` and `stress.rs` are introduced as phase-aware tests here and expanded in Phase 5/6.

### Required lifecycle concept

```text
source snapshot
   ↓
build new index
   ↓
validate
   ↓
ready
   ↓
publish
   ↓
active
```

The active index must remain usable during major rebuilds.

### Done when

- Incremental updates work within the declared consistency model.
- Batch operations are supported.
- Rebuild does not corrupt the active version.
- New versions can be validated and published safely.

---

## Phase 4: Consistency, Synchronization, Query & Retrieval

### Goal

Implement the query/retrieval model and consistency guarantees.

### Planned implementation

Support generic query forms including:

```text
exact lookup
inverted/text lookup
relationship lookup
neighborhood lookup
similarity lookup
filtered lookup
hybrid retrieval
```

Relationship retrieval must support:

```text
OUT
IN
BIDIRECTIONAL
```

without requiring semantic engines to maintain duplicate physical structures.

Support:

```text
fresh/current index
version-aware query
stale-index behavior
consistency requirements
query capability negotiation
reference-only result retrieval
```

### Planned source files / modules

```text
src/
├── query/
│   ├── mod.rs
│   ├── request.rs
│   ├── result.rs
│   ├── planner.rs
│   └── retrieval.rs
└── consistency/
    ├── mod.rs
    ├── policy.rs
    └── synchronization.rs
```

Primary responsibility:

- `query/request.rs` — semantic query requirements.
- `query/result.rs` — generic reference/retrieval results.
- `query/planner.rs` — Indexing-local query planning and index selection.
- `query/retrieval.rs` — execution/retrieval boundary.
- `consistency/policy.rs` — consistency requirements and stale/fresh behavior.
- `consistency/synchronization.rs` — synchronization between source/index versions.

The planner must select compatible logical index definitions and namespaces without exposing physical algorithms to callers.

### Planned test files

```text
tests/
├── query.rs
├── consistency.rs
├── integration.rs
├── conformance.rs
├── fault_injection.rs
└── stress.rs
```

Coverage includes:

- exact lookup;
- inverted/text lookup;
- relationship lookup;
- neighborhood lookup;
- similarity lookup;
- filtered/hybrid retrieval;
- OUT/IN/BIDIRECTIONAL retrieval;
- fresh/current vs version-aware query;
- stale-index behavior;
- consistency requirements;
- query capability negotiation;
- reference-only results;
- slow/blocked retrieval behavior;
- physical-algorithm leakage negative tests.

### Query planning boundary

The Indexing Engine owns index selection and local query planning.

The caller declares requirements such as:

```text
semantic similarity
language = Arabic
domain = Quran
top_k = 20
```

It must not select HNSW, IVF, Lucene, GIN, or another physical algorithm.

### Done when

Queries can be expressed semantically, resolved to compatible index definitions, executed, and returned as generic references with defined consistency behavior.

---

## Phase 5: Lifecycle, Capacity, Integrity & Failure Recovery

### Goal

Make indexing operationally safe under resource pressure, corruption, stale data, rebuilds, and failures.

### Planned implementation

Implement:

```text
index lifecycle
maintenance
capacity accounting
query pressure
build pressure
throttling
resource limits
integrity validation
trust boundaries
failure classification
recovery
rebuild-based recovery
health/readiness reporting
```

Keep:

```text
Health ≠ Lifecycle
Validation ≠ Storage
Failure ≠ Retry policy
Index state ≠ Engine state
```

### Planned source files / modules

```text
src/
├── lifecycle/
│   ├── mod.rs
│   └── state.rs
├── capacity/
│   ├── mod.rs
│   ├── limits.rs
│   └── accounting.rs
├── integrity/
│   ├── mod.rs
│   └── validation.rs
├── recovery/
│   ├── mod.rs
│   ├── failure.rs
│   └── recovery.rs
└── configuration/
    ├── mod.rs
    └── config.rs
```

Primary responsibility:

- `lifecycle/state.rs` — index-specific lifecycle state.
- `capacity/limits.rs` — resource and operation limits.
- `capacity/accounting.rs` — bounded resource accounting.
- `integrity/validation.rs` — metadata, entry, reference, version, and publication validation.
- `recovery/failure.rs` — failure classification.
- `recovery/recovery.rs` — deterministic recovery strategies.
- `configuration/config.rs` — Indexing-local operational configuration when required by the approved architecture.

Configuration is included here only as an operational dependency; it must not become a mechanism for silently selecting deferred physical technologies.

### Planned test files

```text
tests/
├── lifecycle.rs
├── capacity.rs
├── integrity.rs
├── recovery.rs
├── configuration.rs
├── fault_injection.rs
├── stress.rs
├── integration.rs
└── conformance.rs
```

Coverage includes:

- index lifecycle independent of engine lifecycle;
- capacity limits;
- query/build pressure;
- throttling/bounded behavior;
- integrity validation;
- invalid/stale/unavailable/corrupt index states;
- source-data failures;
- provider/storage failure abstraction;
- resource exhaustion;
- deterministic recovery;
- rebuild-based recovery;
- configuration update behavior;
- negative tests ensuring health does not own lifecycle and retry policy is not silently embedded in failure classification.

### Integrity

Validate:

- index metadata;
- index entries;
- referenced object identities;
- version compatibility;
- rebuild output;
- publication preconditions.

### Failure recovery

Recovery must preserve the distinction between:

```text
invalid index
stale index
unavailable index
corrupt index
source-data failure
resource exhaustion
provider/storage failure
query failure
```

A rebuild may be the final recovery mechanism when an index cannot safely be repaired incrementally.

### Done when

The engine has bounded behavior under pressure and deterministic recovery paths without silently serving invalid index state.

---

## Phase 6: Security, Observability, Conformance & Hardening

### Goal

Complete the Indexing Engine as a production-grade Nizaam infrastructure engine while preserving all established Core boundaries.

### Planned implementation

Verify:

```text
security boundary
authorization
operation context propagation
artifact/provenance integration where required
logging
metrics
tracing
diagnostics
events
health
configuration
Control Plane integration
cross-engine communication
```

Add:

- unit tests for implementation behavior;
- integration tests for public contracts;
- architecture-conformance tests;
- negative/boundary tests;
- deterministic fault-injection tests;
- bounded stress/resource tests;
- end-to-end tests using the real Core runtime.

The common infrastructure plan explicitly requires universal testing and contract/conformance infrastructure as reusable mechanisms, while keeping engine-specific semantics inside the engine.

### Planned source files / modules

```text
src/
├── security/
│   ├── mod.rs
│   └── authorization.rs
├── observability/
│   ├── mod.rs
│   ├── logging.rs
│   ├── metrics.rs
│   ├── tracing.rs
│   └── diagnostics.rs
├── engine/
│   ├── registration.rs
│   ├── runtime.rs
│   └── capability.rs
└── lib.rs
```

Phase 6 primarily **hardens and integrates existing modules** rather than introducing a large new subsystem. `security/` and `observability/` are completed here, while earlier engine/runtime files are updated to prove the final cross-boundary behavior.

Event, health, Control Plane, and Core runtime integrations are verified through their public interfaces; they should not automatically result in Indexing-owned duplicate subsystems.

### Planned test files

```text
tests/
├── security.rs
├── observability.rs
├── conformance.rs
├── integration.rs
├── fault_injection.rs
├── stress.rs
└── e2e.rs
```

Final verification covers:

- authentication/authorization boundary;
- operation/security context propagation;
- logging;
- metrics;
- tracing;
- diagnostics;
- health/readiness integration;
- Control Plane admission/routing integration;
- cross-engine communication;
- configuration;
- artifact/provenance integration where required;
- event integration where required;
- negative architecture-boundary tests;
- deterministic fault injection;
- bounded stress/resource tests;
- complete end-to-end flows using the real Core runtime.

### Final testing policy

Testing is performed continuously during every phase, but **phase-local tests are not the final completion gate**.

At the end of implementation, Phase 6 triggers the full verification pass, following the same principle used for Nizaam Core:

```text
Implementation phase
        ↓
Focused phase tests
        ↓
Fix/regression checks
        ↓
Next phase
        ↓
...
        ↓
Final complete test suite
        ↓
Full regression / conformance / fault / stress / E2E verification
```

The final pass must verify the complete Indexing Engine rather than only the tests added in Phase 6.

### Done when

- Full Indexing test suite passes.
- Core regression suite remains passing.
- Architecture boundaries are executable and verified.
- No domain semantics leak into Indexing or Core.
- Physical index implementation remains replaceable behind the logical contract.

---

# Cross-Phase Architectural Invariants

These invariants apply to every phase.

## 1. Identity separation

```text
Object Identity
≠ Index Identity
≠ Index Namespace
≠ Index Definition Version
≠ Physical Implementation Version
```

## 2. Semantic ownership

```text
KG / Domain Engine
→ defines meaning

Indexing
→ defines retrieval/indexing mechanics

Storage/provider
→ defines physical persistence/implementation
```

## 3. Mapping separation

```text
Mapping Family
≠ Predicate
≠ Index Family
≠ Index Namespace
≠ Physical Partition
```

## 4. Text granularity

Indexing must be capable of separate logical spaces for:

```text
Word
Mention / Occurrence
Sentence / Span
Passage
Document
```

without assuming every source has identical textual boundaries.

## 5. Relationship dimensions

The relationship indexing model must be capable of distinguishing:

```text
Lexical
Linguistic
Semantic
Conceptual
Hierarchical
Part-Whole
Reference
Temporal
Causal
Logical
Knowledge
```

rather than collapsing all relationships into a single generic neighborhood.

## 6. Similarity separation

Similarity retrieval is an Indexing capability. A KG `SIMILAR_TO` relationship, when semantically defined, remains KG knowledge and must not be confused with the Indexing similarity structure.

## 7. Physical implementation isolation

Callers must not depend on:

```text
B-tree
HNSW
IVF
FST
Lucene
FAISS
PostgreSQL
Neo4j
MongoDB
```

or any other concrete implementation unless a later approved phase explicitly freezes such a choice.

## 8. Control Plane boundary

```text
Control Plane
→ global coordination / admission / routing

Indexing Engine
→ local indexing planning and execution
```

The Indexing Engine must never become a second Control Plane.

## 9. Reference-only boundary

Indexing returns object/index references and retrieval metadata. It does not silently become the owner of domain object storage or semantic truth.

## 10. Version safety

```text
Active Index
   ↓
remains usable
   ↓
New Index Version Built
   ↓
Validated
   ↓
Published
```

A failed rebuild must not destroy the last known-good active index.

---

# Explicitly Deferred Decisions

The following remain open unless a later phase explicitly freezes them:

```text
Exact namespace/prefix syntax
Exact `Word-{id}` wire encoding
Exact binary/string representation
Namespace registry storage
Physical database
Physical index implementation
Partitioning strategy
Sharding strategy
Compression implementation
Embedding/vector provider
Similarity algorithm
FST implementation
HNSW/IVF or alternative implementation
Persistence provider
External indexing service
Serialization technology
Provider deployment topology
```

The architecture explicitly says exact namespace strings and physical implementation should remain open until the logical definition is complete.

---

# Completion Criteria

The Indexing Engine is complete only when:

- all approved implementation phases are implemented;
- all Indexing unit tests pass;
- all Indexing integration/conformance tests pass;
- full workspace regression tests pass;
- formatting and linting requirements pass;
- Index identity remains separate from physical indexing;
- logical mapping spaces remain distinguishable;
- all established KG mapping dimensions are representable;
- Word/Mention/Span/Passage/Document granularity is representable;
- relationship directionality and cardinality are supported;
- index versions can be rebuilt and published safely;
- active indexes remain usable during rebuild;
- stale/corrupt/unavailable states are handled explicitly;
- resource limits remain bounded;
- failure recovery is deterministic;
- Indexing does not acquire KG/domain semantics;
- Indexing does not acquire a second Control Plane;
- physical index technology remains behind the logical contract;
- no unauthorized architectural decisions were introduced.

---

# Current State

Architecture is established sufficiently to begin implementation.

The next implementation work should begin with **Phase 0: Engine Workspace & Integration Foundation**, followed by the identity/namespace/index-space model.

The KG architecture provides the semantic mapping taxonomy that Indexing must consume, while the common infrastructure implementation architecture provides the runtime shape that Indexing must follow.

The Indexing Engine should therefore be implemented as a **generic infrastructure engine**, not as a KG-specific indexing layer.
