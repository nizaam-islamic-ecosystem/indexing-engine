# Scope: Nizaam Indexing Engine

`nizaam-indexing` is the first major Nizaam infrastructure engine. It provides semantic/logical indexing between domain/infrastructure engine semantics and physical storage/query infrastructure. It does not own Quran, Arabic, Hadith, KG, Fiqh, Tafsir, or other domain semantics, and it does not expose physical index technology as part of its architectural contract.

The Indexing Engine is a **Nizaam-level indexing system**, not merely a database-index wrapper. It represents indexed objects through stable identity plus independent logical index spaces for identity, inverted/lexical lookup, relationships, locality/similarity, and other declared dimensions. fileciteturn100file0L3-L27

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

The Indexing Engine sits between semantic engines and physical storage. It owns **how declared indexing requirements become efficiently retrievable**; the source engine owns **what the indexed relationship means**. fileciteturn100file1L5-L8

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

The KG architecture establishes that KG mapping families are semantic concepts, predicates carry exact relationship meaning, and Indexing converts those requirements into indexing structures. fileciteturn100file0L54247-L54275

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

The Control Plane coordinates globally; Indexing performs its own local planning and execution. It must not contain a second global scheduler/execution engine. fileciteturn100file1L145-L183

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

These are **not** physical index families. They are semantic organizations of relationships. Predicates, context, qualifiers, direction, evidence, provenance, authority, and status are structures around a mapping rather than additional mapping families. fileciteturn100file0L54349-L54417

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

fileciteturn100file0L54257-L54275

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

The architecture deliberately prefers **Text Span / Passage Mapping** over making "sentence" the universal unit, because Quran, Hadith, Tafsir, Fiqh, and other sources have different textual boundaries. fileciteturn100file0L46442-L46496

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

These names are **logical index identifiers/namespaces**, not physical table names, database partitions, or new physical index engines. The Indexing architecture explicitly distinguishes index family, dimension, namespace, and physical implementation. fileciteturn100file0L18575-L18646

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

The logical identity is architected as a fixed-width **256-bit / 32-byte identity**, with the exact human-readable encoding deliberately left open. The identity itself must not imply semantic locality. fileciteturn100file0L448-L525

An object can participate in many logical index spaces without receiving multiple canonical identities. fileciteturn100file0L18539-L18571

---

## Index Families

The foundational physical/logical retrieval families established by the architecture are:

```text
Identity Index
Inverted Index
Relationship Index
Similarity Index
```

The Relationship Index is the main workhorse for typed multidimensional mappings; its dimensions can distinguish semantic, temporal, causal, hierarchical, reference, linguistic, and other relationship spaces. Similarity remains a distinct indexing capability rather than being treated as a generic KG relationship. fileciteturn100file0L18424-L18474

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

This is an architectural model, not a frozen database schema. fileciteturn100file0L18650-L18676

Important distinctions:

```text
Namespace ≠ Object Identity
Namespace ≠ Physical Partition
Predicate ≠ Index Family
Mapping Family ≠ Physical Index
```

Namespaces are logical indexing spaces. Exact namespace syntax, prefix alphabet, registry format, physical partitioning, and automatic namespace generation remain implementation decisions until their phase explicitly freezes them. fileciteturn100file0L19004-L19049

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

The Indexing Engine returns generic references/IDs and retrieval metadata rather than domain objects. fileciteturn100file1L399-L434

Both synchronous query operations and asynchronous/batched index updates are part of the architecture. fileciteturn100file1L529-L571

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

The common infrastructure implementation architecture requires the universal engine boundary, Control Plane integration, capability layer, local planner boundary, context propagation, lifecycle, security, observability, testing, and technology-neutral implementation boundary. fileciteturn100file1L12-L43

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

The architecture explicitly separates query operations from build/update operations. A major rebuild must construct and validate a new version before replacing the active version. fileciteturn100file1L529-L571

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

The common infrastructure plan explicitly requires universal testing and contract/conformance infrastructure as reusable mechanisms, while keeping engine-specific semantics inside the engine. fileciteturn100file1L664-L701

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

The architecture explicitly says exact namespace strings and physical implementation should remain open until the logical definition is complete. fileciteturn100file0L19037-L19049

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

The KG architecture provides the semantic mapping taxonomy that Indexing must consume, while the common infrastructure implementation architecture provides the runtime shape that Indexing must follow. fileciteturn100file0L54349-L54417 fileciteturn100file1L12-L43

The Indexing Engine should therefore be implemented as a **generic infrastructure engine**, not as a KG-specific indexing layer.
