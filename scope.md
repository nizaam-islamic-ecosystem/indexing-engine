# Scope: Nizaam Indexing Engine

nizaam-indexing is a Nizaam infrastructure engine responsible for
creating, maintaining, and retrieving stable indexes for objects and
records supplied by domain/infrastructure engines. It does not own Quran,
Arabic, Hadith, KG, Fiqh, Tafsir, or other domain semantics, and it does
not expose a physical index technology as part of its architectural
contract.

The Indexing Engine is a Nizaam-level indexing system, not merely a
database-index wrapper. Its job is to assign an index to an indexable
object or record and make that index efficiently retrievable. The
semantic meaning of that object or record remains owned by the source
engine.

## Architecture / Boundaries

The authoritative architecture is:

```text

Domain / Infrastructure Engine

    │

    │ index requirement / indexable object

    ▼

Nizaam Indexing Engine

    │

    ├── Index Identity

    ├── Index Namespace

    ├── Index Definition

    ├── Index Generation / Construction

    ├── Index Update / Maintenance

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

The Indexing Engine sits between source engines and physical storage. It
owns how an indexable object or record becomes efficiently retrievable;
the source engine owns what that object or record means.

A source engine may provide words, mentions, spans, passages, documents,
semantic records, contextual records, relationship records, or other
engine-defined data. Indexing assigns and serves indexes for those
records. It does not define their semantic relationships.

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

- semantic mapping creation

- semantic predicate meaning

- domain relationship interpretation

The KG/domain engine establishes the meaning of mappings, predicates,
context, qualifiers, direction, evidence, provenance, authority, and
status. Indexing may index the resulting objects or mapping records, but
does not define or create those semantics.

The common infrastructure implementation architecture establishes the
runtime boundary:

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

The Control Plane coordinates globally; Indexing performs its own local
planning and execution. It must not contain a second global
scheduler/execution engine.

## Source Semantic Taxonomy Boundary

The KG architecture defines semantic mapping families and textual
granularity, including concepts such as identity, lexical, linguistic,
semantic, conceptual, hierarchical, part-whole, reference, temporal,
causal, logical, and knowledge relationships, together with word,
mention/occurrence, span, passage, and document granularity.

These concepts are source-engine semantics, not Indexing-owned models.

The Indexing Engine must therefore not create a mapping/ module whose
purpose is to model or define those semantic families, predicates,
mapping dimensions, or relationship meanings.

A KG or another source engine may submit any such object or record to
Indexing for indexing. Indexing treats the semantic payload as
source-owned input and produces the corresponding index according to the
applicable IndexDefinition.

Conceptually:

```text

Source engine

↓

Semantic object / mapping record

↓

Index requirement

↓

Indexing Engine

↓

Index

```

The distinction is:

```text

KG / Domain Engine
→ defines meaning and relationships

Indexing Engine
→ assigns, stores, maintains, and retrieves indexes

Physical provider
→ implements physical persistence/retrieval

```

Logical index namespaces may still distinguish categories such as
Word, Mention, Span, Passage, Document, Semantic, Temporal,
or Knowledge, but those labels identify where/how a record is
indexed, not semantic models owned by Indexing.

## Identity Principle

The primary Nizaam object identity is separate from indexing locality.

The architecture establishes:

```text

Stable Nizaam Object Identity

    ≠

Index Identity

    ≠

Index Namespace

    ≠

Index Definition

    ≠

Physical Index Implementation

    ≠

Semantic Relationship Graph

```

The canonical object identity remains independent of index namespace.
An object or record may participate in many logical index spaces without
receiving multiple canonical identities.

The exact identity width and human-readable representation are separate
decisions from the index-generation mechanism.

## Index Families

The foundational retrieval families are:

```text

Identity Index

Inverted Index

Relationship Index

Similarity Index

```

These are indexing/retrieval structures, not semantic mapping
families.

A Relationship Index, when used, stores and retrieves relationship
records supplied by a source engine. Indexing does not decide what the
relationship means, which predicate is valid, or what semantic inference
should follow from it.

Similarly, a Similarity Index is an indexing capability. The source
engine remains responsible for the meaning of any semantic similarity
record or relationship.

The Indexing Engine must not force every semantic source category into a
separate physical implementation.

## Index Definition

An IndexDefinition is the logical contract describing an index
space. It establishes how a source-declared indexable record is to be
identified and retrieved without owning the record's domain semantics.

The conceptual fields are:

```text

IndexDefinition

├── index_definition_id

├── namespace

├── index_family

├── key_definition

├── target_reference_type

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

Semantic Predicate ≠ Index Family

Semantic Mapping ≠ Index Definition

Index Definition ≠ Physical Implementation

```

Namespaces are logical indexing spaces. Exact namespace syntax, registry
format, physical partitioning, hash representation, and other
implementation details remain decisions for the phases that explicitly
freeze them.

## Engine Communication

Source engines declare logical indexing requirements, not physical
algorithms and not semantic definitions owned by Indexing.

They may provide or request:

```text

indexable object/record category

logical namespace

key material / key definition

target reference type

retrieval mode

constraints

version requirements

consistency requirements

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

Source Engine

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

The Indexing Engine returns generic index/object references and retrieval
metadata rather than domain objects.

The Indexing Engine may index semantic mapping records, but it must not
become the owner of the semantic mapping itself.

## Agent Implementation Rules

These rules are mandatory for any agent working on the Nizaam Indexing

Engine.

### 1. Scope is authoritative

This scope is the authoritative implementation specification for the

Nizaam Indexing Engine.

The agent must follow the architecture, boundaries, phase order,

decisions,

constraints, and completion criteria defined in this document.

The agent must not silently reinterpret or replace architectural

decisions

with its own preferred design.

### 2. Ask before proceeding when requirements are unclear

The agent MUST ask questions before implementing anything whenever:

- a requirement is ambiguous or incomplete;

- two requirements appear to conflict;

- the implementation requires an architectural decision that is not

already

  defined in this scope;

- the agent needs to choose between multiple materially different

designs;

- the agent believes an existing architectural decision should be

changed;

- a previous phase must be modified in a way that could affect its

contract

  or behavior;

- the agent needs to introduce a dependency, provider, runtime,

transport,

  serialization format, storage mechanism, or other implementation

choice

  that has not been authorized;

- the agent discovers that the current phase cannot be implemented

correctly

  without changing the scope;

- the agent is unsure whether functionality belongs in Core or in an

engine;

- the agent is unsure whether functionality belongs to the current

phase

  or a later phase.

The agent MUST NOT guess in these situations.

The agent must explain the ambiguity, present the relevant options when

appropriate, and wait for explicit user direction.

### 3. Permission is required to work against these rules

If the agent determines that correct implementation requires violating,

bypassing, weakening, changing, or extending any rule, boundary, or

architectural decision in this scope, it MUST STOP and ask for explicit

permission before making that change.

The agent must clearly state:

1. Which rule, boundary, or decision would be affected.

2. Why the current implementation cannot proceed without changing

it.

1. What change the agent proposes.

2. What possible consequences the change may have.

The agent must not proceed until the user explicitly approves the

change.

### 4. Never silently change architecture

The agent must never silently:

- redesign an existing system;

- move responsibilities between Core and engines;

- introduce new Core responsibilities;

- change a verified phase's intended behavior;

- change public contracts;

- change dependency direction;

- introduce domain semantics into Core;

- turn a mechanism into a workflow or policy;

- implement functionality belonging to a later phase;

- remove an architectural boundary because it makes implementation

easier.

If such a change appears necessary, the agent must stop and ask

permission.

### 5. Protect previously verified phases

A previously verified phase is considered a stable foundation.

When implementing a new phase, the agent must preserve the behavior,

contracts, boundaries, and tests of all previously verified phases.

The agent must not rewrite, remove, weaken, or redesign previous-phase

functionality merely to simplify the current phase.

If modification of a previous phase is genuinely required by the

approved

architecture, the agent must explain why and obtain permission before

making

the change.

### 6. Implement only the current phase

The agent must implement the current phase being worked on.

The agent must not implement functionality from future phases merely

because:

- the required files already exist as scaffolding;

- the functionality appears useful;

- the functionality makes the current implementation easier;

- the agent believes it should be implemented earlier.

Future-phase functionality remains deferred unless this scope explicitly

requires it for the current phase.

Existing scaffolding does not mean that a phase is implemented.

### 7. Preserve Core and engine boundaries

Core provides shared contracts and mechanisms.

Core must not acquire:

- domain entities;

- domain workflows;

- domain business rules;

- domain policies;

- domain algorithms;

- engine-specific semantics;

- engine-specific storage;

- engine-specific payload meaning;

- engine-specific methodologies.

If the agent believes something should be added to Core but it may

contain

domain or engine semantics, the agent must stop and ask before

implementing it.

### 8. Do not invent deferred implementation choices

When this scope deliberately leaves a choice open, the agent must not

silently choose a technology or provider unless the choice is necessary

and authorized.

This includes, where applicable:

- async runtime;

- transport implementation;

- serialization format;

- authentication provider;

- storage provider;

- persistence mechanism;

- observability vendor;

- external service;

- crate splitting;

- infrastructure provider.

If a deferred choice becomes necessary for the current phase, the agent

must explain the choice and ask for permission before committing to it.

### 9. Full regression testing is mandatory

After implementing or modifying any phase, the agent MUST run the

complete

Core test suite from the repository root:

    cargo test

The agent must NOT test only the current phase.

Tests belonging to all previously implemented phases are mandatory

regression tests and must continue to pass.

A current phase must not be declared complete if previously passing

tests

are failing.

If a regression occurs, the agent must investigate and resolve it before

declaring the current phase complete.

### 10. Do not modify tests merely to hide regressions

The agent must not modify, remove, weaken, skip, or delete an existing

test

simply because the current implementation causes it to fail.

If an existing test conflicts with an approved architectural change, the

agent must stop and ask the user before changing the test or its

expected

behavior.

#### 10.1. Unit and integration testing requirements

Every implementation source file must have unit tests covering the

behavior

of each testable function, method, constructor, validation path, and

relevant

error path defined in that file.

Unit tests should remain close to the implementation and may be placed

in the

corresponding source module using Rust's `#[cfg(test)]` test

modules.

The repository-level `tests/` directory must contain

integration tests for

the public Core API and cross-module behavior.

Unit tests verify individual implementation behavior.

Integration tests verify that multiple Core modules and public APIs work

together correctly from the perspective of a downstream consumer.

Both unit and integration tests are mandatory. Neither replaces the

other.

The agent must add or update appropriate unit and integration tests

whenever

it adds or modifies behavior.

The complete test suite must be executed with:

    cargo test --workspace

This command must be treated as the standard verification command for

both

unit and integration tests.

The agent must not consider a function, file, or phase adequately tested

merely because another unrelated integration test happens to pass.

### 10.1.1. Module-level testing through `mod.rs`

Every Indexing Engine module folder that contains multiple

implementation source files

must use its `mod.rs` as the module's shared internal test

surface.

Each implementation source file must contain its own unit tests for its

testable behavior.

The corresponding `mod.rs` may additionally contain unit tests

that exercise

the interaction between multiple implementation files within the same

module.

These module-level tests are still unit tests because they verify

behavior

within a single Core module and do not represent downstream consumer

usage.

The `mod.rs` file must not become a replacement for

source-file unit tests.

Source-file behavior must remain covered by tests close to its

implementation.

Repository-level integration tests under `tests/` must be used

to verify

public API behavior, cross-module interactions, feature-level behavior,

and

end-to-end Indexing Engine flows from the perspective of a downstream

consumer.

### 11. Compilation is not completion

The agent must not consider a phase complete merely because:

- the project compiles;

- `cargo check` passes;

- the current phase's tests pass;

- the required files exist;

- the implementation appears logically correct.

Before declaring a phase complete, the agent must verify the phase

against:

- Goal;

- Planned implementation;

- Files and folders;

- Boundary;

- Done when;

- Checklist;

- Existing regression tests.

### 12. Full verification after every implementation

The minimum completion verification for every implementation phase is:

```bash

   set -o pipefail
   {
      cargo fmt --all

      cargo fmt --all --check &&

      cargo clippy --workspace --all-targets -- -D warnings &&

      cargo build --workspace &&

      cargo check --workspace &&

      cargo test --workspace --all-targets &&

      cargo test --workspace --doc

     } 2>&1 | tee cargo-check.log

```

These checks must be run from the repository root.

The agent must verify the complete existing test suite, formatting, and

Clippy checks after implementing or modifying any phase.

The agent must NOT test, format-check, or lint only the current phase.

All previously implemented phases are included in the required

regression

verification.

A phase must not be declared complete if any of these checks fail.

The agent may run additional focused tests or verification commands when

appropriate, but focused checks do not replace these complete

repository-wide

checks.

### 13. Ask questions instead of guessing

When uncertain, the default behavior is:

    STOP → EXPLAIN → ASK → WAIT → IMPLEMENT

The agent must not use:

    GUESS → IMPLEMENT → HOPE

This rule applies especially to architectural, API, ownership,

dependency,

boundary, and phase-scope decisions.

### 14. Phase completion requires explicit verification

Before reporting that a phase is complete, the agent must verify that:

- the implementation matches this scope;

- no unauthorized architectural changes were introduced;

- previously verified phases remain intact;

- the complete `cargo test` suite passes;

- the current phase's requirements are satisfied;

- no future-phase functionality was accidentally implemented as part

of

  the current phase.

If any of these conditions cannot be satisfied, the agent must not claim

the phase is complete.

### 15. User approval overrides implementation convenience

Implementation convenience is never sufficient justification for

breaking

an architectural rule.

If following the scope makes implementation harder, the agent must

follow

the scope.

If the agent believes the scope itself needs to change, it must ask the

user for permission before changing the scope or implementing against

it.

### 16. Do not modify scope.md without permission

The agent must not modify `scope.md` as part of normal

implementation.

If the agent believes the scope is incorrect, incomplete, contradictory,

or needs clarification, it must report the issue and ask the user for

permission before changing the scope.

### 17. Completion report

When a phase is completed, the agent should report:

- what was implemented;

- what files were changed;

- what tests were added or updated;

- the result of the full `cargo test`;

- any other verification performed;

- whether any previous-phase files or behavior were modified;

- whether any architectural decisions required user approval.

The agent must clearly state if anything remains unresolved.

## Implementation Phases

The Indexing architecture contains many conceptual sections, but they

are intentionally grouped into a smaller number of implementation

phases. This keeps implementation manageable while preserving the full

architectural sequence.

### Phase 0: Engine Workspace & Integration Foundation

#### Status

**Completed**

##### Goal

Establish `nizaam-indexing` as a valid Nizaam infrastructure engine using the
published `nizaam-core` crate and the universal infrastructure-engine
implementation architecture.

Phase 0 is the **engine shell and integration foundation**. It must make the
Indexing Engine structurally capable of participating in Nizaam, receiving a
universal request, associating the correct execution context, resolving and
dispatching a capability, and completing lifecycle/shutdown behavior through
Core mechanisms.

Phase 0 does **not** implement the actual indexing system yet.

The Indexing Engine's later responsibility is to assign, maintain, and retrieve
indexes for source-engine-owned objects or records. Semantic meaning remains
outside Indexing.

---

##### Architectural Position

The universal infrastructure-engine implementation architecture is:

```text
Nizaam / Control Plane
        ↓
Universal Engine Contract
        ↓
Engine Runtime
        ↓
Capability Dispatch
        ↓
Engine-local Planner
        ↓
Engine-local Workflow
        ↓
Engine-local Services / Execution
        ↓
Universal Result
```

For Indexing, Phase 0 establishes only the engine-side foundation of this
structure:

```text
Nizaam / Control Plane
        ↓
Indexing Engine
        ↓
Core Engine Runtime
        ↓
Core Capability Infrastructure
        ↓
Indexing-local planner boundary
        ↓
Indexing capability execution boundary
```

The universal infrastructure plan deliberately leaves the internals below the
runtime boundary to the individual engine. Indexing therefore uses the common
Nizaam runtime mechanisms but retains ownership of its own future planner,
workflow, index logic, and execution semantics.

---

##### Core Boundary

The current Core codebase already provides the domain-neutral mechanisms that
Phase 0 needs, including:

```text
Identity
Contracts
UniversalRequest / UniversalResponse
OperationContext
EngineContext
Lifecycle
EngineRuntime
CapabilityRegistry
CapabilityHandler
Capability dispatch
ExecutionPipeline
SecurityContext
Health / readiness observations
Logging / observability
Error system
Control Plane / registration mechanisms
```

Phase 0 must **reuse these Core mechanisms** rather than creating competing
Indexing-local versions of them.

The Core report establishes that:

```text
EngineId         = logical engine identity
EngineInstanceId = concrete running instance

OperationId      = logical operation identity
AttemptId        = concrete execution attempt

MessageId        = logical message identity
EventId          = event occurrence identity

CapabilityId     = capability identity
```

These roles remain separate in the Indexing Engine.

The Core runtime also owns lifecycle validity, request admission, execution
coordination, context propagation, capability dispatch, and shutdown
coordination. Indexing must not recreate those responsibilities locally.

---

##### Phase 0 Scope

Phase 0 establishes the following:

```text
1. Package/library integration
2. Indexing engine identity
3. Engine registration boundary
4. Engine runtime integration
5. Lifecycle startup/shutdown boundary
6. Capability registration/dispatch boundary
7. Universal request/response integration
8. Operation/EngineContext propagation
9. Runtime admission boundary
10. Control Plane integration boundary
11. Health/readiness/observability/error integration points
12. Indexing-local planner extension point
13. Test harness foundation
```

The implementation should be minimal but real: no placeholder architecture that
is only made to compile.

---

#### Standalone Binary Decision

The repository currently contains:

```text
src/main.rs
src/bin/nizaam-indexing.rs
```

Phase 0 intentionally does **not** implement either file.

The final standalone executable structure will be decided only after the
Indexing implementation phases have established the engine's real capabilities,
runtime behavior, operational needs, and deployment requirements.

The package may ultimately contain:

```text
library target
+
one or more binary targets
```

but Phase 0 must not prematurely freeze:

```text
src/main.rs
vs
src/bin/nizaam-indexing.rs
```

as the permanent executable arrangement.

Until the post-implementation executable review, these files are treated as
**scaffolding outside Phase 0 ownership**.

Phase 0 therefore does not:

- add engine logic to `src/main.rs`;
- add engine logic to `src/bin/nizaam-indexing.rs`;
- duplicate the same startup logic in both binaries;
- delete either file solely for convenience;
- decide the final number of binaries.

After the Indexing implementation phases are complete, the executable surface
will be reviewed against the actual engine API and deployment requirements.

---

#### Planned Source Files / Modules

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

These are **initial implementation locations**, not immutable file-level
architecture. A file may later be split, merged, moved, or renamed when the
actual implementation demonstrates a better boundary, provided the phase
contract and architecture remain intact.

---

##### `src/lib.rs`

###### Responsibility

`lib.rs` is the reusable library crate root.

It defines the public Indexing Engine crate boundary and exposes the Phase 0
engine-facing API required by downstream Nizaam code and by the eventual
standalone executable.

Conceptually:

```text
nizaam_indexing
    └── engine
        ├── registration
        ├── runtime
        └── capability
```

###### Phase 0 logic

`lib.rs` should:

- declare the Phase 0 modules;
- re-export only the public Indexing Engine types that are intentionally part
  of the library boundary;
- depend on `nizaam-core` for shared infrastructure;
- avoid embedding runtime implementation directly in the crate root.

It should not contain:

```text
index generation
hashing
storage
query implementation
semantic mappings
domain objects
```

The crate root remains an API boundary, not a miscellaneous implementation
file.

---

#### `src/engine/mod.rs`

##### Responsibility

`engine/mod.rs` assembles the Indexing Engine's Phase 0 components into one
coherent engine-facing abstraction.

It is the composition point between:

```text
Core runtime
Core registration
Core capability infrastructure
Indexing engine-specific bootstrap
```

###### Phase 0 logic

The module should conceptually provide an Indexing Engine object/facade that
owns or coordinates:

```text
engine identity
engine instance identity
runtime
capability registry / capability integration
registration state
```

The exact struct field layout is implementation-dependent.

The important invariant is that the Indexing Engine has **one coherent runtime
boundary** rather than unrelated registration/runtime/capability objects being
exposed as disconnected pieces.

###### It should coordinate

```text
construct engine
    ↓
prepare runtime
    ↓
prepare registration
    ↓
prepare capabilities
    ↓
startup
    ↓
ready
    ↓
serving
    ↓
shutdown
```

###### It must not perform

```text
index hashing
index creation
index storage
query planning
semantic relationship creation
KG processing
```

Those responsibilities belong to later Indexing phases or to their owning
source engines.

---

#### `src/engine/registration.rs`

##### Responsibility

This module owns Indexing's engine-level registration integration.

It answers:

> "How does this Indexing Engine identify and register itself as a Nizaam
> engine?"

###### Phase 0 logic

The module should establish the separation between:

```text
EngineId
EngineInstanceId
Engine metadata
Capability metadata
Registration state
```

The registration path should use the Core registration/registry mechanisms
rather than introducing another Indexing-specific global registry.

Conceptually:

```text
Indexing Engine
      ↓
EngineId
EngineInstanceId
      ↓
Core registration mechanism
      ↓
Nizaam engine metadata
```

Registration must remain distinct from capability registration.

```text
Engine registration
    =
"This engine instance participates in Nizaam."

Capability registration
    =
"This engine exposes these capabilities."
```

###### Phase 0 registration guarantees

The implementation should be able to prove that:

- the logical Indexing `EngineId` is stable for the engine definition;
- a concrete `EngineInstanceId` identifies a running instance;
- registration does not confuse engine identity with instance identity;
- registration is performed before the engine is considered fully ready;
- registration failure prevents successful readiness/serving;
- repeated invalid registration does not silently produce duplicate or corrupt
  registration state.

###### Control Plane boundary

Phase 0 may integrate with the existing Core Control Plane registration and
metadata mechanisms, but it must not implement:

```text
global routing
destination selection
eligibility policy
provider selection
replanning
global scheduling
```

Those remain Nizaam-wide coordination responsibilities.

---

#### `src/engine/runtime.rs`

##### Responsibility

This is the main Phase 0 integration module.

It adapts the Indexing Engine to the established Core `EngineRuntime`
mechanisms.

The runtime is **not a second runtime implementation**.

###### Core-owned mechanisms

The Core runtime already defines the operational runtime boundary, including:

```text
lifecycle validity
request admission
execution coordination
context propagation
capability dispatch
shutdown coordination
```

Indexing's `runtime.rs` should compose with those mechanisms.

###### Phase 0 lifecycle

The established lifecycle model is:

```text
Created
  ↓
Starting
  ↓
Configuring
  ↓
Dependencies
  ↓
Capabilities
  ↓
Registering
  ↓
Ready
  ↓
Serving
  ↓
Draining
  ↓
Stopped
```

Phase 0 does not need to implement substantial work for every future stage.
It must establish the engine's participation in the lifecycle and preserve the
Core lifecycle contract.

###### Required lifecycle behavior

The Indexing runtime must be capable of:

```text
construction
    ↓
startup
    ↓
required Phase 0 initialization
    ↓
registration
    ↓
READY
    ↓
explicit transition to SERVING
```

and later:

```text
SERVING
    ↓
DRAINING
    ↓
STOPPED
```

`READY` and `SERVING` must remain separate.

Only `SERVING` may admit normal requests.

`DRAINING` must reject new normal work while already-admitted work remains
subject to its existing execution context, cancellation, and deadline rules.

`STOPPED` is terminal.

###### Request admission boundary

The runtime must enforce:

```text
incoming request
      ↓
lifecycle admission
      ↓
if serving:
    continue
else:
    reject before capability execution
```

A request that fails lifecycle admission must never reach the capability
handler.

The runtime must not rely on a capability handler to determine whether the
engine is currently serving.

###### Context propagation

The runtime must preserve the single Core context path:

```text
OperationContext
       ↓
EngineContext
       ↓
Capability execution
```

The Core `EngineContext` already carries execution information such as:

```text
OperationContext
CancellationToken
Deadline
optional SecurityContext
optional ProvenanceContext
optional ConfigurationSnapshot
```

Phase 0 must consume and propagate this context rather than introducing
parallel:

```text
operation identity
deadline
cancellation
security
provenance
```

systems.

###### Cancellation and deadline

Phase 0 must preserve Core cancellation/deadline semantics.

It must not introduce:

```text
IndexingCancellationToken
IndexingDeadline
```

or another competing mechanism.

Before capability execution, the normal Core ordering remains authoritative:

```text
cancellation check
      ↓
deadline check
      ↓
capability resolution
      ↓
handler invocation
```

###### Shutdown

The runtime should use Core shutdown mechanisms.

Conceptually:

```text
SERVING
   ↓
DRAINING
   ↓
stop new admission
   ↓
allow admitted work to complete according to context
   ↓
runtime-owned cleanup
   ↓
STOPPED
```

Phase 0 does not create an Indexing-specific shutdown system.

---

#### `src/engine/capability.rs`

##### Responsibility

This module connects Indexing capabilities to the Core capability system.

The Core already supplies:

```text
CapabilityDefinition
CapabilityId
CapabilityHandler
CapabilityInvocation
CapabilityOutcome
CapabilityRegistry
dispatch
```

Phase 0 must build on these mechanisms.

###### Phase 0 logic

The module should establish the ability to:

```text
define/register a capability
        ↓
place it in the Core registry
        ↓
resolve it
        ↓
invoke the handler with EngineContext
        ↓
produce CapabilityOutcome
```

The handler receives:

```text
EngineContext
CapabilityInvocation
```

and returns an appropriate capability result/error according to the Core
capability contract.

###### Minimal capability boundary

Phase 0 should use a minimal engine-owned capability or equivalent test
handler to prove the plumbing.

The Phase 0 capability must exist primarily to test:

```text
runtime
    ↓
capability resolution
    ↓
dispatch
    ↓
handler
    ↓
response
```

It must not become the first implementation of actual indexing behavior.

Do not freeze final public names for future capabilities such as:

```text
create_index
build_index
query_index
similarity_search
```

during Phase 0.

Those belong to later architectural phases.

###### Capability ownership

Indexing owns:

```text
what an Indexing capability does
```

Core owns:

```text
how capabilities are structurally registered/resolved/dispatched
```

The Phase 0 implementation must not duplicate the Core capability registry or
dispatch mechanism.

---

#### `src/error.rs`

##### Responsibility

This module contains only Indexing-specific errors that are genuinely owned
by the Indexing Engine.

Core remains the owner of shared technical error categories and the global
error contract.

###### Phase 0 rule

Do not recreate Core errors such as:

```text
InvalidTransition
DeadlineExpired
Cancelled
Unauthorized
CapabilityError
TransportError
```

when those failures are already represented by Core.

Indexing-specific errors may eventually include errors related to:

```text
IndexDefinition
index generation
index availability
index integrity
index storage
```

but those are not Phase 0 features.

Phase 0 should keep this module intentionally small and only introduce an
Indexing-owned error when a real Phase 0 boundary requires one.

---

#### Request / Execution Flow

Phase 0 must establish the engine-side portion of the universal request path.

Conceptually:

```text
Universal Request
       ↓
Indexing Engine Runtime
       ↓
Lifecycle Admission
       ↓
Request / contract structural boundary
       ↓
EngineContext association
       ↓
Cancellation / deadline checks
       ↓
Capability Resolution
       ↓
Capability Dispatch
       ↓
Indexing-owned Handler
       ↓
CapabilityOutcome
       ↓
UniversalResponse
```

Core's existing request and response constructors already own their universal
event-occurrence construction behavior. Phase 0 should use those constructors
instead of manually manufacturing occurrence metadata.

Core structural validation remains structural:

```text
payload structure
contract identity/version
interaction type
descriptor compatibility
```

Indexing must not interpret domain payload semantics at this boundary.

---

#### Control Plane Integration Boundary

Phase 0 establishes **participation**, not a local Control Plane.

The intended relationship is:

```text
Control Plane
    ↓
global coordination / routing
    ↓
Indexing Engine
    ↓
local runtime admission
    ↓
Indexing capability
```

Indexing must not contain a substitute for:

```text
global registry
global router
global scheduler
global provider resolver
global coordination planner
```

The Core report establishes that the Control Plane coordinates discovery,
eligibility, resolution, planning, and routing while the target engine remains
responsible for local admission and execution.

Therefore Phase 0 must preserve:

```text
Control Plane
    ≠
Indexing Runtime
```

and:

```text
Control Plane
    ≠
Capability execution
```

A future routing decision may identify a concrete `EngineInstanceId`, but
Indexing's local runtime remains authoritative over whether that instance is
actually able to admit work.

---

#### Health / Readiness / Observability Boundary

Phase 0 establishes integration points only.

The Indexing Engine should consume the existing Core abstractions for:

```text
health
readiness
logging
observability
diagnostics
```

rather than creating parallel Indexing versions.

The architecture requires the following separations to remain intact:

```text
Health      ≠ Lifecycle ownership
Health      ≠ Routing
Logs        ≠ Diagnostics
Diagnostics ≠ Health
Metrics     ≠ Tracing
```

For Phase 0, this means the engine can expose or participate in the required
signals, but detailed Indexing-specific operational telemetry belongs to later
phases.

---

#### Local Planner Boundary

Phase 0 establishes an **extension point**, not the Indexing planner itself.

The intended future structure is:

```text
Capability
    ↓
Indexing-local Planner
    ↓
Indexing-local Workflow
    ↓
Indexing-local Execution
```

For Phase 0, the planner boundary should only make it possible for a capability
handler to hand work to future Indexing-specific planning logic.

The implementation may represent this boundary as:

```text
trait
function boundary
service object
internal strategy interface
```

or another suitable mechanism.

The exact representation is deliberately not frozen.

The planner must not yet decide:

```text
which index family to use
which hash algorithm to use
which physical index implementation to use
which database to use
how semantic relationships are interpreted
```

Those decisions belong to later phases.

---

#### Engine Startup Responsibilities

Phase 0 startup should be treated as **establishment and validation**, not as
Indexing work.

The startup sequence is conceptually:

```text
STARTING
    ↓
establish engine identity
    ↓
establish runtime
    ↓
establish required Phase 0 capability state
    ↓
register engine
    ↓
register capabilities
    ↓
READY
    ↓
SERVING
```

Startup must fail explicitly when a required Phase 0 initialization or
registration boundary cannot be established.

An engine with unresolved required startup failure must not become `SERVING`.

Phase 0 must not add automatic startup retry loops.

---

#### Engine Registration vs Capability Registration

These remain separate in both code and tests.

```text
Engine Registration
    ↓
Indexing Engine / Indexing Instance exists

Capability Registration
    ↓
Indexing capability exists
```

A valid engine can therefore be modeled independently from a particular
capability.

This distinction is important for future phases because Indexing may expose
multiple capabilities with different readiness or availability characteristics.

---

#### Test Harness Foundation

##### Planned Test Files

```text
tests/
├── common/
│   ├── mod.rs
│   └── helpers.rs
├── engine.rs
├── integration.rs
└── conformance.rs
```

Phase 0 tests are integration/conformance tests at the repository boundary.
Implementation source files should additionally contain their own unit tests
for testable behavior.

---

##### `tests/common/`

Provide reusable construction helpers for:

```text
Indexing Engine
engine/instance identities
operations
contexts
capabilities
universal requests
test handlers
registration fixtures
```

Helpers should reduce repeated setup without hiding the behavior being tested.

They must not contain production Indexing logic.

---

##### `tests/engine.rs`

Cover the engine shell itself.

Minimum behavioral coverage:

```text
engine construction
engine identity
engine instance identity
runtime initialization
lifecycle startup
required registration
capability registration
READY transition
SERVING transition
request admission boundary
DRAINING behavior
shutdown
STOPPED state
```

Negative cases should include invalid lifecycle usage where the Core lifecycle
contract makes such cases observable.

---

##### `tests/integration.rs`

Exercise the complete Phase 0 path through the public Indexing API.

Minimum flow:

```text
construct Indexing Engine
       ↓
initialize runtime
       ↓
register
       ↓
reach READY
       ↓
enter SERVING
       ↓
construct UniversalRequest
       ↓
admit request
       ↓
resolve capability
       ↓
dispatch capability
       ↓
produce UniversalResponse
       ↓
shutdown
```

The integration suite must prove that the modules work together rather than
only testing each component in isolation.

---

##### `tests/conformance.rs`

Protect architectural boundaries.

The Phase 0 conformance suite should prove:

```text
Indexing depends on Core mechanisms
EngineId and EngineInstanceId remain distinct
OperationId and AttemptId remain distinct
Universal request/response contracts are used
Core capability registration/dispatch is reused
Core runtime admission is authoritative
Engine registration is distinct from capability registration
Control Plane is not recreated inside Indexing
Indexing does not contain domain semantics
Indexing does not select physical storage technology
Indexing does not implement indexing algorithms in Phase 0
```

Negative tests are especially important here.

The test suite should prevent architecture from drifting while later phases add
real indexing behavior.

---

#### Phase 0 Non-Goals

The following are explicitly outside Phase 0:

```text
index generation
hash generation
hash algorithm selection
index width implementation
index canonicalization
IndexDefinition implementation
IndexEntry implementation
namespace implementation
index family implementation
mapping model
semantic predicates
relationship creation
similarity algorithms
query planning implementation
retrieval implementation
index build/update
incremental indexing
batch indexing
rebuild
physical storage
database integration
partitioning
sharding
compression
vector/embedding provider
physical similarity implementation
Indexing-specific security policy
advanced configuration semantics
advanced resource management
retry policy
idempotency
advanced streaming
Indexing event infrastructure
Indexing-specific observability system
final binary/executable packaging
```

The presence of scaffold files for later phases does not move those
responsibilities into Phase 0.

---

#### Architectural Decisions for Phase 0

The following decisions are considered part of the Phase 0 foundation.

##### 1. Indexing is a library-first engine implementation

The Indexing implementation must be usable as a library through `lib.rs`.

The eventual executable is a deployment/application surface over the library,
not the place where the engine implementation lives.

##### 2. Executable packaging is deferred

The final arrangement of:

```text
src/main.rs
src/bin/*
```

will be decided after the engine's implementation phases establish the actual
runtime and capability surface.

##### 3. Core mechanisms are reused

Indexing must use `nizaam-core` for universal:

```text
identity
contracts
runtime
context
capabilities
errors
health
security
observability
Control Plane integration
```

where those mechanisms already exist.

##### 4. Indexing owns future index semantics, not domain semantics

Indexing may later decide how to generate and retrieve indexes, but it does not
decide what a word, semantic record, context, relationship, or domain object
means.

##### 5. Runtime admission remains authoritative

A request reaching a concrete Indexing instance is still subject to that
instance's local runtime admission.

Routing does not override local runtime state.

##### 6. Engine registration and capability registration remain separate

Neither mechanism is a substitute for the other.

##### 7. Phase 0 establishes boundaries before algorithms

No actual hashing, physical index, database, query algorithm, or storage provider
is frozen merely to make Phase 0 easier to implement.

---

#### Boundary Diagram

The complete Phase 0 architecture should reduce to:

```text
                         NIZAAM
                           │
                    Control Plane
                           │
                 Universal Engine Contract
                           │
                           ▼
                ┌──────────────────────┐
                │   Indexing Engine    │
                │                      │
                │  Engine Identity     │
                │  Registration        │
                │  Core Runtime        │
                │  Capability Boundary │
                │  Context Propagation │
                └──────────┬───────────┘
                           │
                     local planner
                       extension
                           │
                           ▼
                 future Indexing logic
```

The final Phase 0 request path is:

```text
UniversalRequest
      ↓
Indexing Runtime
      ↓
Lifecycle Admission
      ↓
EngineContext
      ↓
Capability Dispatch
      ↓
Indexing Handler Boundary
      ↓
UniversalResponse
```

The final Phase 0 lifecycle path is:

```text
Created
  ↓
Starting
  ↓
Configuring
  ↓
Dependencies
  ↓
Capabilities
  ↓
Registering
  ↓
Ready
  ↓
Serving
  ↓
Draining
  ↓
Stopped
```

---

#### Completion Criteria

Phase 0 is complete when all of the following are true:

- `nizaam-indexing` is a valid reusable library crate using the published
  `nizaam-core` dependency.
- The Indexing Engine has distinct logical engine and instance identities.
- Engine registration is integrated through the established Core boundary.
- Capability registration/dispatch is integrated through Core.
- The runtime participates in the established Core lifecycle.
- `READY` and `SERVING` remain distinct.
- Only `SERVING` admits normal requests.
- Lifecycle admission occurs before capability execution.
- Universal requests and responses are handled through the Core contract
  boundary.
- Operation/EngineContext propagation uses the established Core context model.
- Cancellation and deadlines use Core mechanisms rather than Indexing-specific
  substitutes.
- A minimal engine-owned capability can be invoked through the runtime and
  capability boundary.
- Shutdown reaches `STOPPED` through the established runtime path.
- Control Plane integration is represented without creating an Indexing-owned
  Control Plane.
- Health/observability/error integration points use Core mechanisms without
  creating duplicate global systems.
- The local planner exists as a clean extension point without implementing
  future indexing behavior.
- Phase 0 contains no domain semantics.
- Phase 0 contains no physical storage technology.
- Phase 0 contains no actual index-generation algorithm.
- The Phase 0 unit/integration/conformance tests cover the established
  boundaries.
- The complete repository verification required by the active workspace
  remains green.

---

#### Verification Checklist

- [x] Library crate builds against nizaam-core
- [x] EngineId / EngineInstanceId roles are distinct
- [x] Engine registration works
- [x] Capability registration works
- [x] Runtime lifecycle startup works
- [x] READY is distinct from SERVING
- [x] Non-serving admission is rejected
- [x] Serving request reaches the intended capability
- [x] OperationContext is preserved
- [x] EngineContext is used
- [x] Cancellation/deadline boundary is preserved
- [x] UniversalRequest / UniversalResponse boundary is preserved
- [x] Capability dispatch uses Core infrastructure
- [x] Control Plane integration does not create local global routing
- [x] Shutdown reaches DRAINING then STOPPED
- [x] No domain semantics entered Phase 0
- [x] No physical indexing technology entered Phase 0
- [x] No indexing algorithm entered Phase 0
- [x] Unit tests exist for Phase 0 implementation behavior
- [x] Integration tests cover the complete Phase 0 flow
- [x] Conformance tests protect architectural boundaries
- [x] Full workspace verification passes

---

#### Phase 0 Implementation Principle

The simplest correct mental model is:

```text
Phase 0 does not build the Indexing Engine's indexing algorithm.

Phase 0 builds the Nizaam engine that will later host
the Indexing Engine's indexing algorithm.
```

Once Phase 0 is stable, later phases can add the actual Indexing responsibilities
without changing the universal engine boundary:

```text
Phase 0
Engine foundation
        ↓
Phase 1
Index identity / namespaces
        ↓
Phase 2
Index data model / contract
        ↓
Phase 3
Index construction / update
        ↓
Phase 4
Query / retrieval
        ↓
later operational / hardening phases
```

---

### Phase 1: Index Identity, Families & Namespaces

#### Status

**In Progress**

##### Goal

Implement the foundational indexing identity model and separate logical
index spaces without introducing semantic mapping models into the
Nizaam Indexing Engine.

Phase 1 establishes the vocabulary and identity boundaries that all later
index construction, update, query, consistency, and storage work will use.

This phase must remain deliberately smaller than the later data-model and
execution phases. It establishes **what an index is identified by and where
it logically exists**, not how a physical index is built or stored.

---

##### Architectural Position

The Indexing Engine is a Nizaam infrastructure engine.

Its global position remains:

```text
Nizaam / Control Plane
        ↓
Indexing Engine
        ↓
Core Engine Runtime
        ↓
Core Capability Dispatch
        ↓
Indexing-local planner boundary
        ↓
Later Indexing operations
```

The common Infrastructure Engine implementation plan establishes the shared
engine shape:

```text
Infrastructure Engine
│
├── Engine Integration
├── Capability Layer
├── Infrastructure-local Planning
├── Workflow / Coordination
├── Provider / Backend Abstraction
├── Infrastructure Execution
├── Result / Artifact Handling
└── Runtime Integration
```

Phase 1 does not implement these subsystems again. The already-established
Core runtime, lifecycle, capability, context, security, health, observability,
and Control Plane mechanisms remain the shared infrastructure boundary.

The phase-specific responsibility is the foundational Indexing model beneath
that runtime.

---

#### Phase 1 Scope

Phase 1 establishes:

```text
1. Stable index identity
2. Index-family vocabulary
3. Logical namespace identity
4. Index-definition identity
5. Logical index-space registration
6. Separation of object identity from index identity
7. Separation of namespace from identity
8. Separation of logical indexing from physical implementation
9. Source-declared index-category handling without domain ownership
10. Public indexing-module boundaries
```

The phase must establish these concepts as real Rust types and behavior, not
as placeholder structs that exist only to satisfy compilation.

---

#### Core Architectural Principle

The following identities must remain separate:

```text
Stable Nizaam Object Identity
        ≠
Index Identity
        ≠
Index Namespace
        ≠
Index Definition Identity
        ≠
Physical Index Implementation
        ≠
Semantic Relationship Graph
```

A source object may participate in several logical index spaces while keeping
one canonical source-owned identity.

Conceptually:

```text
Source Object
    │
    ├── logical index space A
    ├── logical index space B
    ├── logical index space C
    └── logical index space D
```

The object does not receive a new canonical identity merely because it enters
another logical indexing space.

The Indexing architecture also requires that an index identity must not encode
the object's entire relationship graph or mutable neighborhood. Dynamic
relationships, dimensions, neighborhood data, and versions remain separate
from the stable identity.

---

#### Relationship to Core

Core owns domain-neutral execution infrastructure.

Relevant Core mechanisms include:

```text
EngineId
EngineInstanceId
OperationId
AttemptId
CapabilityId

UniversalRequest
UniversalResponse

OperationContext
EngineContext

EngineRuntime
CapabilityRegistry
CapabilityHandler
Capability dispatch

Lifecycle
Security
Health / readiness
Control Plane
Logging / observability
Error system
```

Phase 1 must reuse these mechanisms rather than introducing competing local
runtime abstractions.

The distinction is:

```text
Core
→ how a Nizaam engine participates in execution

Indexing
→ how an index identity and logical index space are represented
```

Phase 1 identity types must not become aliases of unrelated Core identities.

For example:

```text
Core::EngineId
    ≠
Indexing::IndexId
```

and:

```text
Core::ArtifactId
    ≠
Indexing::IndexId
```

Core runtime ownership, lifecycle ownership, capability dispatch, request
admission, and shutdown remain outside the Phase 1 identity implementation.

---

#### 1. Index Identity

##### Purpose

`IndexId` represents the identity of one concrete logical index.

Conceptually:

```text
IndexId
    = identity of one indexing resource
```

It must not be interpreted as:

```text
ObjectId
EngineId
Namespace
IndexDefinitionId
Database key
Physical partition
Storage identifier
```

The stable identity must be independent of the index's mutable contents.

The architecture requires:

```text
Object survives
        ↓
Index may be rebuilt or replaced
        ↓
Object identity remains unchanged
```

Likewise, the index identity itself must not encode an object's complete mutable
relationship neighborhood.

##### Required behavior

`IndexId` should provide the normal identity semantics required by a stable
identifier:

```text
equality
ordering, where useful
hashing
opaque representation
stable construction/access semantics
```

The identity should be represented in a form that can later support efficient
storage and comparison.

The exact generation algorithm remains outside this phase unless explicitly
frozen.

##### Identity-width decision

The Indexing architecture establishes a fixed-width 512-bit / 64-byte primary
identity.

The architectural decision is:

```text
Index width = 512 bits
Index size  = 64 bytes
Index space = 2^512 possible values
```

The generation/hash algorithm remains a separate implementation or
configuration decision. Human-readable encoding also remains separate
from the fixed binary width.

The implementation must not silently introduce a different width or a
different generation scheme without an explicit architectural decision.

The generation mechanism, human-readable encoding, and other representation
details remain separate concerns unless this phase explicitly freezes them.

---

#### 2. Index Family

##### Purpose

Index family identifies the broad indexing/retrieval structure represented by
an index.

Phase 1 establishes the foundational families:

```text
Identity
Inverted
Relationship
Similarity
```

These are **Indexing concepts**, not semantic mapping families.

Conceptually:

```text
IndexFamily::Identity
IndexFamily::Inverted
IndexFamily::Relationship
IndexFamily::Similarity
```

##### Identity family

The Identity family supports direct identity-oriented indexing.

It does not become the owner of source-object identity.

##### Inverted family

The Inverted family supports inverted or lexical-style retrieval structures.

It does not imply ownership of Arabic, Quran, Hadith, or any other source
semantics.

##### Relationship family

The Relationship family identifies an index intended to retrieve
relationship-oriented records supplied by a source engine.

It must not define or interpret relationship meaning.

Therefore Phase 1 must not introduce:

```text
predicate enums
semantic relationship structs
mapping families
mapping dimensions
domain ontology
inference rules
```

For example:

```text
CAUSES
PART_OF
BEFORE
RELATED_TO
SUPPORTED_BY
```

remain source-owned semantic meanings.

`IndexFamily::Relationship` merely identifies an Indexing retrieval family.

##### Similarity family

The Similarity family remains distinct from Relationship indexing.

Indexing may later consume similarity representations supplied by another
engine, but Phase 1 must not freeze:

```text
embedding provider
vector model
similarity algorithm
HNSW
IVF
FAISS
```

or another concrete physical technology.

---

#### 3. Index Namespace

##### Purpose

A namespace identifies a **logical indexing space**.

Conceptually:

```text
IndexNamespace
    = logical indexing locality / space
```

It is not:

```text
Object Identity
Physical Partition
Database
Shard
Storage Provider
Engine Type
Domain Entity
```

The architecture deliberately keeps namespace separate from canonical
identity.

For example:

```text
canonical object
        │
        ├── namespace A
        ├── namespace B
        └── namespace C
```

The same object may therefore participate in multiple logical spaces.

---

##### Namespace must remain generic

Do not implement domain-coupled variants such as:

```rust
enum IndexNamespace {
    Quran,
    Arabic,
    Hadith,
    Kg,
}
```

That would turn the generic Indexing Engine into a domain-specific component.

Namespaces are identifiers supplied by source engines.

Conceptually:

```text
IndexNamespace("quran")
IndexNamespace("arabic")
IndexNamespace("kg")
```

may all be valid values while Indexing remains unaware of their domain
meaning.

Those strings are examples of caller-supplied identifiers, not frozen
namespace names.

---

##### Namespace validation

Phase 1 may validate only what is necessary to maintain a meaningful logical
identifier.

Do not silently freeze:

```text
specific prefixes
hierarchical syntax
case rules
reserved domain names
physical partition rules
database naming rules
automatic namespace generation
```

unless explicitly authorized.

The architecture leaves these details open.

The purpose of Phase 1 validation is correctness of the identifier concept,
not invention of a final namespace wire format.

---

#### 4. Logical Namespace Registration

A namespace must be registerable as a logical indexing space.

Phase 1 should provide an in-memory logical registration boundary sufficient
to establish:

```text
register
contains
duplicate detection
unregister, where required by the chosen lifecycle
snapshot/read access
```

The registry must remain an Indexing logical model.

It must not become:

```text
database catalog
distributed registry
persistent storage layer
sharding system
physical partition manager
```

Those concerns remain deferred.

Conceptually:

```text
NamespaceRegistry

    namespace A
    namespace B
    namespace C
```

The registry's responsibility is to answer:

> Which logical index spaces are currently registered?

It must not answer:

> How are those spaces physically partitioned?

---

#### 5. Index Definition Identity

Phase 1 establishes only the identity aspects of an index definition.

The complete `IndexDefinition` data model belongs primarily to Phase 2.

The distinction should therefore be:

```text
IndexDefinitionId
    = identity of a logical index definition

IndexId
    = identity of a concrete index

IndexDefinitionIdentity
    = definition identity + logical indexing context
```

Conceptually:

```text
IndexDefinitionIdentity
│
├── definition_id
├── namespace
└── family
```

This lets the engine distinguish:

```text
which definition
+
which logical space
+
which indexing family
```

without prematurely implementing the complete definition contract.

---

##### Deferred Phase 2 definition fields

Do not move the complete definition model into Phase 1 merely for convenience.

The following belong to the later generic data-model phase:

```text
key_definition
target_reference_type
uniqueness
consistency_requirement
source_version
schema_version
lifecycle_state
```

and any other fields required by the approved Phase 2 design.

Phase 1 establishes the identity boundary that Phase 2 will extend.

---

#### 6. Logical Index Space

A logical index space is the combination of the logical namespace and the
indexing definition/family that describes how that space is used.

Conceptually:

```text
Logical Index Space
        │
        ├── Namespace
        ├── Family
        └── Definition Identity
```

The space exists at the logical architecture layer.

It must not imply:

```text
one database table
one partition
one shard
one physical index
one storage provider
```

A later physical implementation may choose to back multiple logical spaces
with one physical implementation.

Therefore:

```text
logical separation
        ≠
physical duplication
```

---

#### 7. Source-Owned Object Categories

Phase 1 must allow indexing requirements for source-declared categories such
as:

```text
Word
Mention / Occurrence
Sentence / Span
Passage
Document
Semantic record
Context record
Relationship record
```

These categories remain source-engine concepts.

Indexing must not create domain entities for them.

Do not introduce models such as:

```rust
struct QuranWord { ... }
struct HadithRecord { ... }
struct SemanticMapping { ... }
struct ContextRecord { ... }
```

as Phase 1 Indexing-owned domain abstractions.

Instead, the logical category should remain source-declared metadata or
identifier information.

The Indexing Engine only needs to distinguish the indexing space/category
requested by the source.

---

#### 8. Semantic Ownership Boundary

The KG and other domain/source engines own:

```text
meaning
predicates
relationship semantics
context meaning
qualifier meaning
evidence meaning
provenance meaning
authority
status
inference
ontology
```

Indexing owns:

```text
index identity
logical index namespace
index family
index definition identity
logical registration
future index construction
future retrieval
```

Therefore:

```text
KG / domain engine
→ defines the relationship

Indexing
→ indexes the supplied relationship record
```

and not:

```text
Indexing
→ decides what the relationship means
```

Phase 1 must contain negative architectural tests protecting this boundary.

---

#### 9. Module Structure

The planned Phase 1 source structure is:

```text
src/

├── identity/
│   ├── mod.rs
│   ├── index.rs
│   ├── namespace.rs
│   └── definition.rs
│
└── index/
    └── mod.rs
```

The file boundaries are implementation locations rather than permission to
duplicate concepts across modules.

---

##### `src/identity/index.rs`

Owns:

```text
IndexId
```

and the identity behavior associated with it.

Responsibilities:

```text
construction
validation
identity comparison
hashing/ordering where appropriate
opaque access
```

Must not own:

```text
storage
query
build
relationship semantics
physical index implementation
```

---

##### `src/identity/namespace.rs`

Owns:

```text
IndexNamespace
NamespaceRegistry
```

or the final equivalent approved by implementation.

Responsibilities:

```text
namespace representation
namespace validation
namespace equality
logical registration
duplicate detection
logical lookup/snapshot
```

Must not own:

```text
physical partitioning
database catalog
domain-specific namespace meanings
```

---

##### `src/identity/definition.rs`

Owns:

```text
IndexDefinitionId
IndexDefinitionIdentity
```

or the final equivalent identity-only model.

Responsibilities:

```text
definition identity
namespace association
family association
identity comparison
```

Must not prematurely become the full Phase 2 `IndexDefinition` contract.

---

##### `src/identity/mod.rs`

Owns the module-level identity boundary.

Responsibilities:

```text
public exports
module-level tests where interaction between identity files is meaningful
```

It must not become a replacement for source-file unit tests.

Each source file remains responsible for its own testable behavior.

---

##### `src/index/mod.rs`

Owns the initial indexing boundary and foundational family vocabulary.

Responsibilities:

```text
IndexFamily
public index-facing exports
cross-module identity/index boundary where appropriate
```

It should remain small during Phase 1.

Concrete index entries, references, similarity structures, and versioned
index data belong primarily to Phase 2.

---

#### 10. Crate Root

`src/lib.rs` must expose the Phase 1 modules cleanly without exposing
implementation details unnecessarily.

Conceptually:

```text
nizaam_indexing
    │
    ├── identity
    └── index
```

The package's runtime/bin arrangement is intentionally outside Phase 1.

The previously established decision remains:

```text
src/main.rs
src/bin/nizaam-indexing.rs
```

are not to be designed or finalized during Phase 1.

The final executable arrangement will be decided only after the complete
implementation phases have established the engine's real operational needs.

---

#### 11. Test Architecture

Phase 1 test files:

```text
tests/

├── identity.rs
├── index.rs
└── conformance.rs
```

Tests must include both source-local unit tests and repository-level
integration/conformance coverage.

---

#### 12. Unit Tests

Every implementation source file must test its own testable behavior.

##### `identity/index.rs`

Cover:

```text
valid construction
invalid construction, when applicable
equality
inequality
stable representation behavior
ordering, when exposed
hashing behavior, when exposed
```

and identity-separation invariants where the implementation itself owns the
relevant logic.

##### `identity/namespace.rs`

Cover:

```text
valid namespace construction
invalid namespace construction
namespace equality
different namespaces remain distinct
registration
duplicate registration
lookup/contains
logical removal, when implemented
snapshot behavior
```

##### `identity/definition.rs`

Cover:

```text
definition identity construction
definition equality
namespace association
family association
definition identity remains distinct from IndexId
```

##### `identity/mod.rs`

Add module-level tests only when they verify interaction between multiple
identity implementation files.

Do not move all tests into `mod.rs`.

##### `index/mod.rs`

Cover foundational `IndexFamily` behavior:

```text
Identity exists
Inverted exists
Relationship exists
Similarity exists
families remain distinct
```

and any cross-module behavior actually implemented there.

---

#### 13. Repository-Level Integration Tests

##### `tests/identity.rs`

Verify:

```text
IndexId remains separate from namespace
namespace identity remains separate from object identity
multiple namespaces can coexist
duplicate namespaces are rejected
one logical identity can participate in multiple spaces
definition identity remains separate from concrete index identity
```

The canonical object identity used for this proof may be test-side data; do
not create a production domain-object model merely to make the test convenient.

---

##### `tests/index.rs`

Verify:

```text
all foundational IndexFamily variants
family separation
namespace + family composition
logical index-space separation
Relationship family does not create semantic predicates
Similarity remains a distinct family
```

The test must not depend on a physical implementation.

---

##### `tests/conformance.rs`

Verify the public consumer-facing composition:

```text
create namespace
        ↓
register logical space
        ↓
create definition identity
        ↓
associate family
        ↓
obtain index identity
        ↓
keep all identities distinct
```

This should behave as a small downstream-consumer scenario rather than
duplicating internal unit tests.

---

#### 14. Required Negative Tests

Phase 1 needs explicit boundary tests.

At minimum, tests must demonstrate that the implementation does not imply:

```text
Namespace == Object Identity
Namespace == Physical Partition
IndexId == Object Identity
IndexId == IndexDefinitionId
Relationship family == semantic predicate model
Indexing == KG
Indexing == domain object storage
```

There should also be test coverage against introducing domain semantics through
the indexing family model.

For example, the presence of:

```text
IndexFamily::Relationship
```

must not imply that Indexing owns:

```text
CAUSES
BEFORE
PART_OF
RELATED_TO
```

or any other semantic predicate.

---

#### 15. Examples of Valid Phase 1 Composition

A source engine may conceptually declare:

```text
source object:
    object-id = X

namespace:
    kg

family:
    relationship
```

Indexing can then represent the logical indexing identity:

```text
namespace = kg
family    = Relationship
definition = D
index      = I
```

A different logical indexing space may exist for the same source object:

```text
source object:
    object-id = X

namespace:
    lexical

family:
    inverted
```

The canonical source identity remains:

```text
X
```

while the indexing identities remain separate.

Another source engine could use:

```text
namespace = arabic
family    = inverted
```

without Indexing knowing anything about Arabic semantics.

---

#### 16. What Phase 1 Does NOT Implement

The following are explicitly deferred:

```text
IndexEntry
ObjectReference
SimilarityEntry
IndexVersion
complete IndexDefinition
IndexRequirement
query requests
query results
query planning
index building
incremental updates
batch ingestion
rebuilds
publication
storage
persistence
physical providers
partitioning
sharding
hash algorithm selection
embedding generation
similarity algorithm selection
FST
HNSW
IVF
Lucene
FAISS
PostgreSQL
MongoDB
semantic mapping creation
semantic predicate definitions
domain inference
domain ontology
domain object hydration
```

The source scaffold may already contain files for later phases. Their existence
does not mean Phase 1 should implement their functionality.

Future-phase scaffolding must remain untouched unless necessary for module
wiring.

---

#### 17. Dependency Direction

The dependency direction must remain:

```text
Source / Domain Engine
        ↓
Index Requirement / source-owned record
        ↓
Indexing logical model
        ↓
Later Indexing implementation
        ↓
Physical provider
```

and not:

```text
Indexing
        ↓
domain semantics
        ↓
KG
```

Indexing must remain reusable by multiple engines.

---

#### 18. Control Plane Boundary

The Control Plane remains responsible for global coordination and routing.

The Indexing Engine remains responsible for local indexing work.

Therefore Phase 1 must not add:

```text
global scheduler
global workflow engine
cross-engine planner
routing policy
destination selection
transport implementation
```

The later local planner belongs inside Indexing and will operate beneath the
common Core runtime.

The architecture remains:

```text
Control Plane
→ which engine / capability / destination participates

Indexing local planner
→ how Indexing performs its own work
```

---

#### 19. Physical Implementation Boundary

Phase 1 must not expose or require a physical implementation.

Callers must not need to know whether a later implementation uses:

```text
B-tree
hash table
FST
HNSW
IVF
Lucene
FAISS
PostgreSQL
MongoDB
other provider
```

The logical identity model must remain valid regardless of the eventual
physical implementation.

Therefore:

```text
logical index space
        ≠
physical index
```

---

#### 20. Phase 1 Implementation Sequence

The implementation order should be:

```text
Step 1
src/identity/index.rs
        ↓
IndexId

Step 2
src/identity/namespace.rs
        ↓
IndexNamespace
NamespaceRegistry

Step 3
src/identity/definition.rs
        ↓
IndexDefinitionId
IndexDefinitionIdentity

Step 4
src/identity/mod.rs
        ↓
identity public boundary

Step 5
src/index/mod.rs
        ↓
IndexFamily
index public boundary

Step 6
src/lib.rs
        ↓
crate exports

Step 7
unit tests
        ↓
source-local verification

Step 8
tests/identity.rs
tests/index.rs
tests/conformance.rs
        ↓
public integration/conformance verification
```

This sequence minimizes circular design pressure and keeps the Phase 1
implementation focused.

---

#### 21. Architectural Invariants

These must remain true after Phase 1:

```text
Object Identity
    ≠
Index Identity
```

```text
Index Identity
    ≠
Index Definition Identity
```

```text
Index Namespace
    ≠
Physical Partition
```

```text
Index Namespace
    ≠
Object Identity
```

```text
Index Family
    ≠
Semantic Mapping Family
```

```text
Relationship Index
    ≠
Relationship Semantics
```

```text
Indexing
    ≠
KG
```

```text
Logical Index Space
    ≠
Physical Implementation
```

```text
Core Runtime
    ≠
Indexing Runtime Reimplementation
```

---

#### 22. Important Deferred Decisions

The following decisions must not be silently frozen during Phase 1 unless
explicitly approved:

```text
exact human-readable IndexId encoding
exact hash/index generation algorithm
exact namespace syntax
namespace prefix alphabet
hierarchical namespace format
namespace registry persistence
physical partitioning
sharding
storage provider
physical index family implementation
similarity algorithm
embedding/vector provider
serialization technology
provider deployment topology
```

The implementation should preserve room for these decisions.

---

#### Completion Criteria

Phase 1 is complete when:

#### Verification Checklist

- [ ] IndexId exists as a genuine Indexing identity type.

- [ ] Namespace exists as a genuine logical identity type.

- [ ] IndexDefinition identity is represented separately from IndexId.

- [ ] The four foundational IndexFamily variants exist.

- [ ] Logical namespaces can be registered and distinguished.

- [ ] Duplicate logical namespace registration is handled deterministically.

- [ ] One source-owned object identity can participate in multiple logical
  index spaces without receiving multiple canonical identities.

- [ ] Namespace is not treated as a physical partition.

- [ ] Relationship indexing does not introduce semantic predicate ownership.

- [ ] Source categories remain source-owned and are not implemented as
  Indexing domain entities.

- [ ] The complete Phase 1 public module boundary is usable.

- [ ] Unit tests cover implementation behavior.

- [ ] Integration/conformance tests cover public cross-module behavior.

- [ ] Negative boundary tests protect identity and semantic ownership.

- [ ] No physical indexing technology has leaked into the logical contract.

- [ ] No Core runtime mechanism has been reimplemented locally.

- [ ] No Phase 2+ indexing functionality has been implemented as a shortcut.

- [ ] Previously verified Core behavior remains untouched.

Completion does **not** mean that the Indexing Engine can yet build or query
real physical indexes. That begins in later phases.

---

#### Final Phase 1 Mental Model

After Phase 1, the Indexing Engine should be able to answer:

```text
What index is this?
        ↓
IndexId

Where does this index logically live?
        ↓
IndexNamespace

What logical definition does it belong to?
        ↓
IndexDefinitionId / IndexDefinitionIdentity

What broad indexing family is it?
        ↓
IndexFamily

Can several logical spaces coexist?
        ↓
Namespace registration / logical index-space model
```

It should still **not** answer:

```text
How is the index physically stored?
How is the index built?
How is the index queried?
How is the relationship semantically interpreted?
How is the embedding generated?
How is the provider partitioned?
```

Those questions belong to later approved phases.

The Phase 1 foundation is therefore:

```text
                 Source Engine
                       │
                 source-owned object
                       │
                 logical requirement
                       │
                       ▼
              ┌───────────────────┐
              │ Indexing Phase 1  │
              │                   │
              │ IndexId           │
              │ Namespace         │
              │ Definition Id     │
              │ Index Family      │
              │ Logical Registry  │
              └─────────┬─────────┘
                        │
                        ▼
                 Phase 2+ models
                        │
                        ▼
              build / query / storage
```

Phase 1 therefore establishes the **identity and logical-space foundation**
without prematurely committing the Indexing Engine to semantic ownership,
physical storage, or a particular indexing technology.

---

### Phase 2: Index Data Contracts & Logical Data Model

#### Status

**Not Started**

##### Goal

Implement the generic logical data contracts that operate on the identity and logical index-space foundation established by Phase 1.

Phase 2 establishes the actual logical indexing model used by later index construction, update, versioning, query planning, retrieval, and provider execution.

The phase must introduce real Rust types and behavior for:

```text
1. IndexRequirement
2. IndexDefinition
3. IndexEntry
4. ObjectReference
5. SimilarityEntry
6. IndexVersion
7. QueryRequest
8. QueryResult
9. Generic key-definition / key-material representation
10. Logical validation and normalization
11. Reference-only result semantics
12. Source/schema/version compatibility metadata
13. Phase 2 public module boundaries
```

Phase 2 extends the identity foundation established by Phase 1.

It does not replace it.

The relationship is:

```text
Phase 1
    ↓
identity + namespace + family + definition identity
    ↓
Phase 2
    ↓
logical data contracts
    ↓
Phase 3+
    ↓
construction / update / publication / query execution / storage
```

Phase 2 therefore answers:

```text
What does an index logically contain?
What does a source engine request from Indexing?
What is the canonical logical definition of an index?
What does one indexed entry represent?
How does Indexing refer to source-owned objects?
How is similarity represented generically?
How is an index version represented?
What does a logical query request look like?
What does a logical query result contain?
```

It must still not answer:

```text
How is the index physically built?
How is it physically stored?
Which provider executes it?
How is a query physically executed?
How are embeddings generated?
How are semantic relationships interpreted?
How are domain objects hydrated?
```

Those concerns belong to later phases.

---

#### Architectural Position

The Indexing Engine remains a Nizaam infrastructure engine.

Its global architectural position remains:

```text
Nizaam / Control Plane
        ↓
Indexing Engine
        ↓
Core Engine Runtime
        ↓
Core Capability Dispatch
        ↓
Indexing-local planner boundary
        ↓
Later Indexing operations
```

The shared Infrastructure Engine architecture remains responsible for the common engine shape:

```text
Infrastructure Engine
│
├── Engine Integration
├── Capability Layer
├── Infrastructure-local Planning
├── Workflow / Coordination
├── Provider / Backend Abstraction
├── Infrastructure Execution
├── Result / Artifact Handling
└── Runtime Integration
```

Phase 2 does not reimplement those shared mechanisms.

Core remains responsible for:

```text
Engine identity
Operation identity
Attempt identity
Capability identity

UniversalRequest
UniversalResponse

OperationContext
EngineContext

EngineRuntime
CapabilityRegistry
CapabilityHandler
Capability dispatch

Lifecycle
Security
Health / readiness
Control Plane
Logging / observability
Error handling
```

Indexing remains responsible for:

```text
Index identity
Index namespace
Index family
Index definition
Index requirements
Index entries
Object references
Similarity entries
Index versions
Logical query contracts
Logical result contracts
```

The architectural distinction is therefore:

```text
Core
    → how an engine participates in Nizaam execution

Indexing
    → what logical indexing data means to the Indexing Engine
```

Phase 2 must reuse Phase 1 identity types rather than introduce competing identities.

---

#### Phase 2 Architectural Principle

Phase 2 introduces the following conceptual chain:

```text
Source Engine
      │
      │ declares indexing need
      ▼
IndexRequirement
      │
      │ validation / normalization
      ▼
IndexDefinition
      │
      ├───────────────┐
      │               │
      ▼               ▼
IndexEntry       QueryRequest
      │               │
      ▼               ▼
ObjectReference   QueryResult
      │               │
      └───────┬───────┘
              ▼
        IndexVersion
```

The important distinction is:

```text
IndexRequirement
    = caller/source declaration

IndexDefinition
    = Indexing's canonical logical definition

IndexEntry
    = logical key → target association

ObjectReference
    = reference to a source-owned object

SimilarityEntry
    = logical similarity-index association

IndexVersion
    = version identity and metadata for an index

QueryRequest
    = logical retrieval request

QueryResult
    = generic retrieval response
```

These are logical contracts.

They are not physical storage records, provider configurations, schedulers, or execution workflows.

---

#### 1. IndexRequirement

##### Purpose

`IndexRequirement` represents a request made by a source engine to the Indexing Engine.

It answers:

> What does the source engine need indexed?

It is a source-facing declaration.

Conceptually:

```text
IndexRequirement
├── namespace
├── index family
├── source category
├── key definition / key material
├── target reference type
├── uniqueness requirement
├── consistency requirement
├── source version
└── schema version
```

The exact final representation may use dedicated types for the individual concepts.

The requirement must remain generic.

It must not contain physical implementation instructions.

Valid conceptual information includes:

```text
namespace
family
key material
target reference type
uniqueness
consistency
source version
schema version
```

It must not contain:

```text
BTreeConfig
HnswConfig
PostgresIndexConfig
MongoCollection
LuceneIndex
FAISSIndex
ShardId
PartitionId
DatabaseTable
```

The source engine declares the logical requirement.

Indexing decides how that requirement becomes its canonical logical definition.

---

##### Requirement-to-definition boundary

The intended flow is:

```text
Source Engine
      ↓
IndexRequirement
      ↓
validation
      ↓
normalization
      ↓
IndexDefinition
```

Therefore:

```text
Requirement
    = caller's declared need

Definition
    = Indexing's canonical logical contract
```

The two types must not be collapsed into one type merely because their fields overlap.

This separation allows Indexing to validate and normalize equivalent source requirements into a stable logical definition.

---

##### Requirement validation

Phase 2 must validate logical correctness.

Validation may include:

```text
required identity information exists
namespace is valid
family is valid
key definition is valid
target reference type is valid
uniqueness requirement is coherent
consistency requirement is coherent
version metadata is valid
```

Validation must not validate a physical provider configuration because physical providers are outside the Phase 2 contract.

---

#### 2. IndexDefinition

##### Purpose

`IndexDefinition` represents the canonical logical definition of one indexing space.

Phase 1 established the identity aspects of the definition.

Phase 2 now completes the logical data model.

Conceptually:

```text
IndexDefinition
├── index_definition_id
├── namespace
├── index_family
├── key_definition
├── target_reference_type
├── uniqueness
├── consistency_requirement
├── source_version
├── schema_version
└── lifecycle metadata
```

The exact representation should reuse the Phase 1 identity types where appropriate.

`IndexDefinition` is a logical contract.

It must be:

```text
descriptive
validated
version-aware
provider-neutral
source-neutral
```

It must not be executable.

It must not contain physical implementation details.

---

##### IndexDefinition and IndexId

Phase 1 established:

```text
IndexId
    ≠
IndexDefinitionId
```

Phase 2 must preserve this distinction.

Conceptually:

```text
IndexDefinition
        │
        │ describes
        ▼
logical index configuration

IndexId
        │
        │ identifies
        ▼
one concrete logical index resource
```

A definition may therefore describe the logical structure while an `IndexId` identifies a concrete index associated with that definition.

The implementation must not silently collapse these identities.

---

#### 3. Generic Key Definition and Key Material

##### Purpose

Key material is the bridge between source-owned data and generic Indexing.

The source engine owns the meaning of its data.

Indexing owns the indexing operation.

Conceptually:

```text
Source-owned record
        ↓
source extracts indexable key material
        ↓
IndexRequirement / IndexEntry
        ↓
Indexing
```

The key material must therefore be generic.

Examples may conceptually include:

```text
lemma = X
```

or:

```text
source = X
relation = Y
target = Z
```

Indexing does not need to understand what those values mean.

It receives declared key material and treats it as indexing input.

---

##### Semantic boundary

For example, a KG engine may declare key material corresponding to:

```text
source = A
predicate = CAUSES
target = B
```

Indexing may index that material.

Indexing must not interpret:

```text
CAUSES
```

as a semantic predicate owned by Indexing.

The source engine remains responsible for:

```text
meaning
predicate semantics
ontology
inference
relationship interpretation
```

Indexing remains responsible for:

```text
representing
indexing
retrieving
```

the supplied key material.

---

#### 4. ObjectReference

##### Purpose

`ObjectReference` represents a reference to an object owned by another engine or source domain.

It is one of the most important abstractions in Phase 2 because it prevents Indexing from becoming a domain-object storage or hydration system.

Conceptually:

```text
ObjectReference
├── source / owner identity
└── opaque object identity / reference
```

Its meaning is:

```text
"this indexed result refers to object X owned by source Y"
```

The owning engine remains responsible for resolving X.

---

##### Generic reference requirement

The same Indexing Engine must be able to reference objects from multiple source engines.

Examples:

```text
KG object
Arabic object
Quran object
Hadith object
Document
Passage
Context record
Relationship record
```

Indexing must not create domain-specific reference variants such as:

```rust
enum ObjectReference {
    QuranVerse(...),
    HadithRecord(...),
    KgEntity(...),
}
```

That would couple the generic Indexing Engine to domain schemas.

Instead, the reference remains generic.

---

##### ObjectReference is not a new object identity system

Phase 1 established:

```text
Canonical Object Identity
    ≠
Index Identity
    ≠
Index Namespace
```

Phase 2 adds:

```text
ObjectReference
```

but does not redefine object identity.

The conceptual relationship is:

```text
Source-owned canonical object identity
        ↓
ObjectReference
        ↓
indexed target
```

`ObjectReference` points to a source-owned object.

It does not become the authority that creates or manages that object's domain identity.

---

##### ObjectReference must not hydrate objects

`ObjectReference` must not:

```text
load domain objects
hydrate entities
query KG
query Quran
query Hadith
resolve domain schemas
```

It only represents the reference.

Domain-object resolution remains outside Indexing.

---

#### 5. IndexEntry

##### Purpose

`IndexEntry` represents one generic logical indexed association between key material and a target reference.

It is not a physical database row.

Conceptually:

```text
IndexEntry
├── key material
├── target reference
└── logical metadata
```

The central relationship is:

```text
key material
      ↓
IndexEntry
      ↓
ObjectReference
```

The entry must remain generic enough to represent different source-owned records without embedding domain objects.

---

##### IndexEntry must not own domain objects

Incorrect:

```text
IndexEntry
    ↓
QuranVerse
```

Correct:

```text
IndexEntry
    ↓
ObjectReference
```

The source engine owns the actual object.

Indexing owns the logical association used for indexing and retrieval.

---

##### IndexEntry responsibilities

`IndexEntry` may support:

```text
construction
validation
key access
reference access
logical metadata access
comparison where meaningful
```

It must not perform:

```text
storage
lookup
index building
provider selection
physical execution
semantic interpretation
domain-object hydration
```

---

#### 6. SimilarityEntry

##### Purpose

`SimilarityEntry` represents a generic logical similarity-index association.

Similarity is a foundational indexing family, but similarity semantics and physical algorithms remain outside Phase 2.

Conceptually:

```text
SimilarityEntry
├── similarity representation / key material
├── target reference
└── logical similarity metadata
```

The exact representation must remain generic and provider-neutral.

---

##### Similarity boundary

The source or another appropriate engine may provide a representation used for similarity indexing.

Conceptually:

```text
Representation generation
        ↓
Source / Other Engine
        ↓
SimilarityEntry
        ↓
Indexing
```

Indexing performs the indexing responsibility.

It must not automatically become responsible for generating representations.

Therefore Phase 2 must not implement or freeze:

```text
embedding generation
embedding model
vector provider
HNSW
IVF
FAISS
vector database
similarity algorithm
```

The physical similarity implementation remains deferred.

---

##### Similarity is not relationship semantics

Similarity must remain distinct from relationship semantics.

```text
SimilarityEntry
    ≠
SemanticRelationship
```

A similarity index may retrieve objects according to a representation.

It does not establish that the objects have a semantic relationship.

---

#### 7. IndexVersion

##### Purpose

`IndexVersion` represents the logical version identity and metadata associated with an index.

Phase 2 must keep different kinds of version information distinct.

At minimum:

```text
Contract Version
    → Core contract compatibility

Schema Version
    → structure compatibility

Source Version
    → source dataset state/version

Index Version
    → logical index state/version
```

These concepts must not be silently collapsed.

---

##### IndexVersion responsibilities

`IndexVersion` may represent:

```text
version identity
source version association
schema version association
logical version metadata
```

It must remain suitable for later phases to support:

```text
candidate index version
active index version
rebuild version
publication
```

However, Phase 2 must not implement the rebuild or publication workflow.

---

##### Deferred lifecycle workflow

The later construction architecture may eventually follow:

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

Phase 2 only establishes the version model required by that future workflow.

It does not implement the workflow itself.

---

#### 8. QueryRequest

##### Purpose

`QueryRequest` represents a logical request to retrieve information from an index.

Phase 2 defines the contract.

It does not implement query execution.

The correct boundary is:

```text
Phase 2
    → what a logical query request looks like

Later Query Phase
    → validate
    → plan
    → resolve
    → execute
    → retrieve
```

A `QueryRequest` describes:

```text
what is being requested
```

It must not describe:

```text
how the request is physically executed
```

---

##### QueryRequest must remain provider-neutral

A query request must not require callers to know whether the implementation uses:

```text
B-tree
hash index
FST
HNSW
IVF
Lucene
FAISS
PostgreSQL
MongoDB
other provider
```

The query contract describes the logical retrieval need.

Physical strategy selection belongs to later Indexing planning and provider layers.

---

#### 9. QueryResult

##### Purpose

`QueryResult` represents the generic logical result returned by Indexing.

It must remain reference-oriented.

Conceptually:

```text
QueryResult
├── one or more retrieval records
├── ObjectReference
├── optional retrieval metadata
├── index identity
├── index version
└── continuation information where required
```

Possible retrieval metadata may include:

```text
score
distance
logical retrieval metadata
```

The exact fields should remain generic and should not force one retrieval algorithm onto all index families.

---

##### Reference-only result boundary

The central invariant is:

```text
QueryResult
    ≠
Quran object

QueryResult
    ≠
KG entity

QueryResult
    ≠
Hadith record
```

Instead:

```text
QueryResult
    ↓
ObjectReference
```

The owning source engine can resolve the referenced object after Indexing returns the result.

This keeps Indexing generic and prevents domain-object hydration from leaking into the indexing layer.

---

#### 10. IndexDefinition, IndexEntry, and Query Contracts

The logical relationships should remain:

```text
IndexDefinition
        │
        │ describes
        ▼
logical index space
        │
        ├───────────────┐
        │               │
        ▼               ▼
   IndexEntry       QueryRequest
        │               │
        ▼               ▼
ObjectReference    QueryResult
```

`IndexVersion` provides version metadata for the logical index state:

```text
IndexDefinition
      ↓
IndexVersion
      ↓
IndexEntry / QueryResult
```

The exact runtime flow belongs to later phases.

Phase 2 establishes only the contracts and relationships required by those later phases.

---

#### 11. Source-to-Indexing Contract

Core already provides the execution-level boundary.

Conceptually:

```text
UniversalRequest
        ↓
Core Capability Dispatch
        ↓
Indexing Capability
        ↓
opaque capability payload
        ↓
IndexRequirement / QueryRequest
```

Core does not need to understand the internal meaning of `IndexRequirement` or `QueryRequest`.

Core owns:

```text
request
capability
contract
operation
context
security
deadline
cancellation
```

Indexing owns:

```text
IndexRequirement
IndexDefinition
IndexEntry
ObjectReference
SimilarityEntry
IndexVersion
QueryRequest
QueryResult
```

This preserves the intended architectural boundary.

Phase 2 must not modify Core merely to make the Indexing data model understandable to Core.

---

#### 12. Module Structure

The Phase 2 source structure should be:

```text
src/

├── identity/
│   ├── mod.rs
│   ├── index.rs
│   ├── namespace.rs
│   └── definition.rs
│
├── index/
│   ├── mod.rs
│   ├── definition.rs
│   ├── entry.rs
│   ├── reference.rs
│   ├── similarity.rs
│   ├── version.rs
│   └── query.rs
│
└── requirement/
    ├── mod.rs
    └── requirement.rs
```

Phase 1 identity modules remain unchanged except for necessary public integration.

The Phase 2-specific modules are responsible for the logical data contracts.

The file boundaries are implementation boundaries, not permission to duplicate concepts.

---

#### 13. File-by-File Implementation Responsibility

##### `src/index/definition.rs`

Owns:

```text
IndexDefinition
```

Responsibilities:

```text
logical definition representation
definition validation
logical field access
association with Phase 1 definition identity
namespace association
family association
key-definition association
target-reference-type association
uniqueness
consistency
source/schema version metadata
lifecycle metadata
```

Must not own:

```text
physical provider configuration
storage
query execution
index construction
rebuild
publication
semantic relationship interpretation
```

---

##### `src/index/entry.rs`

Owns:

```text
IndexEntry
```

Responsibilities:

```text
key material
target reference
logical entry metadata
construction
validation
accessors
```

Must not own:

```text
storage
lookup
index building
provider selection
domain objects
semantic interpretation
```

---

##### `src/index/reference.rs`

Owns:

```text
ObjectReference
```

Responsibilities:

```text
source / owner identification
opaque object reference
reference validation
reference equality
reference access
```

Must not own:

```text
domain object loading
hydration
source-engine queries
domain schemas
domain-specific entity types
```

---

##### `src/index/similarity.rs`

Owns:

```text
SimilarityEntry
```

Responsibilities:

```text
generic similarity representation
target reference
logical similarity metadata
validation
```

Must remain:

```text
algorithm-neutral
provider-neutral
source-neutral
```

No:

```text
HNSW
IVF
FAISS
embedding generation
vector database
```

---

##### `src/index/version.rs`

Owns:

```text
IndexVersion
```

Responsibilities:

```text
index version identity
source version association
schema version association
version metadata
comparison where appropriate
```

Must not implement:

```text
rebuild
candidate construction
publication
active-version switching
```

---

##### `src/index/query.rs`

Owns:

```text
QueryRequest
QueryResult
```

Responsibilities:

```text
logical query request representation
logical result representation
reference-only result semantics
optional generic retrieval metadata
index/version metadata
continuation representation where required
```

Must not implement:

```text
query planning
provider selection
physical lookup
retrieval execution
domain-object hydration
```

---

##### `src/index/mod.rs`

Owns the public Indexing data-model boundary.

It should export the Phase 2 index contracts:

```text
IndexDefinition
IndexEntry
ObjectReference
SimilarityEntry
IndexVersion
QueryRequest
QueryResult
```

It should remain a module boundary rather than becoming a large implementation file.

---

##### `src/requirement/requirement.rs`

Owns:

```text
IndexRequirement
```

and the logical validation / normalization needed to convert a requirement into a valid `IndexDefinition`.

The conceptual flow is:

```text
Source Engine
      ↓
IndexRequirement
      ↓
validate
      ↓
normalize
      ↓
IndexDefinition
```

This module is the source-to-Indexing logical contract boundary.

It must not contain physical provider configuration.

---

##### `src/requirement/mod.rs`

Owns the public requirement boundary.

Responsibilities:

```text
public exports
requirement-level interaction tests where appropriate
```

It should not become a large implementation file.

---

##### `src/lib.rs`

Must expose the Phase 2 modules cleanly:

```text
nizaam_indexing
    │
    ├── identity
    ├── index
    └── requirement
```

Existing Core integration must remain untouched unless Phase 2 implementation requires a narrowly scoped public integration change.

---

#### 14. Relationship Semantics Must Remain Outside Indexing

Phase 1 explicitly established that the Relationship family does not give Indexing ownership of relationship semantics.

Phase 2 must preserve this boundary.

Do not introduce:

```text
RelationshipEntry
SemanticRelationship
RelationshipPredicate
RelationshipDimension
MappingFamily
MappingDimension
Ontology
InferenceRule
```

as Indexing-owned semantic models.

A source engine may own:

```text
A --CAUSES--> B
```

and provide indexable key material or a source-owned relationship record.

Indexing sees only the generic indexing representation:

```text
source reference
+
key material
+
target reference
```

Indexing does not decide:

```text
what CAUSES means
```

That meaning remains source-owned.

---

#### 15. Source-Owned Categories

Phase 2 must remain generic for source-owned categories such as:

```text
Word
Mention / Occurrence
Sentence / Span
Passage
Document
Semantic record
Context record
Relationship record
```

These remain source-engine concepts.

Do not introduce:

```rust
struct QuranWord { ... }
struct HadithRecord { ... }
struct SemanticMapping { ... }
struct ContextRecord { ... }
```

as Indexing-owned domain models.

The Indexing Engine should only need the generic information required to index and reference them.

---

#### 16. Identity Separation Invariants

Phase 2 must preserve all Phase 1 identity boundaries.

At minimum:

```text
Object Identity
    ≠
Index Identity
```

```text
Index Identity
    ≠
Index Definition Identity
```

```text
Index Namespace
    ≠
Object Identity
```

```text
Index Namespace
    ≠
Physical Partition
```

```text
Index Family
    ≠
Semantic Mapping Family
```

```text
Relationship Index
    ≠
Relationship Semantics
```

```text
ObjectReference
    ≠
new canonical Object Identity
```

```text
Index Version
    ≠
Core Contract Version
```

```text
Logical Index Definition
    ≠
Physical Index Implementation
```

These are architectural invariants, not merely implementation preferences.

---

#### 17. Version Separation Invariants

Phase 2 must keep the following concepts separate:

```text
Core Contract Version
        ≠
Source Version
        ≠
Schema Version
        ≠
Index Version
```

Their responsibilities are:

```text
Core Contract Version
    → compatibility of Core contracts

Source Version
    → version/state of source-owned data

Schema Version
    → structure/version compatibility of indexed data

Index Version
    → version/state of the logical index
```

No implementation should collapse these into one generic version field merely for convenience.

---

#### 18. Physical Implementation Boundary

Phase 2 must remain independent of physical indexing technology.

Callers must not need to know whether a later implementation uses:

```text
B-tree
hash table
FST
HNSW
IVF
Lucene
FAISS
PostgreSQL
MongoDB
other provider
```

The logical contracts must remain valid regardless of the eventual physical implementation.

Therefore:

```text
Logical Index
    ≠
Physical Index
```

and:

```text
Logical Query
    ≠
Physical Query Execution
```

and:

```text
IndexEntry
    ≠
Database Row
```

---

#### 19. What Phase 2 Must NOT Implement

The following are explicitly deferred:

```text
physical B-tree implementation
physical hash index
FST
HNSW
IVF
Lucene
FAISS
PostgreSQL indexing
MongoDB indexing
vector database integration
physical persistence
storage provider selection
partitioning
sharding
physical replication
```

Also deferred:

```text
index builder
index update executor
incremental ingestion
batch ingestion
rebuild
candidate publication
active-version switching
query planner
retrieval executor
provider selection
physical lookup
```

Also deferred:

```text
embedding generation
embedding model selection
similarity algorithm selection
semantic mapping creation
semantic predicate interpretation
domain inference
domain ontology
domain object hydration
```

The existence of future-phase scaffolding does not mean those features should be implemented in Phase 2.

---

#### 20. Control Plane Boundary

The Control Plane remains responsible for global coordination and routing.

Indexing remains responsible for local indexing contracts and later local indexing work.

Phase 2 must not introduce:

```text
global scheduler
global workflow engine
cross-engine planner
routing policy
destination selection
transport implementation
```

`IndexRequirement` and `QueryRequest` are data contracts.

They are not:

```text
schedulers
planners
workflows
coordination systems
```

This distinction must remain explicit.

---

#### 21. Core Integration Boundary

Core already provides the execution contract.

The conceptual flow is:

```text
Core UniversalRequest
        ↓
Core capability dispatch
        ↓
Indexing capability
        ↓
opaque payload
        ↓
IndexRequirement / QueryRequest
```

Core should not become aware of the internal Indexing data model merely because the payload eventually contains an Indexing request.

The separation remains:

```text
Core
    → execution contract

Indexing
    → indexing data contract
```

Phase 2 must not reimplement:

```text
EngineRuntime
CapabilityRegistry
CapabilityHandler
dispatch
lifecycle
security
deadline handling
cancellation
```

---

#### 22. Test Architecture

Phase 2 tests should prove the logical contracts and architectural boundaries.

The planned test structure is:

```text
tests/

├── requirement.rs
├── index.rs
├── identity.rs
├── integration.rs
└── conformance.rs
```

Source-local unit tests must also exist in each implementation file where behavior is testable.

---

#### 23. Unit Tests

##### `index/definition.rs`

Cover:

```text
valid construction
invalid construction
required field validation
namespace association
family association
key-definition association
target-reference-type association
uniqueness
consistency
source/schema version metadata
lifecycle metadata
definition identity separation
```

---

##### `index/entry.rs`

Cover:

```text
valid construction
invalid construction
key material access
target reference access
logical metadata
entry equality where exposed
reference-only behavior
```

---

##### `index/reference.rs`

Cover:

```text
valid reference construction
invalid reference construction
source/owner identity
opaque object identity/reference
reference equality
different source ownership remains distinct
```

---

##### `index/similarity.rs`

Cover:

```text
valid similarity entry
invalid similarity entry
representation access
target reference access
generic metadata
similarity remains distinct from relationship semantics
```

---

##### `index/version.rs`

Cover:

```text
valid version construction
invalid version construction
version identity
source version association
schema version association
version comparison where exposed
separation from Core contract version
```

---

##### `index/query.rs`

Cover:

```text
valid QueryRequest
invalid QueryRequest
valid QueryResult
reference-only result behavior
optional score
optional distance
index identity
index version
continuation metadata where implemented
```

---

##### `requirement/requirement.rs`

Cover:

```text
valid requirement
invalid requirement
key-definition validation
reference-type validation
uniqueness validation
consistency validation
source/schema version validation
requirement normalization
requirement → definition compatibility
physical-provider request rejection
```

---

#### 24. Repository-Level Integration Tests

##### `tests/identity.rs`

Phase 1 identity behavior must remain valid after Phase 2 is added.

Verify:

```text
IndexId remains separate from IndexDefinitionId
IndexNamespace remains separate from ObjectReference
IndexNamespace remains separate from Object Identity
IndexVersion remains separate from Core Contract Version
multiple logical spaces can coexist
```

---

##### `tests/index.rs`

Verify:

```text
IndexDefinition can compose Phase 1 identities
IndexEntry can reference generic key material
IndexEntry can reference ObjectReference
SimilarityEntry remains generic
IndexVersion composes with IndexDefinition
QueryRequest remains provider-neutral
QueryResult remains reference-only
```

Also verify:

```text
Relationship family does not create semantic predicates
Similarity does not become relationship semantics
domain objects are not introduced into Indexing
```

---

##### `tests/requirement.rs`

Verify:

```text
source engine can express a valid logical indexing requirement
invalid requirements are rejected
requirements remain provider-neutral
requirements can normalize into logical definitions
requirements do not contain physical provider configuration
```

---

##### `tests/integration.rs`

Verify the complete Phase 2 logical composition:

```text
IndexRequirement
        ↓
validation
        ↓
IndexDefinition
        ↓
IndexEntry
        ↓
ObjectReference
```

and:

```text
IndexDefinition
        ↓
IndexVersion
        ↓
QueryRequest
        ↓
QueryResult
        ↓
ObjectReference
```

The integration tests must verify that these contracts compose without introducing physical execution.

---

##### `tests/conformance.rs`

Verify the architectural boundaries expected by downstream consumers.

The conformance tests should demonstrate:

```text
source-owned requirement
        ↓
logical IndexDefinition
        ↓
generic IndexEntry
        ↓
generic ObjectReference
```

and:

```text
logical QueryRequest
        ↓
generic QueryResult
        ↓
ObjectReference
```

They must also demonstrate rejection or absence of:

```text
physical provider configuration
domain object hydration
semantic predicate ownership
embedding generation requirements
```

---

#### 25. Required Negative Tests

Phase 2 requires explicit negative architectural tests.

At minimum, tests must demonstrate that the implementation does not imply:

```text
IndexDefinition == Physical Index Configuration
```

```text
IndexEntry == Database Row
```

```text
ObjectReference == Domain Object
```

```text
ObjectReference == New Canonical Object Identity
```

```text
QueryResult == Domain Object
```

```text
Relationship Family == Semantic Predicate Model
```

```text
Similarity Entry == Relationship Model
```

```text
Index Version == Core Contract Version
```

```text
Indexing == KG
```

```text
Indexing == Domain Storage
```

```text
Indexing == Embedding Generator
```

```text
QueryRequest == Query Executor
```

The negative tests are important because the major risk in Phase 2 is not missing a struct.

The major risk is allowing implementation details or domain semantics to leak into the logical contracts.

---

#### 26. Examples of Valid Phase 2 Composition

A source engine may conceptually provide:

```text
source object:
    object-id = X

namespace:
    kg

family:
    relationship

key material:
    source = X
    predicate = P
    target = Y
```

The source engine can express an:

```text
IndexRequirement
```

which becomes a canonical:

```text
IndexDefinition
```

and an indexed record can be represented as:

```text
IndexEntry
    ↓
key material
    ↓
ObjectReference
```

Indexing does not need to know what `P` means.

---

A different source object may participate in another logical space:

```text
source object:
    object-id = X

namespace:
    lexical

family:
    inverted
```

The same source-owned object may therefore participate in multiple logical indexing spaces without receiving a new canonical identity.

---

A similarity source may provide:

```text
representation
+
target reference
```

which becomes:

```text
SimilarityEntry
```

without Indexing knowing:

```text
which model produced the representation
which embedding provider was used
which physical vector structure will store it
```

---

A query may produce:

```text
QueryResult
    ↓
ObjectReference(X)
    ↓
score = ...
```

The result refers to the source-owned object.

Indexing does not load or return the actual domain object.

---

#### 27. Dependency Direction

The dependency direction must remain:

```text
Source / Domain Engine
        ↓
IndexRequirement / source-owned record
        ↓
Indexing logical model
        ↓
Later Indexing implementation
        ↓
Physical provider
```

and not:

```text
Indexing
        ↓
domain semantics
        ↓
source engine
```

Indexing must remain reusable by multiple source engines.

---

#### 28. Phase 2 Implementation Sequence

The implementation order should be:

```text
Step 1
Phase 1 identity foundation
        ↓
verify existing Phase 1 behavior

Step 2
src/index/reference.rs
        ↓
ObjectReference

Step 3
generic key-definition / key-material representation
        ↓
shared logical key contract

Step 4
src/index/entry.rs
        ↓
IndexEntry

Step 5
src/index/similarity.rs
        ↓
SimilarityEntry

Step 6
src/index/version.rs
        ↓
IndexVersion

Step 7
src/index/definition.rs
        ↓
IndexDefinition

Step 8
src/requirement/requirement.rs
        ↓
IndexRequirement
        ↓
validation / normalization

Step 9
src/index/query.rs
        ↓
QueryRequest
QueryResult

Step 10
module exports
        ↓
src/index/mod.rs
src/requirement/mod.rs
src/lib.rs

Step 11
source-local unit tests
        ↓
contract verification

Step 12
repository-level integration tests
        ↓
public composition verification

Step 13
conformance / negative architectural tests
        ↓
boundary verification
```

The sequence should keep lower-level generic representations available before higher-level contracts depend on them.

---

#### 29. Architectural Invariants

After Phase 2, the following must remain true:

```text
Object Identity
    ≠
Index Identity
```

```text
Index Identity
    ≠
Index Definition Identity
```

```text
Index Namespace
    ≠
Physical Partition
```

```text
Index Namespace
    ≠
Object Identity
```

```text
Index Family
    ≠
Semantic Mapping Family
```

```text
Relationship Index
    ≠
Relationship Semantics
```

```text
ObjectReference
    ≠
Domain Object
```

```text
ObjectReference
    ≠
Canonical Object Identity Owner
```

```text
Index Version
    ≠
Source Version
```

```text
Index Version
    ≠
Schema Version
```

```text
Index Version
    ≠
Core Contract Version
```

```text
Logical Index Definition
    ≠
Physical Index Implementation
```

```text
Logical Query
    ≠
Physical Query Execution
```

```text
Query Result
    ≠
Domain Object Hydration
```

```text
Indexing
    ≠
KG
```

```text
Indexing
    ≠
Domain Storage
```

```text
Indexing
    ≠
Embedding Generation
```

```text
Core Runtime
    ≠
Indexing Runtime Reimplementation
```

---

#### 30. Important Deferred Decisions

The following decisions must remain open unless explicitly approved:

```text
exact physical index representation
physical storage provider
provider deployment topology
partitioning
sharding
replication
physical indexing algorithm
hash/index generation algorithm
embedding provider
embedding model
similarity algorithm
serialization technology
query planner implementation
retrieval strategy
provider selection policy
rebuild strategy
publication mechanism
active-version switching mechanism
```

Phase 2 may establish the metadata required to support these decisions later.

It must not silently freeze the decisions themselves.

---

#### Completion Criteria

Phase 2 is complete when:

#### Verification Checklist

- [ ] IndexRequirement exists as a genuine source-to-Indexing logical contract.

- [ ] IndexDefinition exists as a genuine canonical logical definition.

- [ ] IndexRequirement and IndexDefinition remain distinct concepts.

- [ ] Generic key definition / key material is representable.

- [ ] IndexEntry exists as a generic logical key-to-reference association.

- [ ] ObjectReference exists as a generic reference to a source-owned object.

- [ ] ObjectReference does not hydrate or own domain objects.

- [ ] SimilarityEntry exists as a generic similarity indexing representation.

- [ ] Similarity remains separate from relationship semantics.

- [ ] IndexVersion exists and remains distinct from source, schema, and Core
  contract versions.

- [ ] QueryRequest exists as a provider-neutral logical retrieval contract.

- [ ] QueryResult exists as a generic reference-oriented result contract.

- [ ] QueryResult does not own or hydrate domain objects.

- [ ] IndexDefinition composes correctly with Phase 1 identity types.

- [ ] Namespace and family remain separate from physical implementation.

- [ ] Relationship indexing does not introduce semantic predicate ownership.

- [ ] Source-owned categories remain outside Indexing domain models.

- [ ] Requirement validation rejects invalid logical contracts.

- [ ] Requirement normalization can produce a valid logical definition.

- [ ] Physical provider configuration cannot leak into the logical contracts.

- [ ] Query execution is not implemented as part of Phase 2.

- [ ] Index construction is not implemented as part of Phase 2.

- [ ] Rebuild and publication are not implemented as part of Phase 2.

- [ ] Storage and persistence are not implemented as part of Phase 2.

- [ ] Domain-object hydration is not implemented as part of Phase 2.

- [ ] Core runtime mechanisms are reused rather than reimplemented.

- [ ] Unit tests cover the Phase 2 implementation behavior.

- [ ] Integration tests cover public cross-module composition.

- [ ] Negative conformance tests protect the architectural boundaries.

- [ ] Previously verified Phase 1 behavior remains intact.

- [ ] No physical indexing technology has leaked into the logical contracts.

Completion does not mean that the Indexing Engine can yet build or query a real
physical index.

It means that the logical contracts required by those later phases are now
well-defined and independently testable.

---

#### Final Phase 2 Mental Model

After Phase 2, the Indexing Engine should be able to answer:

```text
What is the source engine asking Indexing to provide?
        ↓
IndexRequirement

What is the canonical logical definition?
        ↓
IndexDefinition

What key material is being indexed?
        ↓
Generic Key Definition / Key Material

What indexed association is represented?
        ↓
IndexEntry

What source-owned object does the entry refer to?
        ↓
ObjectReference

How is a similarity entry represented?
        ↓
SimilarityEntry

Which logical version does the index represent?
        ↓
IndexVersion

What logical retrieval is being requested?
        ↓
QueryRequest

What did Indexing logically return?
        ↓
QueryResult
        ↓
ObjectReference
```

The complete conceptual model is:

```text
                         SOURCE ENGINE
                              │
                              │
                    source-owned object
                              │
                              ▼
                    IndexRequirement
                              │
                       validation
                              │
                       normalization
                              ▼
                     IndexDefinition
                              │
             ┌────────────────┼────────────────┐
             │                │                │
             ▼                ▼                ▼
        IndexEntry      IndexVersion     QueryRequest
             │                                 │
             ▼                                 ▼
      ObjectReference                    QueryResult
                                              │
                                              ▼
                                      ObjectReference
```

Similarity remains a parallel logical representation:

```text
Similarity representation
        +
ObjectReference
        ↓
SimilarityEntry
```

The ownership model remains:

```text
Source / Domain Engine
    → owns meaning and source objects

Core
    → owns runtime, capability, execution, and universal communication

Indexing
    → owns logical indexing contracts and reference-oriented indexing data

Future Provider
    → owns physical implementation
```

The phase boundary remains:

```text
Phase 1
    identity + namespace + family + definition identity
        ↓
Phase 2
    logical data contracts
        ↓
Phase 3
    index construction / update / version lifecycle
        ↓
Phase 4
    query planning / retrieval
        ↓
Later phases
    provider execution / persistence / operational hardening
```

Phase 2 therefore completes the **logical data-model foundation** of the
Indexing Engine without prematurely committing the system to a physical
indexing technology, storage provider, query execution strategy, or domain
semantic model.

---

### Phase 3: Index Construction, Update, Batch & Version Publication

#### Status

**Not Started**

##### 1. What Phase 3 actually is

Phase 1 established:

```text
Index Identity
Index Namespace
Index Family
Index Definition Identity
Logical Index Space

```

Phase 2 established:

```text
IndexRequirement
IndexDefinition
IndexEntry
ObjectReference
SimilarityEntry
IndexVersion
QueryRequest
QueryResult
Generic Key Material

```

Phase 3 now takes those logical contracts and introduces the first real **index state transition and construction mechanics**.

The central transition is:

```text
Phase 2
logical index model
        ↓
Phase 3
index construction / update / publication
        ↓
Phase 4
query planning / retrieval

```

The Phase 3 goal is therefore:

> **Build and maintain index versions from source-owned indexable data without corrupting the currently active index.**

The important word is **versions**.

Phase 3 must not treat an index as one mutable object that is destroyed and recreated whenever a rebuild occurs.

Instead:

```text
IndexDefinition
      ↓
Index Version
      ↓
Candidate Version
      ↓
Validation
      ↓
Publication
      ↓
Active Version

```

This allows a new index version to be constructed independently while the existing active version remains usable.

The Phase 3 scope explicitly requires:

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

and explicitly separates build/update operations from query operations.

---

#### 2. Phase 3 architectural position

The complete infrastructure flow remains:

```text
Nizaam Control Plane
        ↓
Indexing Engine
        ↓
Core Engine Runtime
        ↓
Core Capability Dispatch
        ↓
Indexing-local Workflow / Planner
        ↓
Index Construction / Update
        ↓
Index Provider / Backend Boundary
        ↓
Index Result / Version State

```

The shared Infrastructure Engine architecture must continue to provide the common runtime structure.

Phase 3 does **not** create a second runtime.

It does not recreate:

```text
EngineRuntime
CapabilityRegistry
CapabilityHandler
OperationContext
EngineContext
Lifecycle
Security
Cancellation
Deadline
Control Plane

```

Core already owns these domain-neutral mechanisms.

The Indexing Engine owns the indexing-specific work performed after capability dispatch.

Core's capability system deliberately passes opaque invocation payloads into an engine-owned handler. Core therefore does not need to understand the semantics of:

```text
IndexRequirement
IndexDefinition
IndexEntry
Build request
Update request
Rebuild request
Publication request

```

Those remain Indexing concerns.

The architectural distinction remains:

```text
Core
    → provides execution infrastructure

Control Plane
    → coordinates globally

Indexing
    → performs index-local construction/update/publication work

Physical Provider
    → performs physical index/storage operations

```

---

#### 3. Phase 3's central architectural principle

The most important Phase 3 invariant is:

```text
ACTIVE VERSION
      │
      │ remains usable
      ▼
BUILD CANDIDATE
      │
      ▼
VALIDATE
      │
      ▼
READY
      │
      ▼
PUBLISH
      │
      ▼
NEW ACTIVE VERSION

```

A rebuild must never behave like:

```text
delete active
      ↓
build new
      ↓
hope it works

```

That would create an availability and integrity boundary violation.

Instead:

```text
active-v1
    │
    ├──────────────→ remains available
    │
    ▼
candidate-v2
    │
    ├── populate
    ├── update
    ├── validate
    │
    ▼
ready-v2
    │
    ▼
publish
    │
    ▼
active-v2

```

If candidate construction fails:

```text
active-v1
    │
    └── remains active

```

The failed candidate must not replace the last known-good version.

This is explicitly required by the Phase 3 scope.

---

#### 4. Phase 3 must use the Phase 2 contracts rather than redefine them

Phase 3 should consume the logical model established in Phase 2.

The dependency should be:

```text
IndexRequirement
        ↓
IndexDefinition
        ↓
IndexVersion
        ↓
Build / Update

```

and:

```text
IndexEntry
        ↓
construction/update input

```

and:

```text
ObjectReference
        ↓
indexed target

```

The build layer should not create a second representation of:

```text
IndexDefinition
IndexEntry
ObjectReference
IndexVersion

```

merely because construction needs to operate on them.

Phase 2 defines the logical contracts.

Phase 3 defines the lifecycle and mutation mechanics around those contracts.

---

#### 5. What "create" means in Phase 3

The `create` operation must be understood carefully.

It does not mean:

```text
create a physical database table

```

or:

```text
create a provider-specific index immediately

```

Instead, creation should establish a new logical index lifecycle beginning from the approved `IndexDefinition`.

Conceptually:

```text
IndexRequirement
      ↓
IndexDefinition
      ↓
create index resource
      ↓
create initial version
      ↓
build/populate
      ↓
validate
      ↓
publish

```

The exact physical representation remains behind the provider boundary.

The logical Indexing layer should be able to describe:

```text
this index exists
this definition governs it
this version is being constructed
this version is ready
this version is active

```

without requiring callers to understand the physical provider.

---

#### 6. Build and populate are different responsibilities

Phase 3 includes both:

```text
build
populate

```

These should not automatically be treated as the same concept.

Conceptually:

```text
Build
    → construct the index according to an IndexDefinition

Populate
    → provide indexable entries/data to that construction

```

A build operation may therefore consume:

```text
IndexDefinition
+
source snapshot / indexable entries

```

and produce:

```text
candidate IndexVersion

```

while population is the mechanism through which:

```text
IndexEntry

```

records become part of that candidate.

The exact provider implementation remains deferred.

The logical lifecycle should remain independent of whether the provider internally performs:

```text
bulk insertion
sorting
partition construction
segment creation
vector ingestion
tree construction

```

or another technique.

---

#### 7. Source snapshot is the construction boundary

The Phase 3 lifecycle explicitly begins with:

```text
source snapshot
        ↓
build candidate

```

This is important because construction needs a coherent view of source-owned data.

The Indexing Engine should not assume that a source engine is a static database.

Source data can change while an index is being built.

Therefore:

```text
source state at build start
        ↓
snapshot / declared source version
        ↓
candidate index version

```

The source version established by Phase 2 becomes meaningful here.

The build must be associated with the source state it represents.

Conceptually:

```text
Source Version S1
       ↓
Build Candidate C1
       ↓
Index Version I1

```

If the source has moved to:

```text
Source Version S2

```

during or before publication, the consistency rules must determine whether:

```text
I1

```

is still publishable.

Those compatibility and publication rules belong to the Phase 3 consistency boundary.

---

#### 8. IndexVersion becomes operational in Phase 3

Phase 2 introduced `IndexVersion` as a logical version concept.

Phase 3 gives it lifecycle meaning.

The important states are conceptually:

```text
candidate
building
validating
ready
active

```

The exact final state representation should follow the approved Phase 2/3 model rather than inventing unnecessary states.

The critical distinction is:

```text
candidate
    ≠
active

```

A candidate is not automatically visible as the active index.

Similarly:

```text
ready
    ≠
active

```

A candidate must pass the publication boundary before becoming active.

---

#### 9. Active and candidate versions

The central version model should conceptually be:

```text
Index
 │
 ├── Definition
 │
 ├── Active Version
 │
 └── Candidate Version(s)

```

The active version represents:

```text
the currently published usable index state

```

A candidate represents:

```text
a version under construction or validation

```

This separation is essential for safe rebuilds.

For example:

```text
Index I
    │
    ├── active = V1
    │
    └── candidate = V2

```

During the rebuild:

```text
V1 → remains active
V2 → being constructed

```

After successful validation:

```text
V1 → previous active
V2 → ready

```

After publication:

```text
V2 → active
V1 → previous version

```

The old version must not be destroyed merely because a new candidate is being built.

---

#### 10. Incremental update

Incremental update is different from full rebuild.

Conceptually:

```text
Existing Active Version
        +
small source change
        ↓
incremental update
        ↓
updated index state

```

Phase 3 must support:

```text
create
update
delete

```

operations against indexed records within the declared consistency model.

Examples of logical changes include:

```text
new IndexEntry
existing IndexEntry changed
existing IndexEntry removed
ObjectReference no longer present
key material changed
source version advanced

```

The update layer must preserve the logical contracts established by Phase 2.

It must not reinterpret domain semantics.

---

#### 11. Delete/update behavior

A source engine may indicate that an indexed object or record has changed.

Indexing should operate on the indexing representation.

For example:

```text
ObjectReference X
old key material

```

may become:

```text
ObjectReference X
new key material

```

The Indexing Engine is responsible for updating the index representation.

It is not responsible for deciding why the source object changed.

Similarly, deletion means:

```text
remove the indexed representation/reference

```

not:

```text
delete the source-owned domain object

```

This boundary must remain explicit.

---

#### 12. Batch updates

Phase 3 explicitly introduces bounded batch operations.

A batch should represent a controlled group of logical index mutations.

Conceptually:

```text
Batch
├── entry 1
├── entry 2
├── entry 3
├── ...
└── entry N

```

The important property is that the batch is **bounded**.

The Indexing Engine must not assume unlimited in-memory ingestion.

Batch processing should therefore establish a boundary around:

```text
input size
processing pressure
failure reporting
consistency behavior

```

The exact resource thresholds are not established by the supplied Phase 3 architecture and therefore should not be silently invented at this stage.

The implementation should expose the concept without prematurely freezing arbitrary limits.

---

#### 13. Incremental update vs batch update

These concepts should remain separate.

```text
Incremental Update
    → semantic operation type

Batch Update
    → grouping / bounded execution mechanism

```

Therefore:

```text
one update

```

and:

```text
batch of updates

```

can both exist.

A batch may contain:

```text
create
update
delete

```

operations according to the declared contract.

The batch mechanism must not become a second workflow engine.

It is an Indexing-local construction/update mechanism.

---

#### 14. Rebuild is not an update

A rebuild should be treated as a distinct operation.

Update:

```text
existing index
    ↓
apply changes

```

Rebuild:

```text
source snapshot
    ↓
construct new candidate version

```

The rebuild exists precisely because the active index may need to be reconstructed without corrupting it.

Conceptually:

```text
ACTIVE V1
    │
    │ remains untouched
    │
    ▼
BUILD V2 FROM SNAPSHOT
    │
    ▼
VALIDATE V2
    │
    ▼
PUBLISH V2

```

This is the core safety mechanism of Phase 3.

---

#### 15. Rebuild isolation

The Phase 3 scope explicitly requires rebuild isolation.

The fundamental invariant is:

```text
candidate construction failure
        ↓
active version unchanged

```

For example:

```text
active = V7

rebuild V8
    ↓
failure

```

must result in:

```text
active = V7

```

not:

```text
active = none

```

and not:

```text
active = partially-built V8

```

This is one of the most important Phase 3 conformance properties.

---

#### 16. Validation is a separate lifecycle stage

The construction lifecycle is:

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

Validation therefore must happen **before publication**.

It should verify logical/index-level invariants appropriate to the declared model.

Potential categories include:

```text
candidate version is internally coherent
definition matches candidate
source/schema versions are compatible
required index entries are valid
reference structure is valid
candidate lifecycle state is valid
candidate is complete enough for publication

```

The exact physical integrity checks remain provider-specific.

Phase 3 should not pretend to define provider-specific validation algorithms.

---

#### 17. Validation must not become query execution

Validation should not leak into Phase 4.

Phase 3 validation answers:

```text
Can this candidate safely become an active index version?

```

It does not answer:

```text
How should a query be planned?
Which index should a query use?
How should results be ranked?
How should retrieval be filtered?

```

Those belong to Phase 4.

Therefore:

```text
Build validation
    ≠
Query planning

```

and:

```text
Candidate validation
    ≠
Retrieval

```

---

#### 18. Publication is the critical state transition

Publication is where:

```text
ready candidate

```

becomes:

```text
active version

```

Conceptually:

```text
candidate V2
    ↓
validated
    ↓
ready
    ↓
publish
    ↓
active V2

```

The publication operation must be controlled.

It must not expose a partially-built candidate.

The required invariant is:

```text
Consumers see either:
    previous valid active version

or:
    newly published valid version

```

They must not see:

```text
half-built candidate
invalid candidate
failed candidate

```

The exact atomicity mechanism depends on the later provider/runtime design and should not be prematurely hard-coded into the logical Phase 3 contract.

---

#### 19. Failed publication

A failed publication must preserve the last known-good active version.

Conceptually:

```text
active = V1

candidate = V2
V2 validates
publication fails

```

The resulting state should be:

```text
active = V1
candidate = failed / unpublished V2

```

not:

```text
active = V2

```

and not:

```text
active = none

```

The previous active version therefore acts as the safety boundary.

---

#### 20. Version compatibility

Phase 3 introduces actual use of:

```text
source_version
schema_version
index_version

```

These must remain distinct.

Conceptually:

```text
Source Version
      ↓
describes source state

Schema Version
      ↓
describes structure compatibility

Index Version
      ↓
describes constructed index state

```

A candidate should carry enough metadata to determine whether it is compatible with the source/schema state against which it was constructed.

This is the purpose of:

```text
consistency/versioning.rs

```

The Phase 3 scope explicitly assigns version compatibility and publication consistency rules to this area.

---

#### 21. `consistency/versioning.rs`

This module should own the version compatibility rules needed by construction and publication.

Conceptually it answers:

```text
Is this candidate compatible with its source version?

Is the schema compatible?

Can this candidate be published?

Does the candidate represent a valid successor to the active version?

Does the source state make this candidate stale or invalid?

```

It must not become the query consistency system prematurely.

Phase 4 later introduces:

```text
fresh/current index
version-aware query
stale-index behavior
consistency requirements
synchronization

```

Phase 3 only establishes the consistency necessary to safely construct and publish index versions.

---

#### 22. Build orchestration belongs in `builder.rs`

The Phase 3 scope assigns:

```text
src/build/builder.rs

```

to index construction orchestration.

This module should conceptually coordinate:

```text
IndexDefinition
        ↓
source snapshot
        ↓
candidate version
        ↓
population
        ↓
validation
        ↓
ready

```

It should not become:

```text
global scheduler

```

or:

```text
Control Plane

```

Its scope is Indexing-local construction.

---

#### 23. `build/update.rs`

This module owns incremental create/update/delete operations.

Conceptually:

```text
Update Request
      ↓
validate logical mutation
      ↓
apply mutation
      ↓
maintain version consistency

```

Possible operations:

```text
insert/create
update
delete

```

The module must operate on Indexing's generic logical records and references.

It must not understand:

```text
Quran semantics
Arabic semantics
KG ontology
Hadith semantics
Fiqh semantics

```

The source engine remains the semantic authority.

---

#### 24. `build/batch.rs`

This module owns bounded batch ingestion/update behavior.

Conceptually:

```text
Batch
    ↓
validate
    ↓
process bounded group
    ↓
report success/failure

```

It must preserve the declared consistency semantics.

The batch layer must not silently turn an unbounded stream into a giant in-memory collection.

The Phase 3 scope specifically includes bounded batch pressure in its test requirements.

---

#### 25. `build/rebuild.rs`

This module owns the major rebuild workflow.

Its conceptual responsibility is:

```text
active version
      │
      │ remains usable
      ▼
source snapshot
      ↓
new candidate version
      ↓
build
      ↓
validate
      ↓
ready
      ↓
publication boundary

```

The rebuild module must not directly replace the active version before validation.

It should coordinate with:

```text
builder
versioning
publication

```

rather than duplicate their responsibilities.

---

#### 26. `build/publication.rs`

This module owns the transition:

```text
ready
    ↓
publish
    ↓
active

```

Its responsibility is to protect the publication boundary.

Conceptually:

```text
Candidate
    ↓
validate publication eligibility
    ↓
publish
    ↓
new active version

```

It should also preserve the previous active version if publication fails.

Publication should therefore be treated as a state transition rather than as a generic write operation.

---

#### 27. `index/version.rs`

Phase 2 established the logical `IndexVersion`.

Phase 3 extends its lifecycle semantics.

The version model should support concepts such as:

```text
active version
candidate version
previous version
version state
source version
schema version

```

The exact state representation should remain aligned with the approved contract rather than introducing unnecessary version states.

The important invariant is:

```text
one active logical version
+
zero or more construction candidates as allowed

```

The exact concurrency policy should follow the Phase 3 architecture and should not be silently expanded beyond what is required.

---

#### 28. Core runtime integration

The Core codebase provides the execution mechanisms Phase 3 should reuse.

Core's capability handler receives:

```text
EngineContext
CapabilityInvocation

```

and returns:

```text
CapabilityOutcome

```

The capability payload remains opaque to Core.

Therefore a future build capability can conceptually receive:

```text
CapabilityInvocation
        ↓
Indexing handler
        ↓
decode build/update request
        ↓
Indexing-local workflow

```

Core continues to handle:

```text
operation context
cancellation
deadline
capability dispatch
runtime lifecycle

```

Indexing handles:

```text
build
update
batch
rebuild
validation
publication

```

This keeps the Core/Indexing boundary clean. Core's capability dispatch itself performs admission-related checks such as cancellation/deadline handling before invoking the handler.

---

#### 29. Cancellation and deadline behavior

Because Phase 3 operates through the shared Core runtime, long-running construction operations should respect the existing Core execution context.

The Indexing build layer should not create its own independent cancellation system.

Conceptually:

```text
Core OperationContext
        │
        ├── cancellation
        ├── deadline
        └── attempt
                ↓
        Indexing build operation

```

This matters particularly for:

```text
large builds
batch updates
rebuilds
publication workflows

```

If a build is cancelled, the resulting candidate must not accidentally become active merely because part of the build completed.

The candidate lifecycle must remain explicit.

---

#### 30. Attempt identity remains Core-owned

Phase 3 must not introduce a new Indexing attempt identity that duplicates:

```text
AttemptId

```

Core already distinguishes:

```text
OperationId
AttemptId

```

The Indexing version model is different:

```text
IndexVersion

```

Therefore:

```text
OperationId
    ≠
AttemptId
    ≠
IndexVersion

```

An operation may have multiple attempts while targeting the same logical index/version lifecycle.

Similarly, a candidate index version is not equivalent to a runtime execution attempt.

---

#### 31. Provider boundary

Phase 3 is the first phase that approaches physical execution, but the logical Indexing architecture must still remain provider-neutral.

The conceptual flow is:

```text
Indexing Build Workflow
        ↓
Provider / Backend Abstraction
        ↓
Physical implementation

```

The Indexing Engine owns:

```text
logical build lifecycle
logical update semantics
version management
validation boundary
publication boundary

```

The provider owns:

```text
physical index construction
physical storage
physical mutation
physical persistence

```

The caller must not need to know whether the provider uses:

```text
B-tree
HNSW
IVF
FST
Lucene
FAISS
PostgreSQL
MongoDB

```

or another implementation.

The scope explicitly maintains the rule that physical index technology is not part of the Indexing architectural contract.

##### Top-level provider abstraction

The provider abstraction is a **top-level Indexing module** shared by the build,
update, query, retrieval, and recovery paths. It must not be duplicated or
nested independently under each workflow module.

Conceptually:

```text
src/
├── provider/
│   ├── mod.rs
│   └── ranking.rs
│
├── build/
├── query/
├── consistency/
├── recovery/
└── ...
```

The exact internal decomposition may evolve, but the provider abstraction itself
remains one shared top-level boundary.

The provider abstraction owns generic provider-facing mechanisms, including
generic ranking mechanisms. Generic ranking may operate on provider/index result
data, scores, ordering, or other provider-neutral retrieval information, but it
must not encode domain relevance, semantic authority, or domain-specific
ranking policy.

The ownership boundary is:

```text
Source / Domain Engine
    → owns domain meaning and domain-specific relevance/ranking policy

Indexing
    → owns logical query requirements and retrieval orchestration

Provider Abstraction
    → owns reusable generic provider mechanisms and generic ranking

Physical Provider
    → owns physical index/storage implementation
```

This avoids duplicate provider code across build/query/recovery modules and
prevents unnecessary circular dependencies between those modules.

---

#### 32. Indexing-local workflow is not a global workflow engine

The Phase 3 build workflow may have several stages:

```text
snapshot
    ↓
build
    ↓
populate
    ↓
validate
    ↓
ready
    ↓
publish

```

That does not mean Indexing should implement:

```text
GlobalWorkflowEngine
GlobalScheduler
GlobalControlPlane

```

The distinction remains:

```text
Control Plane
    → global coordination

Indexing Workflow
    → local construction lifecycle

```

This follows the shared infrastructure architecture and the Indexing scope's explicit statement that the Control Plane coordinates globally while Indexing performs local planning and execution.

---

#### 33. Phase 3 dependency direction

The dependency direction remains:

```text
Source Engine
        ↓
IndexRequirement
        ↓
IndexDefinition
        ↓
source snapshot / indexable records
        ↓
Indexing Build
        ↓
IndexVersion
        ↓
validation
        ↓
publication
        ↓
active index
        ↓
physical provider

```

The reverse direction must not be introduced:

```text
Physical Provider
        ↓
domain semantics
        ↓
source engine

```

The physical provider implements storage/index mechanics.

It does not become the semantic authority.

---

#### 34. KG ↔ Indexing during construction

KG or another source engine may provide:

```text
relationship records
entity references
key material
source versions

```

Indexing can construct an index over those records.

For example:

```text
KG
 │
 ├── relationship record
 ├── source reference
 ├── target reference
 └── source version
          ↓
     IndexRequirement
          ↓
     IndexDefinition
          ↓
       Build
          ↓
    Relationship Index

```

Indexing does not decide:

```text
what the relationship means

```

It only constructs an index over the supplied representation.

This preserves the Phase 1 and Phase 2 semantic ownership boundary.

---

#### 35. Phase 3 must not implement query planning

This is one of the most important phase boundaries.

Phase 3 can construct indexes that Phase 4 will query.

It must not implement:

```text
query planner
retrieval planner
ranking
filter planning
hybrid query planning
neighborhood query execution

```

The distinction is:

```text
Phase 3
    → make indexes exist and remain valid

Phase 4
    → decide how a query uses those indexes

```

Therefore:

```text
build
    ≠
query

publication
    ≠
retrieval

```

---

#### 36. Phase 3 must not implement semantic ranking

Even though later retrieval may involve ranking, Phase 3 should not implement domain-specific ranking policy.

For example, Indexing must not decide:

```text
Quran relevance
Hadith authority
Arabic linguistic importance
KG semantic importance
Fiqh priority

```

Those remain outside Indexing's generic construction responsibility.

Phase 3 constructs the retrieval structures.

Phase 4 later determines retrieval behavior.

---

#### 37. Fault boundaries

Phase 3 introduces important failure points:

```text
snapshot failure
build failure
population failure
batch failure
validation failure
version mismatch
publication failure
cancellation
deadline
provider failure

```

Each failure must have a defined lifecycle consequence.

The most important rule is:

```text
candidate failure
    ↓
do not damage active version

```

For example:

```text
active = V4

build V5
    ↓
provider failure

```

results in:

```text
active = V4

```

The failed V5 may be discarded or retained as failed metadata according to the final lifecycle design, but it must not silently become active.

---

#### 38. Concurrency boundary

Phase 3 must explicitly consider concurrent:

```text
update
rebuild
publication

```

operations.

The scope explicitly calls for tests around concurrent update/rebuild boundaries.

The architectural question is not simply:

```text
Can two functions execute simultaneously?

```

It is:

```text
Can concurrent construction and mutation produce an invalid active version?

```

The implementation therefore needs a controlled lifecycle boundary around:

```text
candidate construction
active version
publication

```

The exact synchronization mechanism should be determined by the implementation without changing the logical contract.

---

#### 39. Update/rebuild interaction

A particularly important scenario is:

```text
rebuild V2
+
incremental updates

```

The implementation must have a deterministic policy for this interaction.

The Phase 3 scope requires concurrent update/rebuild boundary testing, but it does not prescribe a specific synchronization strategy.

Therefore the implementation discussion should establish the invariant first:

```text
No concurrent operation may cause an invalid or partially-built version
to become active.

```

The exact mechanism used to achieve this should remain an implementation detail unless the approved architecture explicitly freezes it.

---

#### 40. Batch pressure

Batch ingestion must remain bounded.

Conceptually:

```text
large source dataset
        ↓
bounded batch
        ↓
process
        ↓
next bounded batch

```

rather than:

```text
entire dataset
        ↓
load everything into memory

```

The Phase 3 tests explicitly include bounded batch pressure.

This does not yet require a particular queue, streaming implementation, buffer size, or provider.

Those choices should remain deferred unless required by the implementation.

---

#### 41. Test architecture

The Phase 3 scope defines:

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

This is a substantial expansion from Phase 2 because Phase 3 is no longer only testing data contracts.

It must test state transitions and failure boundaries.

---

#### 42. `tests/build.rs`

This should verify:

```text
create
build
populate
incremental update
batch update
delete
rebuild

```

and the resulting version lifecycle.

Conceptually:

```text
create
   ↓
build
   ↓
candidate
   ↓
validate
   ↓
ready

```

The test should verify that the expected logical state exists at each transition.

---

#### 43. `tests/consistency.rs`

This should verify:

```text
source version compatibility
schema version compatibility
index version compatibility
candidate/active separation
publication eligibility
stale candidate rejection where required

```

The key invariant is:

```text
invalid version relationship
    ↓
candidate cannot become active

```

---

#### 44. `tests/index.rs`

This should preserve the Phase 1 and Phase 2 index model while verifying Phase 3 lifecycle behavior.

Verify:

```text
IndexDefinition
    ↓
IndexVersion
    ↓
candidate
    ↓
active

```

Also verify:

```text
IndexId remains stable
IndexDefinitionId remains distinct
ObjectReference remains source-owned
IndexVersion remains independent

```

Phase 3 must not accidentally collapse these identities.

---

#### 45. `tests/fault_injection.rs`

This test layer should deliberately introduce deterministic failures into:

```text
build
populate
batch
validation
rebuild
publication

```

and verify:

```text
active version remains safe
candidate does not become active accidentally
failed build does not corrupt existing state
failed publication preserves last known-good version

```

The scope explicitly introduces fault-injection coverage at this phase, with later phases expanding it.

---

#### 46. `tests/stress.rs`

Stress tests should focus on:

```text
large batch processing
repeated updates
rebuild pressure
concurrent update/rebuild boundaries
repeated publication

```

The purpose is not to benchmark a specific provider.

The purpose is to expose lifecycle and consistency problems under pressure.

---

#### 47. `tests/integration.rs`

This should verify the complete Phase 3 flow from a downstream perspective.

Conceptually:

```text
IndexRequirement
        ↓
IndexDefinition
        ↓
create
        ↓
source snapshot
        ↓
build
        ↓
populate
        ↓
validate
        ↓
ready
        ↓
publish
        ↓
active IndexVersion

```

The integration test should prove that the modules work together rather than merely testing each module independently.

---

#### 48. `tests/conformance.rs`

The conformance tests should enforce the architectural rules.

They should verify:

```text
build is provider-neutral
updates operate on generic index records
ObjectReference remains reference-only
semantic meaning remains source-owned
active version survives failed rebuild
candidate must be validated before publication
query execution is not required by Phase 3
Core runtime is not reimplemented

```

The conformance layer should protect the architecture, not merely implementation details.

---

#### 49. Required negative tests

Phase 3 needs strong negative tests.

At minimum:

```text
BuildRequest must not require B-tree/HNSW/FAISS/etc.

IndexDefinition must not become a physical provider configuration.

Rebuild must not destroy the active version before validation.

Candidate must not automatically become active.

Failed build must not replace active version.

Failed validation must not publish candidate.

Failed publication must preserve last known-good active version.

Indexing must not hydrate domain objects.

Build must not interpret semantic relationship meaning.

Update must not mutate source-owned domain objects directly.

Batch processing must not imply unbounded memory usage.

Core runtime must not be reimplemented inside build modules.

Query planning must not be required to build an index.

```

---

#### 50. Phase 3 implementation sequence

The implementation order should be:

```text
Step 1
Verify Phase 1 + Phase 2 contracts
        ↓
IndexDefinition / IndexEntry / ObjectReference / IndexVersion

```

```text
Step 2
src/consistency/versioning.rs
        ↓
version compatibility / publication rules

```

```text
Step 3
src/build/builder.rs
        ↓
basic candidate construction

```

```text
Step 4
src/build/update.rs
        ↓
incremental create/update/delete

```

```text
Step 5
src/build/batch.rs
        ↓
bounded batch processing

```

```text
Step 6
src/build/rebuild.rs
        ↓
isolated candidate rebuild

```

```text
Step 7
src/build/publication.rs
        ↓
ready → active transition

```

```text
Step 8
index/version.rs
        ↓
active/candidate lifecycle integration

```

```text
Step 9
module exports
        ↓
src/build/mod.rs
src/consistency/mod.rs
src/index/mod.rs
src/lib.rs

```

```text
Step 10
source-local unit tests
        ↓
construction/update/version behavior

```

```text
Step 11
tests/build.rs
tests/consistency.rs
tests/index.rs
        ↓
normal lifecycle verification

```

```text
Step 12
tests/fault_injection.rs
tests/stress.rs
        ↓
failure/concurrency/pressure verification

```

```text
Step 13
tests/integration.rs
tests/conformance.rs
        ↓
full Phase 3 architectural verification

```

The ordering keeps version consistency available before publication and keeps basic construction available before rebuild orchestration.

---

#### 51. What Phase 3 must NOT do

Phase 3 must not implement:

```text
query planner
query retrieval
query ranking
filtered query execution
hybrid retrieval
neighborhood retrieval

```

Those belong to Phase 4.

It must also not implement:

```text
domain semantic interpretation
KG ontology
semantic predicate ownership
domain inference
domain-specific ranking
domain-object hydration

```

It must not introduce:

```text
global scheduler
global workflow engine
global execution engine

```

Core and Control Plane already provide the shared infrastructure boundary.

It must not prematurely freeze:

```text
database provider
physical index algorithm
storage technology
partitioning strategy
sharding strategy
serialization technology

```

unless an explicitly approved architectural decision requires it.

---

#### 52. Phase 3 architectural invariants

The following must remain true after Phase 3:

```text
IndexDefinition
    ≠
Physical Index Configuration

```

```text
IndexVersion
    ≠
AttemptId

```

```text
IndexVersion
    ≠
Source Version

```

```text
IndexVersion
    ≠
Schema Version

```

```text
Candidate Version
    ≠
Active Version

```

```text
Build
    ≠
Query

```

```text
Rebuild
    ≠
Query Retrieval

```

```text
Index Update
    ≠
Source Domain Mutation

```

```text
Indexing
    ≠
Domain Semantics

```

```text
Indexing
    ≠
Control Plane

```

```text
Core Runtime
    ≠
Indexing Runtime Reimplementation

```

```text
Logical Index
    ≠
Physical Provider

```

Most importantly:

```text
Failed Candidate
    ≠
Failed Active Index

```

and:

```text
Active Version
    remains valid
    until a validated candidate is safely published

```

---

#### 53. Phase 3 complete lifecycle

The complete intended lifecycle is:

```text
                  SOURCE ENGINE
                       │
                       │ source state
                       ▼
                 Source Snapshot
                       │
                       ▼
                Index Definition
                       │
                       ▼
                Candidate Version
                       │
             ┌─────────┴─────────┐
             │                   │
             ▼                   ▼
        populate              updates
             │                   │
             └─────────┬─────────┘
                       ▼
                    Validate
                       │
              ┌────────┴────────┐
              │                 │
           failure            success
              │                 │
              ▼                 ▼
       candidate rejected      READY
              │                 │
              │                 ▼
              │              Publish
              │                 │
              │                 ▼
              │             ACTIVE
              │                 │
              └───────┬─────────┘
                      ▼
             previous active remains
             protected until safe
             replacement

```

This is the heart of Phase 3.

---

#### 54. Phase 3 vs Phase 4 boundary

The clean boundary is:

```text
PHASE 3
────────────────────────────
How do we construct and maintain
valid index versions?

        ↓

PHASE 4
────────────────────────────
How do we select and retrieve
from those index versions?

```

Phase 3:

```text
create
build
populate
update
delete
batch
rebuild
validate
publish
version consistency

```

Phase 4:

```text
query
planning
index selection
retrieval
filtering
ranking
freshness
stale-index behavior
reference-only result retrieval

```

The Phase 3 implementation must stop at the publication boundary.

---

#### Completion Criteria

Phase 3 is complete when:

#### Verification Checklist

- [ ] Index creation mechanics exist.

- [ ] Index construction can create a candidate version.

- [ ] Candidate construction consumes the Phase 2 logical contracts.

- [ ] Index population works using generic IndexEntry / reference-oriented data.

- [ ] Incremental create/update/delete operations work within the declared
  consistency model.

- [ ] Bounded batch operations work.

- [ ] Major rebuilds construct a separate candidate version.

- [ ] The active version remains usable while a rebuild is occurring.

- [ ] Candidate versions can be validated before publication.

- [ ] Invalid candidates cannot become active.

- [ ] Version compatibility rules are enforced.

- [ ] Source version and schema version remain distinct from IndexVersion.

- [ ] Publication is a controlled lifecycle transition.

- [ ] A successfully published candidate becomes the active version.

- [ ] A failed rebuild does not corrupt the active version.

- [ ] A failed validation does not replace the active version.

- [ ] A failed publication preserves the last known-good active version.

- [ ] Concurrent update/rebuild boundaries are controlled.

- [ ] Cancellation and deadline behavior use the existing Core runtime mechanisms.

- [ ] No local replacement for Core cancellation/deadline/runtime behavior is
  introduced.

- [ ] No global scheduler or second Control Plane is introduced.

- [ ] No physical index technology leaks into the logical Indexing contract.

- [ ] No domain semantics are introduced into the build/update system.

- [ ] No source-owned domain objects are hydrated or mutated by Indexing.

- [ ] Query planning and retrieval remain deferred to Phase 4.

- [ ] Unit tests cover build/update/versioning behavior.

- [ ] Integration tests cover complete construction and publication flows.

- [ ] Fault-injection tests cover build/rebuild/publication failures.

- [ ] Stress tests cover bounded batches and concurrent lifecycle pressure.

- [ ] Conformance tests protect the Phase 1 and Phase 2 architectural boundaries.

- [ ] Previously verified Phase 1 and Phase 2 behavior remains intact.

---

#### Final Phase 3 Mental Model

After Phase 3, the Indexing Engine should be able to answer:

```text
How do I create an index?
        ↓
Create logical index lifecycle

How do I construct it?
        ↓
Build candidate version

How do I put indexable records into it?
        ↓
Populate / batch

How do I modify it?
        ↓
Incremental update / delete

How do I rebuild it?
        ↓
Construct isolated candidate version

How do I know the candidate is safe?
        ↓
Validate

How does a candidate become usable?
        ↓
Publish

Which version is currently active?
        ↓
Active IndexVersion

What happens if construction fails?
        ↓
Active version remains protected

```

The final Phase 3 architecture is therefore:

```text
                    SOURCE ENGINE
                         │
                         │
                  source snapshot
                         │
                         ▼
                  IndexDefinition
                         │
                         ▼
                  Build / Update
                         │
              ┌──────────┴──────────┐
              │                     │
              ▼                     ▼
         Batch Updates         Rebuild
              │                     │
              └──────────┬──────────┘
                         ▼
                  Candidate Version
                         │
                         ▼
                     Validate
                         │
                ┌────────┴────────┐
                │                 │
             failure            success
                │                 │
                ▼                 ▼
        preserve active         READY
                                  │
                                  ▼
                               Publish
                                  │
                                  ▼
                            Active Version
                                  │
                                  ▼
                            Phase 4 Query

```

And the ownership remains:

```text
Source / Domain Engine
    → owns source objects and their meaning

Core
    → owns runtime, capability dispatch, context, lifecycle,
      cancellation, deadline, and universal execution infrastructure

Control Plane
    → owns global coordination

Indexing
    → owns construction, update, rebuild, validation,
      version lifecycle, and publication

Physical Provider
    → owns physical index construction and persistence

```

The key architectural rule for the entire phase is:

```text
BUILD A NEW VERSION
        ↓
VALIDATE IT
        ↓
PUBLISH IT
        ↓
ONLY THEN REPLACE THE ACTIVE VERSION

```

That is what makes Phase 3 fundamentally different from Phase 2. Phase 2 defined **what the logical index is**; Phase 3 defines **how valid index state is constructed and safely promoted without damaging the currently active state**. This follows the Phase 3 scope directly.

---

### Phase 4: Consistency, Synchronization, Query & Retrieval

#### Status

**Not Started**

##### 1. Phase 4 Goal

Phase 4 implements the generic query/retrieval model and consistency guarantees for source-owned indexed objects and records.

Phase 2 defines the logical index and query-related contracts.

Phase 3 makes valid index versions exist, maintainable, and safely publishable.

Phase 4 makes those published index versions usable for generic retrieval while preserving the ownership boundaries between the source engine, Core, Indexing, and physical providers.

The central question of Phase 4 is:

```text
How does a source-owned query use a compatible published index
and retrieve generic references with defined consistency behavior?
```

Phase 4 is therefore the query-side counterpart to Phase 3.

```text
Phase 3
    ↓
construct, validate, and publish valid index versions

Phase 4
    ↓
select and retrieve from those valid index versions
```

---

#### 2. Architectural Position

The Indexing Engine remains inside the broader Infrastructure Engine architecture.

The intended execution path is:

```text
Nizaam / Control Plane
        ↓
Universal Engine Contract
        ↓
Core Engine Runtime
        ↓
Core Capability Dispatch
        ↓
Indexing Capability
        ↓
Indexing-local Query Planner
        ↓
Indexing Query Workflow
        ↓
Provider / Backend Boundary
        ↓
Physical Retrieval
        ↓
Generic Query Result
```

Core remains responsible for universal runtime infrastructure.

Indexing remains responsible for indexing-specific construction, versioning, query planning, consistency decisions, and retrieval orchestration.

The physical provider remains responsible for physical storage and retrieval mechanics.

The source or domain engine remains responsible for the meaning of the retrieved data.

The provider abstraction is a shared top-level Indexing module rather than a
provider implementation duplicated inside individual workflows. Query planning
and retrieval may use this shared provider abstraction to access compatible
provider capabilities.

Generic ranking mechanisms are owned by the provider abstraction. They operate
on generic retrieval/index information and remain independent of domain meaning.
Domain-specific ranking policy and semantic relevance remain source-owned.

The ownership boundary is:

```text
Source / Domain Engine
    → owns source objects and their meaning

Core
    → owns runtime, capability dispatch, context, lifecycle,
      cancellation, deadline, and universal execution infrastructure

Control Plane
    → owns global coordination

Indexing
    → owns logical index selection, query planning,
      consistency behavior, and retrieval orchestration

Physical Provider
    → owns physical index/storage retrieval mechanics
```

---

#### 3. Phase 4 Scope

The Phase 4 scope requires support for:

```text
exact lookup
inverted/text lookup
structured record lookup
neighborhood lookup
similarity lookup
filtered lookup
hybrid retrieval
```

It also requires:

```text
fresh/current index
version-aware query
stale-index behavior
consistency requirements
query capability negotiation
reference-only result retrieval
```

Relationship records may be retrieved through generic structured or neighborhood queries.

However, Indexing must not interpret:

```text
relationship direction
predicate meaning
cardinality
domain semantics
```

Those remain owned by the source/domain engine.

---

#### 4. Phase 3 → Phase 4 Boundary

The most important phase boundary is:

```text
PHASE 3
────────────────────────────────
How do we construct and maintain
valid index versions?
────────────────────────────────
            ↓
PHASE 4
────────────────────────────────
How do we select and retrieve
from those index versions?
────────────────────────────────
```

Phase 3 owns:

```text
create
build
populate
update
delete
batch
rebuild
validate
publish
version lifecycle
```

Phase 4 owns:

```text
query
query validation
consistency requirements
index selection
query planning
retrieval
filtering
similarity retrieval
neighborhood retrieval
hybrid retrieval
stale-index behavior
reference-only results
```

Therefore:

```text
build ≠ query

publication ≠ retrieval
```

Phase 4 consumes the valid active versions produced by Phase 3.

It must not replace or duplicate the Phase 3 version lifecycle.

---

#### 5. Core Runtime Boundary

Phase 4 must reuse the Core runtime and capability infrastructure.

The Indexing Engine should not create a second implementation of:

```text
EngineRuntime
CapabilityRegistry
CapabilityHandler
OperationContext
CancellationToken
Deadline
SecurityContext
lifecycle
request admission
capability dispatch
```

The conceptual execution path is:

```text
Core Capability Dispatch
        ↓
Indexing Query Handler
        ↓
QueryRequest
        ↓
Consistency Policy
        ↓
Query Planner
        ↓
Retrieval
        ↓
QueryResult
        ↓
Core CapabilityOutcome
        ↓
UniversalResponse
```

The Core capability boundary provides the universal execution mechanism.

Indexing provides the typed query semantics on top of that boundary.

The physical query representation must not leak through the universal Core contract.

---

#### 6. Generic Query Model

The public query model must remain logical and provider-neutral.

A caller should express:

```text
what kind of lookup is required
what source-owned key material is relevant
what namespace/index family is relevant
what consistency is required
what result representation is required
```

A caller should not express:

```text
use HNSW
use IVF
use Lucene
use GIN
use a specific database execution plan
use a provider-specific physical operator
```

The public contract therefore looks conceptually like:

```text
Source-owned requirements
        ↓
QueryRequest
        ↓
Indexing-local planner
        ↓
compatible index definition
        ↓
retrieval plan
        ↓
provider
```

The physical algorithm remains an implementation detail behind the provider boundary.

---

#### 7. `query/request.rs`

`query/request.rs` owns the logical query requirements.

It should represent the query in terms of Indexing's generic contract rather than a physical database query.

The request may contain source-declared information such as:

```text
object category
namespace
language field
domain-owned context field
key components
query mode
query parameters
consistency requirements
version requirements
result/reference requirements
```

The exact semantic interpretation of those fields remains outside Indexing.

For example:

```text
language = Arabic
```

may help identify compatible indexed key material.

Indexing does not decide what Arabic means for the source domain.

Similarly:

```text
relationship category = X
```

may be part of source-owned key material.

Indexing may use it for index selection, but it must not interpret the relationship semantically.

---

#### 8. Exact Lookup

Exact lookup retrieves entries matching exact logical key material.

Conceptually:

```text
QueryRequest
    ↓
exact key requirement
    ↓
compatible IndexDefinition
    ↓
active IndexVersion
    ↓
provider retrieval
    ↓
ObjectReference
```

The exact lookup contract must remain generic.

It must not become tied to:

```text
HashMap
B-tree
database primary key
specific storage engine
```

Those are implementation choices.

The logical contract describes the lookup requirement.

---

#### 9. Inverted / Text Lookup

Text-oriented retrieval may use indexed text or inverted representations.

The logical query can express requirements such as:

```text
text field
token/term requirement
language/key material
matching mode
consistency requirement
```

The physical implementation may use an inverted index or another provider strategy.

However:

```text
logical text lookup
        ≠
specific physical text engine
```

The caller must not be required to know the provider's physical technology.

---

#### 10. Structured Record Lookup

Structured lookup allows retrieval using indexed record fields or key material.

Conceptually:

```text
structured requirements
        ↓
compatible logical index
        ↓
retrieval
        ↓
ObjectReference results
```

The structured fields may be source-owned.

Indexing can use those fields for selecting an index and executing a generic lookup.

Indexing must not assign domain meaning to them.

---

#### 11. Neighborhood Lookup

Neighborhood retrieval supports retrieval around an indexed reference or structured key.

Conceptually:

```text
starting reference
        ↓
neighborhood requirement
        ↓
compatible index
        ↓
retrieval
        ↓
related indexed references
```

This does not make Indexing a knowledge-graph semantic engine.

Indexing may retrieve records that represent relationships.

It does not decide:

```text
what the relationship means
whether a relationship is semantically valid
what a predicate means
what direction means in the domain
what cardinality means
```

The source engine remains the semantic authority.

---

#### 12. Similarity Lookup

Similarity lookup allows a caller to express a generic similarity retrieval requirement.

The logical contract may describe:

```text
query representation
similarity requirement
limit/bounds
filter requirements
consistency requirements
```

The physical provider may implement similarity retrieval using an appropriate physical strategy.

The logical contract must not require the caller to specify:

```text
HNSW
IVF
PQ
specific vector database
specific ANN algorithm
```

The provider and Indexing-local planning layer may select an appropriate compatible implementation internally.

---

#### 13. Filtered Lookup

Filtered lookup combines retrieval with source-declared constraints.

Conceptually:

```text
query
  +
filter requirements
        ↓
planner
        ↓
compatible index/provider capability
        ↓
retrieval
        ↓
reference results
```

Filters must remain generic and source-owned.

Indexing may apply them as part of retrieval planning or provider execution.

Indexing must not transform filtering into domain-specific business logic.

---

#### 14. Hybrid Retrieval

Hybrid retrieval combines multiple generic retrieval requirements.

For example:

```text
text requirement
        +
structured filter
        +
similarity requirement
```

The important architectural rule is that hybrid retrieval remains a logical query contract.

The planner decides how the available compatible indexes and providers can satisfy the request.

The caller does not need to know the physical execution strategy.

Conceptually:

```text
Logical Query
      ↓
Planner
      ↓
┌───────────────┬───────────────┐
│ text index    │ similarity    │
│               │ index         │
└───────┬───────┴───────┬───────┘
        ↓               ↓
          provider execution
                 ↓
          generic result
```

The exact physical combination remains internal.

---

#### 15. Logical Query Planning

`query/planner.rs` is one of the most important Phase 4 modules.

Its responsibility is:

```text
select compatible logical index definitions
select compatible namespaces
consider index versions
consider consistency requirements
construct retrieval plans
coordinate provider capabilities
```

It must not become a domain semantic planner.

It also must not expose physical algorithms as part of the logical query contract.

The planner may internally coordinate physical provider selection.

The distinction is:

```text
PUBLIC CONTRACT
────────────────────────────
logical query requirements

            ↓

INDEXING-LOCAL PLANNER
────────────────────────────
index selection
provider capability matching
retrieval planning

            ↓

PROVIDER
────────────────────────────
physical algorithm / storage
```

This reconciles the Infrastructure Engine architecture with the Phase 4 requirement that physical algorithms remain hidden.

---

#### 16. Consistency Must Be Considered Before Retrieval

Consistency is not an afterthought.

The query flow should conceptually be:

```text
QueryRequest
      ↓
Consistency Requirements
      ↓
Acceptable Index Versions
      ↓
Index Selection
      ↓
Retrieval Plan
      ↓
Provider
      ↓
Result
```

The planner must not blindly select an index and only afterward discover that the selected version violates the caller's consistency requirements.

For example:

```text
caller requires current/fresh data
        ↓
planner
        ↓
stale index
```

The stale index must not silently satisfy a request that requires freshness.

The final behavior must be defined by the consistency policy.

---

#### 17. `consistency/policy.rs`

`consistency/policy.rs` defines the consistency requirements and stale/fresh behavior.

It should answer questions such as:

```text
What index state is acceptable?

Can a stale index satisfy this query?

Is a particular source/index version relationship required?

Which published version may be queried?

What happens when no compatible version exists?
```

The consistency policy must remain explicit.

It must not be silently inferred from provider behavior.

The important distinction is:

```text
Indexing consistency policy
        ↓
determines whether index state is acceptable

Provider
        ↓
retrieves from the selected index
```

---

#### 18. Fresh / Current Index Queries

A fresh/current query requires the query to use an acceptable current published index state.

The normal query path should therefore use:

```text
active published version
```

rather than an unpublished candidate.

During a rebuild:

```text
Active V1
    +
Candidate V2
```

normal queries must continue to observe:

```text
Active V1
```

until V2 has passed validation and has been published.

This preserves the Phase 3 publication invariant.

---

#### 19. Version-Aware Queries

Some callers may need to query against a specific compatible index version.

Phase 4 therefore supports version-aware query behavior.

Conceptually:

```text
QueryRequest
    ↓
requested version requirement
    ↓
consistency validation
    ↓
compatible IndexVersion
    ↓
retrieval
```

The version used for query execution must be a valid queryable version according to the lifecycle rules established by Phase 3.

Version-aware querying must not allow callers to bypass publication or validation rules.

---

#### 20. Stale-Index Behavior

Stale indexes must have explicit behavior.

The system should distinguish between:

```text
fresh/current
acceptable stale
unacceptable stale
unavailable
```

The exact policy depends on the declared consistency contract.

What must not happen is:

```text
stale index
    ↓
silently returned as current
```

If the consistency requirement cannot be satisfied, the query should produce a defined consistency/capability result rather than silently weakening the request.

---

#### 21. `consistency/synchronization.rs`

`consistency/synchronization.rs` owns the synchronization relationship between source state and indexed state.

Its responsibility is not to become a second update engine.

It should establish the query-time relationship between:

```text
source version
        ↓
indexed version
        ↓
query consistency requirement
```

The distinction is:

```text
Phase 3 update/rebuild system
    → changes index state

Phase 4 synchronization logic
    → determines whether index state is sufficiently
      synchronized/compatible for the query
```

This keeps construction and retrieval responsibilities separate.

---

#### 22. Query Retrieval Boundary

`query/retrieval.rs` is the execution/retrieval boundary.

It should:

```text
receive a validated retrieval plan
        ↓
invoke the appropriate provider capability
        ↓
obtain indexed records/references
        ↓
convert them into generic Indexing results
```

It should not contain the primary query-planning logic.

The separation is:

```text
planner
    → decides what should be retrieved and from where

retrieval
    → executes the resulting retrieval plan
```

---

#### 23. Provider Boundary

The provider owns physical retrieval mechanics.

Indexing owns:

```text
logical query model
index selection
consistency policy
retrieval orchestration
reference-oriented result construction
```

The provider owns:

```text
physical storage
physical indexes
physical query execution
physical filtering
physical similarity implementation
physical persistence
```

The direction must remain:

```text
Indexing
    ↓
Provider
    ↓
physical retrieval
```

Not:

```text
Provider
    ↓
domain semantics
    ↓
source engine
```

The physical provider is not the semantic authority.

---

#### 24. Generic Ranking Boundary

Phase 4 may require ranking as part of retrieval, but the ranking mechanism
must remain generic and provider-oriented.

```text
Logical Query
    ↓
Indexing Query Planner
    ↓
Provider Abstraction
    ↓
Generic Ranking
    ↓
Reference-oriented Result
```

The provider abstraction owns the reusable generic ranking mechanism so that
build/query/retrieval workflows do not each implement their own ranking logic.

Generic ranking may use provider-neutral retrieval information such as scores,
ordering keys, or comparable result metadata. It must not define:

```text
domain relevance
semantic authority
relationship meaning
domain-specific ranking policy
```

Those remain outside Indexing.

---

#### 25. Physical Algorithm Leakage

One of the most important negative architectural tests is preventing physical algorithm leakage.

The public logical contract must not contain fields such as:

```text
algorithm = HNSW
algorithm = IVF
algorithm = Lucene
algorithm = GIN
```

as requirements imposed by callers.

Instead:

```text
logical requirement
        ↓
planner
        ↓
compatible provider capability
        ↓
physical implementation
```

This allows providers to evolve without changing the logical Indexing contract.

---

#### 25. Reference-Only Result Retrieval

Indexing should return references rather than hydrate source-owned domain objects.

A generic result may contain:

```text
ObjectReference
index identity
index version
query metadata
consistency metadata
optional ranking/retrieval metadata
```

The important boundary is:

```text
Indexing
    → returns what matched

Source engine
    → decides what the matched object means
      and may retrieve/hydrate the source object
```

Indexing must not become a domain object store.

---

#### 26. Result Flow

The complete result path is:

```text
Physical Provider
        ↓
Indexing Retrieval Layer
        ↓
Reference / logical result conversion
        ↓
QueryResult
        ↓
CapabilityOutcome
        ↓
UniversalResponse
```

The universal response remains part of the Core execution boundary.

The typed `QueryResult` remains owned by Indexing.

---

#### 27. Relationship Records

Relationship records can be indexed and retrieved generically.

For example:

```text
Source Engine / KG
        ↓
relationship record
        ↓
IndexRequirement
        ↓
IndexDefinition
        ↓
IndexVersion
        ↓
Query
        ↓
ObjectReference / relationship reference
```

Indexing may retrieve the relationship record.

It does not decide:

```text
what the relationship means
whether it is authoritative
whether it is valid in the domain
what a predicate means
how a relationship should be interpreted
```

Those decisions remain outside Indexing.

This preserves the source-owned semantic boundary established in the earlier phases.

---

#### 28. Query Capability Negotiation

Before executing a query, Indexing must determine whether the request can actually be satisfied.

Capability negotiation may consider:

```text
query type
index definition
index namespace
index version
provider capability
consistency requirement
key/query compatibility
```

The result should distinguish between:

```text
supported and executable
unsupported
temporarily unavailable
incompatible with requested consistency
```

Indexing must not silently convert an unsupported query into an unrelated operation merely to return something.

---

#### 29. Query During Rebuild

A rebuild must remain isolated from normal query visibility.

Example:

```text
Active V4
     +
Candidate V5
```

While V5 is being built:

```text
normal query
     ↓
V4
```

not:

```text
normal query
     ↓
partial V5
```

If V5 fails:

```text
Active V4
```

remains queryable.

If V5 validates and is published:

```text
Active V5
```

becomes queryable according to the controlled publication transition.

This is a direct consumer-side consequence of the Phase 3 lifecycle.

---

#### 30. Query During Publication

Publication must not expose partial state.

The observable states should be conceptually:

```text
Old valid active
        OR
New valid active
```

but never:

```text
partially published index
invalid candidate
half-updated active version
```

The query path therefore depends on the atomic/controlled publication guarantee established by Phase 3.

Phase 4 consumes that guarantee rather than reimplementing it.

---

#### 31. Query / Update Concurrency

Queries may execute while updates or rebuilds are occurring.

The important invariant is:

```text
query may observe a valid queryable version,
but must never observe invalid index state.
```

The implementation must therefore preserve:

```text
active version validity
candidate isolation
controlled publication
consistent version visibility
```

The exact synchronization mechanism is an implementation detail.

The logical contract should not depend on a particular lock or concurrency primitive.

---

#### 32. Slow or Blocked Retrieval

Retrieval may involve slow providers or blocked operations.

Indexing must use the existing Core execution mechanisms for:

```text
OperationContext
CancellationToken
Deadline
```

The Indexing Engine must not create a separate cancellation/deadline system.

Conceptually:

```text
Core deadline/cancellation
        ↓
Indexing query workflow
        ↓
provider retrieval
        ↓
cancellation/deadline propagation
```

This keeps runtime behavior consistent across engines.

---

#### 33. Query Module Structure

The planned structure is:

```text
src/
├── query/
│   ├── mod.rs
│   ├── request.rs
│   ├── result.rs
│   ├── planner.rs
│   └── retrieval.rs
│
└── consistency/
    ├── mod.rs
    ├── policy.rs
    └── synchronization.rs
```

Responsibilities:

##### `query/mod.rs`

Owns module organization and public query exports.

###### `query/request.rs`

Defines logical query requirements.

###### `query/result.rs`

Defines generic reference-oriented query results.

###### `query/planner.rs`

Performs Indexing-local query planning and index selection.

###### `query/retrieval.rs`

Executes retrieval through the provider boundary.

###### `consistency/mod.rs`

Owns consistency module organization.

###### `consistency/policy.rs`

Defines consistency requirements and stale/fresh behavior.

###### `consistency/synchronization.rs`

Defines source/index synchronization checks relevant to retrieval.

---

#### 34. Phase 4 Workflow

The complete workflow should be understood as:

```text
                         CONTROL PLANE
                              │
                              ▼
                     INDEXING CAPABILITY
                              │
                              ▼
                       CORE RUNTIME
                              │
                              ▼
                       QUERY REQUEST
                              │
                              ▼
                    CONSISTENCY POLICY
                              │
                              ▼
                       QUERY PLANNER
                              │
                 ┌────────────┼────────────┐
                 │            │            │
                 ▼            ▼            ▼
             exact/text   structured   similarity
                 │            │            │
                 └────────────┼────────────┘
                              ▼
                    RETRIEVAL WORKFLOW
                              │
                              ▼
                      PROVIDER BOUNDARY
                              │
                              ▼
                     PHYSICAL RETRIEVAL
                              │
                              ▼
                  REFERENCE-ONLY RESULT
                              │
                              ▼
                         QueryResult
                              │
                              ▼
                    UniversalResponse
```

---

#### 35. Phase 4 and the Infrastructure Engine Plan

The broader Infrastructure Engine architecture requires engines to own their local planning and execution workflows.

Phase 4 follows that architecture.

The Indexing Engine owns:

```text
Indexing-local query planning
Index selection
Consistency decisions
Retrieval orchestration
Provider coordination
```

Core owns:

```text
runtime
capability dispatch
execution context
lifecycle
cancellation
deadline
universal result transport
```

The Control Plane owns:

```text
global coordination
engine registration
system-level orchestration
```

The physical provider owns:

```text
physical index/storage mechanics
```

The source engine owns:

```text
source objects
domain semantics
semantic interpretation
```

This produces the following clean boundary:

```text
Universal Infrastructure
        ↓
Core
        ↓
Indexing Engine
        ↓
Provider
```

while semantic meaning remains:

```text
Source Engine
```

---

#### 36. Phase 4 Must Not Become a Semantic Query Engine

Indexing must not implement domain-specific semantic reasoning.

For example, Indexing must not decide:

```text
Quran relevance
Hadith authority
Fiqh priority
Arabic linguistic importance
KG semantic importance
```

It can retrieve records representing these concepts.

It cannot decide what they mean.

The source/domain engine remains the semantic authority.

---

#### 37. Phase 4 Must Not Become a Second Control Plane

Indexing may perform local query planning.

It must not become responsible for global engine coordination.

It should not introduce:

```text
global scheduler
global orchestration
engine lifecycle coordination
cross-engine ownership
```

Those remain outside the Indexing query subsystem.

---

#### 38. Phase 4 Must Not Replace Phase 3 Versioning

Phase 4 can consume:

```text
IndexVersion
active version
published version
source/index version relationship
```

It must not create an alternative publication mechanism.

The lifecycle remains:

```text
build
    ↓
validate
    ↓
ready
    ↓
publish
    ↓
active
    ↓
query
```

Querying begins only from a valid queryable version.

---

#### 39. Testing Strategy

The planned Phase 4 test structure is:

```text
tests/
├── query.rs
├── consistency.rs
├── integration.rs
├── conformance.rs
├── fault_injection.rs
└── stress.rs
```

##### `tests/query.rs`

Test:

```text
exact lookup
inverted/text lookup
structured lookup
neighborhood lookup
similarity lookup
filtered lookup
hybrid retrieval
```

##### `tests/consistency.rs`

Test:

```text
fresh/current queries
version-aware queries
stale-index behavior
consistency requirements
source/index synchronization
```

##### `tests/integration.rs`

Test complete workflows:

```text
request
    ↓
consistency policy
    ↓
planner
    ↓
provider
    ↓
reference result
```

##### `tests/conformance.rs`

Protect architectural boundaries:

```text
no physical algorithm leakage
no semantic interpretation
no domain object hydration
Core runtime reuse
Core cancellation/deadline reuse
Phase 1–3 compatibility
```

##### `tests/fault_injection.rs`

Test:

```text
provider failure
retrieval failure
unsupported capability
consistency failure
version mismatch
cancellation
deadline
```

##### `tests/stress.rs`

Test:

```text
concurrent queries
query during update
query during rebuild
query during publication
slow retrieval
bounded concurrent pressure
```

---

#### 40. Important Negative Tests

Phase 4 should explicitly protect against architectural regression.

##### Physical algorithm leakage

A query must not require:

```text
HNSW
IVF
Lucene
GIN
```

as part of the logical contract.

###### Semantic interpretation leakage

Indexing must not interpret:

```text
relationship meaning
domain relevance
domain authority
domain-specific ranking semantics
```

###### Domain object hydration

Indexing must not silently return fully hydrated source-domain objects.

###### Candidate visibility

Queries must not observe unpublished candidate versions.

###### Stale-index misrepresentation

A stale result must not be presented as current when the consistency contract requires freshness.

###### Core duplication

Indexing must not introduce its own replacement for Core runtime/cancellation/deadline infrastructure.

---

#### 41. Provider Failure Behavior

Provider failures must remain visible through the defined error/result boundary.

For example:

```text
Query
  ↓
Planner
  ↓
Provider
  ↓
provider failure
```

must not result in:

```text
empty result
```

unless the logical contract explicitly defines that behavior.

A provider failure is not equivalent to:

```text
no matching records
```

The distinction must remain observable.

---

#### 42. Unsupported Query Behavior

If no compatible index/provider capability exists:

```text
QueryRequest
      ↓
Planner
      ↓
No compatible capability
```

the system should return a defined unsupported/incompatible outcome.

It must not silently:

```text
change query type
ignore required filters
ignore consistency requirements
select an incompatible index
```

This protects the logical contract.

---

#### 43. Query Result Invariants

Every successful query result should obey the generic Indexing result contract.

Important invariants include:

```text
result references are source-owned references
result version is identifiable
result consistency state is defined
result does not expose physical provider details
result does not contain domain semantic interpretation
```

The result should provide enough metadata for the caller to understand:

```text
what was retrieved
from which logical index/version
under which consistency conditions
```

without exposing physical implementation details.

---

#### 44. Query and Source Ownership

The source engine supplies source-owned query requirements.

For example:

```text
object category
namespace
language field
domain context
key components
```

Indexing can use those requirements to select compatible indexes.

The source engine remains responsible for interpreting the meaning.

Therefore:

```text
Source
    → declares what it needs

Indexing
    → determines how the available indexes can satisfy it

Provider
    → performs physical retrieval

Source
    → interprets the result
```

This is the core semantic ownership model for Phase 4.

---

#### 45. Phase 4 Implementation Sequence

A safe implementation sequence is:

```text
1. Define query request types.

2. Define generic query result types.

3. Define consistency requirement/policy types.

4. Define source/index synchronization checks.

5. Define query capability requirements.

6. Implement logical index selection.

7. Implement query planning.

8. Implement retrieval boundary.

9. Integrate provider capability execution.

10. Implement exact lookup.

11. Implement text/inverted lookup.

12. Implement structured lookup.

13. Implement neighborhood lookup.

14. Implement similarity lookup.

15. Implement filtering.

16. Implement hybrid retrieval.

17. Implement fresh/current behavior.

18. Implement version-aware behavior.

19. Implement stale-index behavior.

20. Implement reference-only result construction.

21. Integrate Core cancellation/deadline behavior.

22. Add integration and conformance tests.

23. Add fault-injection and stress tests.

24. Run Phase 1–3 regression tests.
```

The implementation should preserve the logical contracts while the internal provider strategy remains replaceable.

---

#### 46. Phase 4 Architectural Invariants

The following invariants should hold after Phase 4:

- [ ] Queries are expressed through generic logical requirements.

- [ ] Exact lookup is supported.

- [ ] Inverted/text lookup is supported.

- [ ] Structured record lookup is supported.

- [ ] Neighborhood lookup is supported.

- [ ] Similarity lookup is supported.

- [ ] Filtered lookup is supported.

- [ ] Hybrid retrieval is supported.

- [ ] Index selection is performed locally by Indexing.

- [ ] Query planning remains provider-neutral at the public contract boundary.

- [ ] Physical provider selection may occur internally behind the planner/provider boundary.

- [ ] Physical algorithms are not exposed through the logical query contract.

- [ ] Fresh/current query behavior is defined.

- [ ] Version-aware query behavior is defined.

- [ ] Stale-index behavior is defined.

- [ ] Consistency requirements are enforced.

- [ ] Source/index synchronization is explicitly considered.

- [ ] Query capability negotiation is supported.

- [ ] Unsupported queries do not silently become different queries.

- [ ] Results are reference-oriented.

- [ ] Source-owned domain objects are not hydrated by Indexing.

- [ ] Relationship records remain semantically owned by the source engine.

- [ ] Indexing does not interpret relationship meaning.

- [ ] Core runtime and capability dispatch are reused.

- [ ] Core cancellation and deadline mechanisms are reused.

- [ ] Provider failures remain distinguishable from empty results.

- [ ] Unpublished candidate versions are not exposed to normal queries.

- [ ] Queries observe only valid queryable index state.

- [ ] Phase 3 publication guarantees remain intact.

- [ ] No second Control Plane is introduced.

- [ ] No semantic query engine is introduced.

- [ ] No physical storage technology becomes part of the logical contract.

- [ ] Phase 1–3 behavior remains intact.

---

#### Completion Criteria

Phase 4 is complete when:

#### Verification Checklist

- [ ] Logical query requests can be expressed against source-owned
  index requirements.

- [ ] Compatible IndexDefinitions can be selected.

- [ ] Compatible IndexVersions can be selected according to
  declared consistency requirements.

- [ ] Exact lookup works.

- [ ] Inverted/text lookup works.

- [ ] Structured record lookup works.

- [ ] Neighborhood lookup works.

- [ ] Similarity lookup works.

- [ ] Filtered lookup works.

- [ ] Hybrid retrieval works.

- [ ] Fresh/current queries behave according to the declared
  consistency model.

- [ ] Version-aware queries work.

- [ ] Stale-index behavior is explicit and deterministic.

- [ ] Consistency requirements are enforced.

- [ ] Source/index synchronization requirements are enforced.

- [ ] Query capability negotiation works.

- [ ] Unsupported or incompatible queries return defined outcomes.

- [ ] Reference-only results are returned.

- [ ] Source-domain objects are not hydrated by Indexing.

- [ ] Relationship meaning remains outside Indexing.

- [ ] Physical algorithms do not leak into the logical contract.

- [ ] Provider failures are handled through the defined error boundary.

- [ ] Query cancellation uses Core cancellation infrastructure.

- [ ] Query deadlines use Core deadline infrastructure.

- [ ] Queries cannot observe invalid or unpublished candidate state.

- [ ] Queries remain safe during rebuild and publication.

- [ ] Integration tests cover end-to-end query/retrieval workflows.

- [ ] Conformance tests protect architecture boundaries.

- [ ] Fault-injection tests cover provider and consistency failures.

- [ ] Stress tests cover concurrent query/update/rebuild conditions.

- [ ] Previously verified Phase 1, Phase 2, and Phase 3 behavior remains intact.

---

#### 48. Final Phase 4 Mental Model

The complete Phase 4 architecture can be summarized as:

```text
                     SOURCE ENGINE
                           │
                           │
                           │ source-owned requirements
                           ▼
                      QueryRequest
                           │
                           ▼
                  Consistency Policy
                           │
                           ▼
                     Query Planner
                           │
                 ┌─────────┴─────────┐
                 │                   │
           IndexDefinition      IndexVersion
                 │                   │
                 └─────────┬─────────┘
                           ▼
                    Retrieval Plan
                           │
                           ▼
                    Provider Boundary
                           │
                           ▼
                  Physical Retrieval
                           │
                           ▼
                 Reference-only Result
                           │
                           ▼
                      QueryResult
                           │
                           ▼
                    UniversalResponse
                           │
                           ▼
                       SOURCE ENGINE
```

The ownership model is:

```text
SOURCE ENGINE
    → meaning

CORE
    → runtime

CONTROL PLANE
    → global coordination

INDEXING
    → index selection, query planning,
      consistency, retrieval orchestration

PROVIDER
    → physical retrieval
```

And the fundamental Phase 4 flow is:

```text
LOGICAL QUERY
      ↓
CONSISTENCY CHECK
      ↓
INDEX SELECTION
      ↓
QUERY PLAN
      ↓
PROVIDER RETRIEVAL
      ↓
REFERENCE RESULT
      ↓
UNIVERSAL RESPONSE
```

The most important architectural rule is:

```text
Logical query semantics belong to the source contract.
Index selection and retrieval planning belong to Indexing.
Physical retrieval belongs to the provider.
Runtime execution belongs to Core.
Domain meaning belongs to the source engine.
```

---

#### 49. Final Phase 4 Summary

Phase 3 made valid index versions exist.

Phase 4 makes those versions usable for generic retrieval without allowing:

```text
domain semantics
physical technology
runtime ownership
global coordination
```

to leak across their architectural boundaries.

In short:

```text
Phase 3
    → build valid indexed state

Phase 4
    → query valid indexed state

Source Engine
    → owns meaning

Core
    → owns execution infrastructure

Indexing
    → owns logical retrieval orchestration

Provider
    → owns physical retrieval
```

That gives the Indexing Engine a clean generic retrieval layer that can serve multiple source/domain engines without becoming a domain-specific search engine or exposing the implementation technology underneath it.

---

### Phase 5: Lifecycle, Capacity, Integrity & Failure Recovery

#### Status

**Not Started**

##### 1. Purpose

Phase 5 makes the Indexing Engine operationally safe under:

- resource pressure
- corruption
- stale data
- rebuilds
- failures

Phase 5 is the operational safety layer around the indexing lifecycle established in Phases 1–4.

The central principle is:

> Phase 5 must make failure and pressure explicit, bounded, observable, and recoverable without allowing invalid index state to become queryable.

---

#### 2. Phase Position

The Indexing Engine progresses through the following architecture:

```text
Phase 0
    ↓
Engine integration foundation

Phase 1
    ↓
Index identity / families / namespaces

Phase 2
    ↓
Logical index data model / contracts

Phase 3
    ↓
Construction / update / rebuild / validation / publication

Phase 4
    ↓
Query / retrieval / consistency

Phase 5
    ↓
Operational safety / lifecycle / capacity /
integrity / failure recovery

Phase 6
    ↓
Security / observability / conformance / hardening
```

The relationship between the phases is:

```text
Phase 3
    makes valid index versions exist

Phase 4
    consumes valid index versions

Phase 5
    protects those versions while the system is operating
```

Phase 5 must extend the Phase 3 and Phase 4 architecture rather than replace it.

---

#### 3. Architectural Boundaries

Phase 5 explicitly preserves these boundaries:

```text
Health ≠ Lifecycle

Validation ≠ Storage

Failure ≠ Retry policy

Index state ≠ Engine state
```

These are core implementation invariants.

Phase 5 must not introduce:

- a second engine runtime
- a second lifecycle system for the engine itself
- a second retry system
- a global scheduler
- a physical storage engine
- domain-specific semantic recovery
- physical-provider algorithms into the logical contract

---

#### 4. Core Ownership

The Nizaam Core runtime remains responsible for universal engine infrastructure.

Core owns:

```text
engine lifecycle
request admission
execution coordination
context propagation
capability dispatch
shutdown coordination
cancellation
deadlines
universal error infrastructure
retry infrastructure
health/readiness infrastructure
configuration infrastructure
```

The Indexing Engine must compose with those mechanisms.

It must not recreate:

```text
IndexingRuntime
IndexingCancellationToken
IndexingDeadline
IndexingRetryManager
```

or equivalent competing systems.

The architectural relationship is:

```text
Core
    ↓
engine lifecycle/runtime

Indexing
    ↓
index lifecycle
```

These are different layers.

---

#### 5. Engine Lifecycle vs Index Lifecycle

Core owns the engine lifecycle.

The established lifecycle is:

```text
Created
  ↓
Starting
  ↓
Configuring
  ↓
Dependencies
  ↓
Capabilities
  ↓
Registering
  ↓
Ready
  ↓
Serving
  ↓
Draining
  ↓
Stopped
```

Only the Core runtime determines whether the Indexing Engine is accepting normal requests.

Phase 5 introduces an independent lifecycle for individual indexes.

Conceptually:

```text
Index
  ↓
Creating
  ↓
Building
  ↓
Validating
  ↓
Ready
  ↓
Active
  ↓
Maintaining
  ↓
Retiring
  ↓
Retired
```

Operational conditions such as:

```text
Stale
Unavailable
Corrupt
Failed
```

should not automatically be treated as lifecycle states. They may represent separate operational dimensions.

For example:

```text
Lifecycle      = Active
Integrity      = Valid
Synchronization = Stale
Availability   = Queryable
Resource       = UnderPressure
```

This separation prevents one enum from becoming a catch-all representation of unrelated concepts.

---

#### 6. `src/lifecycle/`

Planned structure:

```text
src/lifecycle/
├── mod.rs
└── state.rs
```

##### Responsibility

`lifecycle/state.rs` owns index-specific lifecycle state and transition validity.

It should answer:

```text
What lifecycle state is this index in?

Can this lifecycle transition occur?

What transitions are legal?
```

It should not decide:

```text
Is the index corrupt?

Should the operation be retried?

How much resource is available?

Should the index be rebuilt?

Is the engine healthy?
```

Those responsibilities belong to the integrity, capacity, recovery, and Core health/runtime boundaries.

---

#### 7. Index Lifecycle Independence

Different indexes may have different operational states.

For example:

```text
Identity Index       = Active
Inverted Index      = Active
Relationship Index  = Active
Similarity Index    = Failed
```

A failure of one index must not automatically imply:

```text
Engine = Failed
```

unless an explicit architecture policy requires that behavior.

The fundamental invariant remains:

```text
Index state ≠ Engine state
```

---

#### 8. Maintenance

Phase 5 includes index maintenance.

Maintenance should preserve the logical correctness of an existing index without turning Indexing into a physical storage maintenance system.

Conceptually:

```text
Active Index
      ↓
maintenance
      ↓
still-valid index
```

Possible maintenance concerns include:

```text
validation
cleanup
synchronization
metadata refresh
retirement
resource-pressure handling
```

Physical operations such as database page repair, filesystem repair, storage-engine compaction, or provider-specific maintenance remain provider-owned.

---

#### 9. Capacity Model

Phase 5 introduces:

```text
capacity accounting
resource limits
query pressure
build pressure
throttling
```

Capacity does not mean that Indexing owns the machine's resources.

The boundary is:

```text
Infrastructure / runtime environment
          ↓
provides resources

Indexing
          ↓
accounts for workload
and protects operations
```

Indexing can determine:

```text
This operation exceeds its logical limit.

This batch is too large.

This build requires bounded capacity.

This workload is under pressure.
```

It should not become a global resource scheduler.

---

#### 10. `src/capacity/`

Planned structure:

```text
src/capacity/
├── mod.rs
├── limits.rs
└── accounting.rs
```

##### `capacity/limits.rs`

Owns resource and operation limits.

Potential logical limits include:

```text
maximum batch size
maximum concurrent indexing work
maximum query pressure
maximum build pressure
maximum operation budget
```

Exact numerical values should only be introduced where the approved architecture defines them.

The module should represent the concept of a limit rather than hard-code arbitrary physical resource assumptions.

---

#### 11. Capacity Accounting

`capacity/accounting.rs` is distinct from limits.

```text
limits
    → what is allowed

accounting
    → what is currently being consumed
```

Conceptually:

```text
Operation
    ↓
capacity request
    ↓
accounting
    ↓
limit check
    ↓
admit / throttle / reject
```

Accounting may track:

```text
active builds
active queries
pending work
batch usage
estimated resource consumption
```

The purpose is bounded behavior rather than creation of a global scheduler.

---

#### 12. Query Pressure vs Build Pressure

Phase 5 must distinguish at least two major workload categories:

```text
Query workload
    ↓
read/retrieval pressure

Build workload
    ↓
construction/rebuild/update pressure
```

A heavy rebuild must not be allowed to create unbounded pressure that accidentally destroys query availability.

The intended behavior is:

```text
build pressure
    ↓
bounded handling
    ↓
query availability remains protected
```

The exact scheduling mechanism should remain limited to what the approved architecture requires.

---

#### 13. Throttling

Throttling is a controlled response to resource pressure.

Conceptually:

```text
request
   ↓
capacity check
   ↓
enough capacity?
   ├── yes → execute
   └── no  → throttle / defer / reject safely
```

The key invariant is:

```text
resource pressure
    ↓
bounded behavior
```

rather than:

```text
resource pressure
    ↓
unbounded resource consumption
```

---

#### 14. Integrity

Phase 5 explicitly separates:

```text
Validation ≠ Storage
```

Indexing owns logical integrity validation.

The provider owns physical storage behavior.

Indexing can validate:

```text
index metadata
index entries
referenced object identities
version compatibility
rebuild output
publication preconditions
```

It should not become a physical storage repair subsystem.

---

#### 15. `src/integrity/`

Planned structure:

```text
src/integrity/
├── mod.rs
└── validation.rs
```

##### `integrity/validation.rs`

The validator should answer:

```text
Can this index state be trusted according
to the logical Indexing contract?
```

The validation layers are conceptually:

```text
Index Metadata
      ↓
Index Definition
      ↓
Index Version
      ↓
Index Entries
      ↓
Object References
      ↓
Version Compatibility
      ↓
Publication Preconditions
```

Validation must occur before an invalid state can become active.

---

#### 16. Validation and Publication

Phase 3 established:

```text
build
   ↓
validate
   ↓
ready
   ↓
publish
   ↓
active
```

Phase 5 strengthens the same rule.

If an index is invalid:

```text
invalid candidate
    ↓
cannot become active
```

Recovery must eventually return through:

```text
validate
    ↓
publish
```

rather than bypassing validation.

---

#### 17. Trust Boundaries

Phase 5 explicitly includes trust boundaries.

Indexing must not blindly assume that:

```text
reference exists
reference is valid
version is compatible
metadata is trustworthy
provider output is valid
```

The logical trust boundary is:

```text
External / Source Input
        ↓
Indexing validation boundary
        ↓
Trusted logical index state
        ↓
Query / retrieval
```

This is especially important during:

```text
rebuild
recovery
synchronization
provider interaction
```

---

#### 18. Failure Classification

Phase 5 explicitly requires the following distinctions:

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

These must not collapse into one generic failure.

##### Invalid index

The logical/index-level validation rules are not satisfied.

##### Stale index

The index may be structurally valid but does not represent sufficiently current source state.

##### Unavailable index

The index may be valid but cannot currently be used.

##### Corrupt index

The index cannot be trusted because its logical integrity has been violated.

##### Source-data failure

Required source information cannot be obtained or is invalid.

##### Resource exhaustion

The operation cannot continue within available logical resource limits.

##### Provider/storage failure

The physical provider cannot successfully perform the required operation.

##### Query failure

A query cannot be successfully executed.

The distinction between these categories drives recovery behavior.

---

#### 19. `src/recovery/`

Planned structure:

```text
src/recovery/
├── mod.rs
├── failure.rs
└── recovery.rs
```

##### `recovery/failure.rs`

Owns failure classification.

Conceptually:

```text
Failure
   ↓
FailureClass
   ├── InvalidIndex
   ├── StaleIndex
   ├── UnavailableIndex
   ├── CorruptIndex
   ├── SourceDataFailure
   ├── ResourceExhaustion
   ├── ProviderFailure
   └── QueryFailure
```

The exact Rust representation should follow the established error architecture. The architectural requirement is that these failure meanings remain distinguishable.

---

#### 20. Recovery vs Retry

Recovery is not the same as retry.

```text
Retry
    → repeat an operation

Recovery
    → restore a safe operational state
```

Examples:

##### Transient provider failure

Potentially:

```text
failure
    ↓
Core reliability mechanism
    ↓
retry
```

###### Corrupt index

Potentially:

```text
corruption
    ↓
invalidate/isolate
    ↓
rebuild
    ↓
validate
    ↓
publish
```

###### Resource exhaustion

Potentially:

```text
resource exhaustion
    ↓
throttle / defer / reject
```

not:

```text
resource exhaustion
    ↓
retry immediately forever
```

---

#### 21. No Second Retry System

Core already provides retry infrastructure.

Therefore Phase 5 must not introduce a competing:

```text
IndexRetryPolicy
IndexRetryManager
IndexRetryScheduler
```

or equivalent system.

The correct boundary is:

```text
Indexing
    ↓
classify failure
    ↓
determine indexing-specific recovery meaning
    ↓
reuse Core reliability mechanisms where appropriate
```

The scope explicitly requires:

```text
Failure ≠ Retry policy
```

---

#### 22. Recovery Strategy

`recovery/recovery.rs` owns deterministic recovery strategies.

Conceptually:

```text
Failure Classification
        ↓
Recovery Decision
        ↓
┌─────────────────────────────┐
│                             │
▼                             ▼
recover incrementally       rebuild
│                             │
▼                             ▼
validate                    validate
│                             │
└──────────────┬──────────────┘
               ▼
            publish
```

Recovery must return the system to a known safe state.

---

#### 23. Rebuild-Based Recovery

The architecture explicitly allows rebuild to be the final recovery mechanism when an index cannot safely be repaired incrementally.

Phase 3 already established the rebuild lifecycle:

```text
candidate
    ↓
validate
    ↓
publish
```

Phase 5 extends this into:

```text
corruption
    ↓
recovery
    ↓
rebuild candidate
    ↓
validate
    ↓
publish
```

Phase 5 should reuse Phase 3 construction and publication machinery rather than creating a second rebuild system.

---

#### 24. Protect the Last Known-Good Version

A recovery operation must not destroy the active version before the replacement is known to be valid.

Unsafe:

```text
Active V7
    ↓
delete V7
    ↓
build V8
```

If V8 fails, no safe index remains.

Safe:

```text
Active V7
     │
     │ remains protected
     ▼
Recovery
     │
     ▼
Build V8
     │
     ▼
Validate V8
     │
     ▼
Publish V8
     │
     ▼
Active V8
```

This preserves the Phase 3 publication invariant.

---

#### 25. Stale vs Corrupt

These conditions must remain distinct.

```text
Stale
    =
structurally valid,
but behind source state
```

while:

```text
Corrupt
    =
cannot be trusted structurally/logically
```

A stale index may potentially be synchronized.

A corrupt index may require rebuild.

Conceptually:

```text
stale
   ↓
synchronize
   ↓
validate
```

versus:

```text
corrupt
   ↓
invalidate/isolate
   ↓
rebuild
   ↓
validate
```

---

#### 26. Configuration

Phase 5 includes:

```text
src/configuration/
├── mod.rs
└── config.rs
```

Configuration is included as an operational dependency.

It may provide configuration for:

```text
resource limits
capacity policies
maintenance behavior
operational thresholds
recovery-related behavior
```

It must not silently become a mechanism for selecting deferred physical technologies.

For example, operational configuration may define:

```text
max_concurrent_builds = N
```

but should not silently establish a physical implementation such as:

```text
provider = specific_physical_database
algorithm = specific_vector_algorithm
```

unless explicitly authorized by the architecture.

---

#### 27. Core Configuration Boundary

Core already provides the configuration architecture:

```text
Environment
    ↓
Loader
    ↓
Parser
    ↓
Validator
    ↓
Resolver
    ↓
Immutable Snapshot
```

Runtime updates publish a new prepared immutable configuration snapshot rather than exposing partially prepared configuration.

Indexing should consume that mechanism rather than implement an independent configuration lifecycle.

---

#### 28. Health and Readiness

Phase 5 includes:

```text
health/readiness reporting
```

but:

```text
Health ≠ Lifecycle
```

Core already distinguishes:

```text
liveness
readiness
dependency health
capability health
aggregate health
```

Health provides information about operational state. It does not become a second routing or lifecycle authority.

Indexing can report conditions such as:

```text
Index available
Index stale
Index rebuilding
Index degraded
Index unavailable
Capacity pressure
Integrity failure
```

The appropriate Core health/observability mechanisms should carry those observations.

---

#### 29. Core Readiness vs Index Availability

These concepts must remain distinct.

```text
Core readiness
    ↓
Can the Indexing Engine instance serve?
```

while:

```text
Indexing availability
    ↓
Can a particular index currently serve its operation?
```

For example:

```text
Engine
    = SERVING

Identity Index
    = Active

Similarity Index
    = Rebuilding

Relationship Index
    = Stale
```

The exact policy for when an individual index condition changes overall engine readiness must follow an explicitly approved architecture. Phase 5 must not silently invent that policy.

---

#### 30. Phase 4 Interaction

Phase 4 established:

```text
query
    ↓
consistency
    ↓
planner
    ↓
retrieval
```

Phase 5 adds operational protection around that path:

```text
query
    ↓
index availability
    ↓
integrity state
    ↓
capacity admission
    ↓
consistency
    ↓
planner
    ↓
retrieval
```

Phase 5 must not turn recovery into query planning.

For example:

```text
query requires fresh index
        ↓
selected index is stale
        ↓
Phase 4 consistency policy
        ↓
cannot satisfy requirement
```

Any recovery/synchronization decision remains a separate operational workflow.

---

#### 31. Phase 3 Interaction

Phase 3 established:

```text
build
validate
publish
```

Phase 5 adds:

```text
detect operational problem
        ↓
recover
        ↓
reuse build/validate/publish
```

The resulting relationship is:

```text
             Phase 3
        construction/update
               │
       build / validate / publish
               │
               ▼
          active index
               │
               ▼
            Phase 4
         query/retrieval
               │
               ▼
            Phase 5
      operational protection
               │
        ┌──────┴──────┐
        ▼             ▼
      healthy       failure
                      │
                      ▼
                  classify
                      │
                      ▼
                   recover
                      │
              ┌───────┴────────┐
              ▼                ▼
          incremental       rebuild
              │                │
              └───────┬────────┘
                      ▼
                   validate
                      ↓
                   publish
```

---

#### 32. Provider Boundary

Phase 5 preserves the provider abstraction.

For example:

```text
Physical Provider
        ↓
provider error
        ↓
Indexing failure classification
        ↓
Indexing recovery decision
```

Indexing does not need to understand physical details such as:

```text
database page
filesystem block
physical segment
provider-specific index structure
storage-engine internals
```

Those remain behind the provider boundary.

---

#### 33. Planned Test Structure

Phase 5 plans:

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

```text
index lifecycle independent of engine lifecycle
capacity limits
query/build pressure
throttling/bounded behavior
integrity validation
invalid/stale/unavailable/corrupt index states
source-data failures
provider/storage failure abstraction
resource exhaustion
deterministic recovery
rebuild-based recovery
configuration update behavior
```

Negative tests must ensure:

```text
health does not own lifecycle
retry policy is not silently embedded in failure classification
```

---

#### 34. `tests/lifecycle.rs`

Verify:

```text
index lifecycle transitions
valid transitions
invalid transitions
independent index states
index lifecycle ≠ engine lifecycle
retirement
recovery-related transitions
```

Important architectural test:

```text
Engine = SERVING

Index A = Corrupt
Index B = Active
```

must not automatically imply:

```text
Engine = STOPPED
```

unless an explicit architecture policy requires it.

---

#### 35. `tests/capacity.rs`

Verify:

```text
limit enforcement
capacity accounting
bounded batches
query pressure
build pressure
throttling
resource exhaustion
```

Negative cases must prove:

```text
capacity exceeded
    ↓
no unbounded allocation
```

and:

```text
build pressure
    ↓
does not silently create unbounded query degradation
```

---

#### 36. `tests/integrity.rs`

Verify:

```text
metadata validation
entry validation
reference validation
version compatibility
rebuild output validation
publication preconditions
```

Core invariant:

```text
invalid candidate
    ↓
cannot become active
```

---

#### 37. `tests/recovery.rs`

Verify deterministic recovery for:

```text
invalid index
stale index
corrupt index
resource exhaustion
provider failure
source-data failure
```

Tests must prove that recovery cannot produce:

```text
invalid active index
```

and that a known-good active version remains protected during recovery.

---

#### 38. Fault Injection

`fault_injection.rs` should inject deterministic failures at boundaries such as:

```text
index validation
provider operation
source data
resource admission
rebuild
publication
configuration update
```

Then verify:

```text
failure classified correctly
active version protected
no invalid state served
recovery path deterministic
```

Fault injection should reuse real Core APIs and replace only external/test-controlled components with deterministic doubles.

---

#### 39. Stress Testing

`stress.rs` should test bounded behavior under combinations such as:

```text
many queries
+
large batches
+
rebuild pressure
+
resource limits
+
concurrent maintenance
```

The important assertions are:

```text
resource behavior remains bounded
active index remains valid
queries do not observe partial rebuild state
recovery remains deterministic
```

---

#### 40. Configuration Tests

Configuration tests should verify:

```text
configuration parsing
configuration validation
operational configuration update
snapshot/update behavior
invalid configuration rejection
```

They should not silently establish physical implementation choices that remain outside the approved architecture.

---

#### 41. Integration Testing

The Phase 5 integration path should resemble:

```text
Core Runtime
      ↓
Indexing Capability
      ↓
Phase 3 Build / Phase 4 Query
      ↓
Phase 5 operational checks
      ↓
capacity / integrity / lifecycle
      ↓
failure
      ↓
classification
      ↓
recovery
      ↓
validate
      ↓
publish
      ↓
safe active state
```

This proves Phase 5 is integrated with the existing Indexing Engine rather than being a collection of isolated utilities.

---

#### 42. Conformance Testing

Conformance tests should protect the architecture from drift.

Important negative tests include:

```text
Index lifecycle must not replace Core engine lifecycle.

Health must not become lifecycle authority.

Recovery must not become retry policy.

Indexing must not create a second Control Plane.

Indexing must not create a second runtime.

Capacity must not become a global scheduler.

Validation must not become physical storage repair.

Recovery must not expose physical provider algorithms.

Configuration must not silently select deferred technologies.

Domain semantics must not enter lifecycle, capacity,
integrity, or recovery logic.
```

---

#### 43. What Phase 5 Must Not Implement

Phase 5 must not become:

```text
Security subsystem
```

Security belongs to the appropriate security/hardening phase.

It must not become:

```text
Full observability implementation
```

It must not become:

```text
Physical storage engine
```

It must not become:

```text
Database-specific maintenance engine
```

It must not become:

```text
Global resource scheduler
```

It must not become:

```text
Second retry engine
```

It must not become:

```text
Second engine lifecycle runtime
```

It must not become:

```text
Domain semantic recovery engine
```

---

#### 44. Phase 5 Core Invariants

The following invariants should be treated as implementation requirements:

- [ ] Index lifecycle is independent from Core engine lifecycle.

- [ ] Health does not own lifecycle.

- [ ] Index state does not automatically equal engine state.

- [ ] Capacity behavior is bounded.

- [ ] Query pressure is bounded.

- [ ] Build/rebuild pressure is bounded.

- [ ] Throttling does not silently corrupt index state.

- [ ] Integrity validation is separate from physical storage.

- [ ] Invalid index state cannot become active.

- [ ] Stale and corrupt indexes are distinguishable.

- [ ] Unavailable and corrupt indexes are distinguishable.

- [ ] Source-data failure is distinguishable from provider failure.

- [ ] Resource exhaustion is distinguishable from provider failure.

- [ ] Failure classification is separate from retry policy.

- [ ] Recovery is separate from retry.

- [ ] Core retry mechanisms are reused where appropriate.

- [ ] Rebuild-based recovery reuses Phase 3 construction/publication.

- [ ] Active known-good versions remain protected during recovery.

- [ ] Configuration does not silently select deferred physical technologies.

- [ ] Health/readiness observations do not become a second Control Plane.

- [ ] No second runtime is introduced.

- [ ] No global scheduler is introduced.

- [ ] Domain semantics do not enter lifecycle, capacity,
  integrity, or recovery.

- [ ] Physical provider details remain behind the provider boundary.

- [ ] Phase 1–4 behavior remains intact.

---

#### Completion Criteria

Phase 5 is complete when the Indexing Engine has:

#### Verification Checklist

- [ ] index lifecycle independent of engine lifecycle

- [ ] controlled lifecycle transitions

- [ ] capacity limits

- [ ] capacity accounting

- [ ] bounded query pressure

- [ ] bounded build pressure

- [ ] throttling/bounded behavior

- [ ] integrity validation

- [ ] metadata validation

- [ ] entry validation

- [ ] reference validation

- [ ] version compatibility validation

- [ ] rebuild output validation

- [ ] publication precondition validation

- [ ] distinguishable failure classes

- [ ] deterministic recovery decisions

- [ ] rebuild-based recovery where incremental repair is insufficient

- [ ] protection of the last known-good active version

- [ ] validated configuration updates

- [ ] appropriate health/readiness reporting

- [ ] no health-owned lifecycle

- [ ] no retry policy hidden inside failure classification

- [ ] Core runtime reused rather than duplicated

- [ ] Core retry mechanisms reused rather than duplicated

- [ ] physical storage remaining provider-owned

- [ ] domain semantics remaining source-owned

- [ ] Phase 1–4 regression safety

The fundamental completion condition is:

> The engine has bounded behavior under pressure and deterministic recovery paths without silently serving invalid index state.

---

#### 46. Final Mental Model

```text
                         CORE
                          │
              engine lifecycle/runtime
                          │
                          ▼
                     INDEXING
                          │
          ┌───────────────┼────────────────┐
          │               │                │
          ▼               ▼                ▼
      Lifecycle        Capacity        Integrity
          │               │                │
          └───────────────┼────────────────┘
                          ▼
                 Failure Classification
                          │
                          ▼
                      Recovery
                          │
                 ┌────────┴────────┐
                 ▼                 ▼
             Incremental        Rebuild
              recovery             │
                 │                 │
                 └────────┬────────┘
                          ▼
                       Validate
                          │
                          ▼
                       Publish
                          │
                          ▼
                     Safe Index
```

The complete Indexing Engine progression is therefore:

```text
Phase 1
    defines index identity

Phase 2
    defines logical index contracts

Phase 3
    constructs and publishes valid versions

Phase 4
    queries those versions with consistency guarantees

Phase 5
    keeps those versions operationally safe
    under pressure, corruption, stale data, and failure

Phase 6
    hardens the completed engine
```

##### Final Phase 5 Principle

> **Phase 3 made valid index versions exist. Phase 4 made those versions usable for generic retrieval. Phase 5 makes the Indexing Engine capable of operating safely when resources are constrained, indexes become stale or invalid, providers fail, configuration changes, or recovery is required, without allowing invalid state to become active or silently duplicating responsibilities already owned by Nizaam Core.**

---

### Phase 6: Security, Observability, Conformance & Hardening

#### Status

**Not Started**

##### Goal

Complete the Indexing Engine as a production-grade Nizaam infrastructure engine while preserving all established Core boundaries.

Phase 6 is primarily the final integration and hardening phase. It should not introduce a second runtime, Control Plane, security framework, observability platform, configuration system, or event infrastructure. Instead, it completes the Indexing-specific security and observability integration and verifies that the engine correctly participates in the universal Core infrastructure.

The final objective is to prove that the complete Indexing Engine can operate safely inside the Nizaam architecture while preserving:

- Core ownership of universal infrastructure;
- Control Plane ownership of global coordination and routing;
- Indexing ownership of indexing-specific behavior;
- source-engine ownership of domain semantics;
- provider ownership of physical index implementation.

---

#### 1. Phase 6 Architectural Position

The seven-phase progression is:

```text
Phase 0
    ↓
Engine integration foundation

Phase 1
    ↓
Identity / families / namespaces

Phase 2
    ↓
Logical index contracts / data model

Phase 3
    ↓
Construction / update / rebuild / publication

Phase 4
    ↓
Query / retrieval / consistency

Phase 5
    ↓
Lifecycle / capacity / integrity / recovery

Phase 6
    ↓
Security / observability / conformance / hardening
```

Phase 6 therefore asks:

> Can the complete Indexing Engine operate as a production-grade Nizaam infrastructure engine while correctly participating in the Core runtime, security, configuration, observability, health, Control Plane, provenance, and cross-engine infrastructure?

Phase 6 must build on the behavior established by Phases 0–5 rather than rewriting those phases.

---

#### 2. Core Architectural Principle

The primary Phase 6 rule is:

```text
CORE
    ↓
provides universal infrastructure mechanisms

INDEXING
    ↓
uses those mechanisms

INDEXING
    ↓
adds only indexing-specific semantics
```

The Indexing Engine must not create duplicate implementations of universal Core mechanisms.

The intended architecture is:

```text
                         CORE
                          │
        ┌─────────────────┼─────────────────┐
        │                 │                 │
        ▼                 ▼                 ▼
     Security        Observability     Configuration
        │                 │                 │
        └─────────────────┼─────────────────┘
                          │
                          ▼
                  Indexing Engine
                          │
       ┌──────────────────┼──────────────────┐
       │                  │                  │
       ▼                  ▼                  ▼
   Lifecycle          Capacity          Integrity
       │                  │                  │
       └──────────────────┼──────────────────┘
                          ▼
                       Recovery
                          │
                          ▼
                 Build / Query / Update
                          │
                          ▼
                      Provider
                          │
                          ▼
              Physical Implementation
```

---

#### 3. Security Boundary

Core already provides the generic security foundation.

The Core security model distinguishes:

```text
PrincipalType
PrincipalId
PrincipalIdentity
SecurityContext
AuthenticationRequest
Authenticator
AuthorizationRequest
Authorizer
CredentialExtractor
SecurityMiddleware
```

The security flow is:

```text
CredentialExtractor
       ↓
Authenticator
       ↓
trusted PrincipalIdentity
       ↓
Authorizer
       ↓
AuthorizationDecision
       ↓
SecurityContext
       ↓
Capability execution
```

A denied request must stop before capability execution.

##### Phase 6 responsibility

Indexing must integrate with this Core security boundary.

It must not create a second generic:

```text
IndexingAuthenticator
IndexingCredentialManager
IndexingSecurityContext
IndexingAuthorizer
```

when the corresponding universal functionality already exists in Core.

The intended path is:

```text
Universal Request
       ↓
Core runtime
       ↓
Core security pipeline
       ↓
SecurityContext
       ↓
Indexing capability
```

---

#### 4. `src/security/`

Planned structure:

```text
src/
└── security/
    ├── mod.rs
    └── authorization.rs
```

`security/authorization.rs` should provide only the Indexing-specific authorization integration that is required at the capability/resource boundary.

The module must not become a second authorization framework.

Indexing authorization is limited to the Indexing Engine itself. The authorization
boundary must identify the target logical `EngineId` and concrete
`EngineInstanceId` and verify that the request is authorized for that Indexing
engine/instance.

Core remains the implementation mechanism for authentication and authorization.
Indexing supplies only the Indexing-specific authorization requirements that Core
evaluates.

The distinction is:

```text
Core
    ↓
generic authentication / authorization infrastructure
    ↓
Core authorization decision

Indexing
    ↓
Indexing-specific authorization requirements
    ↓
EngineId / EngineInstanceId target boundary
```

Indexing authorization must remain domain-neutral.

It must not define semantic authority for:

```text
Quran
Hadith
Fiqh
Tafsir
Aqeedah
Arabic
Knowledge Graph
Seerah
```

Those meanings belong to the corresponding source/domain systems.

---

#### 5. Security Execution Invariant

The final implementation must prove that authorization actually occurs before Indexing capability execution.

Required invariant:

```text
request admitted
      ↓
mandatory security pipeline
      ↓
authenticated principal
      ↓
authorization decision
      ↓
capability dispatch
      ↓
Indexing handler
```

A weak test such as checking only that a `SecurityContext` exists is insufficient.

The tests must prove that a request cannot bypass the security boundary and directly reach the Indexing capability.

---

#### 6. Operation Context Propagation

Indexing must preserve the execution context supplied by Core.

The conceptual path is:

```text
OperationContext
       ↓
EngineContext
       ↓
SecurityContext
       ↓
Indexing capability
       ↓
Indexing-local workflow
```

Core `EngineContext` already carries universal execution information such as:

```text
OperationContext
CancellationToken
Deadline
SecurityContext
ProvenanceContext
ConfigurationSnapshot
```

Indexing must reuse these mechanisms.

It must not create competing versions of:

```text
IndexingOperationContext
IndexingSecurityContext
IndexingDeadline
IndexingCancellationToken
```

unless a genuinely Indexing-specific context value is required.

Universal runtime concerns remain Core-owned.

---

#### 7. Context Propagation Verification

Phase 6 must verify that context survives the complete execution path:

```text
Control Plane
    ↓
Universal Request
    ↓
Core Runtime
    ↓
EngineContext
    ↓
Indexing Capability
    ↓
Indexing Planner / Workflow
    ↓
Provider
```

Important properties include:

- operation identity remains associated with the request;
- security context remains available;
- cancellation remains available;
- deadlines remain available;
- provenance remains available where required;
- configuration snapshot remains consistent for the operation.

---

#### 8. Artifact and Provenance Integration

Phase 6 must verify artifact/provenance integration for the Indexing operations that create, update, rebuild, publish, recover, or retire indexed state, while reusing the Core provenance and artifact mechanisms.

Core already separates provenance from artifact storage.

The Indexing Engine should therefore reuse the Core provenance mechanisms rather than creating a parallel provenance system.

The conceptual flow is:

```text
Core ProvenanceContext
        ↓
Indexing operation
        ↓
index build / update / query / recovery
        ↓
provenance remains available
```

Indexing-specific operations covered by the provenance/artifact integration include:

```text
index created
index updated
index rebuilt
index version published
index recovered
index retired
```

Indexing may attach appropriate Indexing-specific metadata while preserving the generic Core provenance boundary.

---

#### 9. Observability Boundary

Core provides generic observability mechanisms including:

```text
Correlation
Metrics
Tracing
Diagnostics
Logging
Observability Events
```

Observability describes system behavior. It must not become a correctness dependency.

The architecture should remain:

```text
Indexing operation
       │
       ├── correctness path
       │
       └── observability path
```

A failure of a logging or telemetry destination must not silently corrupt Indexing correctness.

---

#### 10. `src/observability/`

Planned structure:

```text
src/
└── observability/
    ├── mod.rs
    ├── logging.rs
    ├── metrics.rs
    ├── tracing.rs
    └── diagnostics.rs
```

These modules are primarily Indexing integration/adaptation points around the existing Core observability mechanisms.

They must not become a replacement for Core observability.

---

#### 11. Logging

Indexing-specific logs should describe Indexing behavior.

Examples:

```text
index creation started
index build completed
index publication rejected
query rejected because the selected index is stale
index recovery started
index recovery completed
provider failure
capacity pressure
```

The conceptual path is:

```text
Indexing event
      ↓
Core logging mechanism
      ↓
structured log
```

Logging must remain separate from:

```text
metrics
tracing
diagnostics
provenance
events
```

Sensitive information must not be emitted automatically into logs.

---

#### 12. Metrics

Indexing-specific metrics may represent operational measurements such as:

```text
index build count
index build failures
index query count
index query failures
index rebuild count
index recovery count
index publication count
index publication failures
stale index count
capacity rejections
query latency
build latency
```

Exact metric names are implementation details and should be selected consistently with the existing Core metric conventions.

Metric dimensions must remain bounded.

Avoid uncontrolled dimensions such as:

```text
full query text
raw payload
arbitrary object identifiers
unbounded user-provided strings
```

The purpose is operational measurement, not storing request payloads inside the metrics system.

---

#### 13. Tracing

Tracing should preserve the execution path through Indexing.

A query may conceptually appear as:

```text
index.query
    │
    ├── consistency.check
    ├── planner.select
    ├── provider.retrieve
    └── result.materialize
```

A rebuild may conceptually appear as:

```text
index.rebuild
    │
    ├── source.snapshot
    ├── candidate.build
    ├── integrity.validate
    └── publication.publish
```

The trace hierarchy must describe actual execution rather than inventing an unrelated abstraction.

---

#### 14. Diagnostics

Diagnostics should capture structured information useful for investigating Indexing failures.

Potential Indexing-specific diagnostic information includes:

```text
IndexId
IndexVersion
FailureClass
OperationId
AttemptId
Provider boundary
Consistency state
Lifecycle state
```

Diagnostics must not become a second error system.

Core remains the authoritative universal error infrastructure.

The relationship is:

```text
Core error
       ↓
Indexing-specific diagnostic information
       ↓
observability
```

---

#### 15. Events

Core already provides a generic event mechanism involving:

```text
Event
Subscription
Publisher
Delivery
Subscriber
```

Phase 6 must integrate Indexing event communication through Core's universal event interface.

Indexing must consume and publish Core `UniversalEvent` rather than introducing
a separate Indexing event type or event transport. This keeps cross-engine
communication on one Nizaam-wide event interface.

Indexing-specific event meanings may still be represented through the
Indexing-defined content carried by the universal event contract. Potential
Indexing event meanings include:

```text
IndexCreated
IndexUpdated
IndexVersionPublished
IndexRebuildStarted
IndexRebuildCompleted
IndexRecoveryStarted
IndexRecoveryCompleted
IndexRetired
```

These names describe Indexing semantics; they do not create a second event
infrastructure.

The event infrastructure itself remains Core-owned.

Indexing must not create a persistent event bus, replay system, separate event
retry framework, or separate event authorization framework.

---

#### 16. Event, Logging, Metrics, Tracing, and Diagnostics Separation

These concepts must remain distinct:

```text
Logging
    → describes operational occurrences

Event
    → represents a meaningful occurrence that subscribers may consume

Metric
    → measures behavior numerically

Trace
    → describes execution path

Diagnostic
    → explains a failure or operational condition
```

One operation may produce several of these signals without merging them.

For example:

```text
Index rebuild failed

    ├── log
    ├── metric
    ├── trace error
    ├── diagnostic information
    └── IndexRebuildFailed event
```

---

#### 17. Health and Readiness

Core health is an observation mechanism, not an execution or routing authority.

Phase 5 established Indexing operational conditions. Phase 6 integrates those conditions with Core health/readiness.

The intended relationship is:

```text
Indexing health
      ↓
Core health observation
      ↓
Control Plane may consume observation
```

not:

```text
Indexing health
      ↓
Indexing directly controls routing
```

---

#### 18. Index Health vs Engine Readiness

Index-level conditions must remain distinct from engine lifecycle.

For example:

```text
Engine
    = SERVING

Index A
    = healthy

Index B
    = rebuilding

Index C
    = stale
```

An Indexing index condition does not automatically become an engine lifecycle transition.

Similarly, an index condition should not automatically become a global routing decision unless an explicit higher-level policy requires it.

Phase 6 must test this separation.

---

#### 19. Configuration Integration

Core configuration follows the general pipeline:

```text
Environment
    ↓
Loader
    ↓
Parser
    ↓
Validator
    ↓
Resolver
    ↓
Immutable Configuration Snapshot
```

Runtime updates prepare and validate a new snapshot before publication.

Phase 6 should define and verify Indexing-specific configuration while reusing Core configuration infrastructure.

The intended relationship is:

```text
Core configuration mechanism
          ↓
Indexing configuration schema
          ↓
validated snapshot
          ↓
Indexing runtime
```

Indexing must not introduce a second independent configuration runtime.

---

#### 20. Configuration Safety

Configuration updates must not expose partially prepared configuration.

The final behavior should be:

```text
Current Snapshot
      │
      ├── prepare new configuration
      ├── parse
      ├── validate
      ├── resolve
      │
      ├── failure → retain current snapshot
      │
      └── success → publish new snapshot
```

Configuration must also respect the existing security boundary so that sensitive configuration and secret material do not leak through:

```text
logs
metrics
traces
diagnostics
events
errors
```

---

#### 21. Control Plane Integration

The final architecture remains:

```text
Control Plane
    ↓
global coordination / routing
    ↓
Indexing Engine
    ↓
local runtime admission
    ↓
capability
    ↓
Indexing workflow
```

Indexing must not become a second Control Plane.

The Control Plane may determine where a request should be sent, but the selected Indexing engine instance still performs its own local admission and lifecycle checks.

---

#### 22. Local Runtime Admission Remains Authoritative

Example:

```text
Control Plane
    ↓
selects Indexing instance 7
    ↓
Indexing instance 7 is DRAINING
```

The request must not bypass local runtime admission merely because the Control Plane selected that instance.

The correct result is local rejection according to the Core runtime lifecycle and admission rules.

Phase 6 must explicitly test this.

---

#### 23. Cross-Engine Communication

Phase 6 must verify communication between Indexing and other Nizaam engines using the universal communication mechanisms.

The conceptual flow is:

```text
Source Engine
     ↓
Universal Request
     ↓
Control Plane / communication layer
     ↓
Indexing Engine
     ↓
Indexing capability
     ↓
Universal Response
```

Indexing should not introduce an unnecessary special:

```text
IndexingTransport
IndexingRPC
IndexingProtocol
```

when the Core infrastructure already provides the required universal boundary.

---

#### 24. Domain Semantic Boundary

The Indexing Engine remains domain-agnostic even in the final phase.

Indexing must not become the owner of semantic meaning for:

```text
Quran
Hadith
Fiqh
Tafsir
Aqeedah
Arabic
Knowledge Graph
Seerah
```

The source/domain engines remain responsible for those meanings.

Indexing can store, retrieve, filter, rank, or otherwise process source-owned indexed representations according to the generic Indexing contract, but it does not define their domain semantics.

---

#### 25. Conformance Testing

Phase 6 introduces architecture-level verification.

Conformance tests should verify questions such as:

```text
Does Indexing use the Core security boundary?

Does Indexing use Core capability dispatch?

Does Indexing preserve EngineContext?

Does Indexing use Core configuration?

Does Indexing use Core health?

Does Indexing use Core observability?

Does Control Plane remain outside Indexing?

Does source semantic meaning remain outside Indexing?

Does the physical provider remain behind the logical contract?
```

These are architectural tests, not ordinary unit tests.

---

#### 26. Negative Architecture Tests

Phase 6 should actively test forbidden behavior.

##### Security bypass

```text
request
    ↓
bypass authorization
    ↓
Indexing capability
```

Must fail.

###### Health controlling lifecycle

```text
health observation
    ↓
automatic lifecycle mutation
```

Must not happen.

###### Control Plane executing capability

```text
Control Plane
    ↓
direct capability execution
```

Must not happen.

###### Observability controlling correctness

```text
logging/telemetry failure
    ↓
invalid index state
```

Must not happen.

###### Physical implementation leakage

```text
logical contract
    ↓
mandatory HNSW/Lucene/Gin/etc.
```

Must not happen.

###### Domain semantic leakage

```text
Indexing
    ↓
creates source-domain relationship meaning
```

Must not happen.

---

#### 27. Fault Injection

Fault injection must use the real Core APIs wherever possible and replace only external or test-controlled components with deterministic doubles.

The conceptual model is:

```text
Real Core Runtime
       ↓
Real Indexing Engine
       ↓
Injected external/test-controlled failure
```

Potential failure boundaries include:

```text
authentication
authorization
provider/storage
query retrieval
index build
rebuild
recovery
configuration update
event delivery
Control Plane communication
```

The expected response must follow the established error, lifecycle, integrity, and recovery boundaries.

---

#### 28. Stress and Resource Testing

Phase 5 established bounded Indexing behavior.

Phase 6 must verify bounded behavior across the complete system.

Examples include:

```text
high query load
+
concurrent rebuild
+
security middleware
+
tracing
+
metrics
+
logging
+
events
+
Control Plane communication
```

The observability and integration layers must not introduce unbounded resource behavior or undermine the Phase 5 capacity guarantees.

---

#### 29. Planned Source Tree

```text
src/
├── security/
│   ├── mod.rs
│   └── authorization.rs
│
├── observability/
│   ├── mod.rs
│   ├── logging.rs
│   ├── metrics.rs
│   ├── tracing.rs
│   └── diagnostics.rs
│
├── engine/
│   ├── registration.rs
│   ├── runtime.rs
│   └── capability.rs
│
└── lib.rs
```

Phase 6 primarily hardens and integrates existing modules rather than introducing a large new subsystem.

The earlier `engine/` files are updated only as necessary to prove final cross-boundary behavior.

Event, health, Control Plane, and Core runtime integrations are verified through their public interfaces and should not automatically result in Indexing-owned duplicate subsystems.

---

#### 30. Planned Test Tree

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

---

#### 31. `tests/security.rs`

Verify:

```text
authentication boundary
authorization boundary
SecurityContext propagation
denial before capability execution
security failure vs authorization denial
calling-service identity preservation
mandatory security pipeline
```

The central invariant is:

```text
No successful authorization
        ↓
No Indexing capability execution
```

---

#### 32. `tests/observability.rs`

Verify:

```text
logging
metrics
tracing
diagnostics
correlation
events
```

and their separation:

```text
metric ≠ log
trace ≠ log
diagnostic ≠ error replacement
event ≠ persistent event bus
health ≠ routing
```

---

#### 33. `tests/conformance.rs`

This is the architecture guardian.

It should verify:

```text
Core owns universal infrastructure.

Indexing owns indexing semantics.

Source engines own domain semantics.

Physical providers own physical implementation.

Control Plane owns global coordination.

Indexing owns local planning/workflow/execution.

Health does not own lifecycle.

Observability does not own correctness.

Security does not become domain policy.
```

---

#### 34. `tests/integration.rs`

Integration tests verify actual interaction among:

```text
Core
+
Indexing
+
Phase 1
+
Phase 2
+
Phase 3
+
Phase 4
+
Phase 5
```

The objective is to demonstrate that the complete Indexing architecture behaves as one coherent engine.

---

#### 35. `tests/fault_injection.rs`

Verify deterministic handling of failures across:

```text
security
provider
storage
query
build
publication
recovery
configuration
event delivery
Control Plane communication
```

The tests must confirm that failures are classified and propagated through the established Core/Indexing boundaries without silently creating unsafe state.

---

#### 36. `tests/stress.rs`

Verify:

```text
bounded query pressure
bounded build pressure
concurrent operations
rebuild under load
observability under load
event delivery under load
resource exhaustion behavior
```

The test suite should confirm that Phase 5 guarantees survive complete Phase 6 integration.

---

#### 37. `tests/e2e.rs`

The final E2E tests must use the real Core runtime.

The conceptual flow is:

```text
Control Plane
      ↓
Engine discovery / selection
      ↓
Indexing instance
      ↓
UniversalRequest
      ↓
Core runtime
      ↓
Lifecycle admission
      ↓
Security middleware
      ↓
Context propagation
      ↓
Capability resolution
      ↓
Indexing capability
      ↓
Indexing operation
      ↓
Core observability
      ↓
UniversalResponse
```

This is the final proof that the Indexing Engine is integrated into the actual Nizaam runtime rather than only working when called in isolation.

---

#### 38. Final Verification Policy

Testing is continuous throughout all phases, but phase-local tests are not the final completion gate.

The final verification process is:

```text
Implementation phase
        ↓
Focused phase tests
        ↓
Fix / regression checks
        ↓
Next phase
        ↓
...
        ↓
Phase 6
        ↓
Final complete test suite
        ↓
Full regression
        ↓
Conformance
        ↓
Fault injection
        ↓
Stress
        ↓
E2E verification
```

The final pass must verify the complete Indexing Engine rather than only the tests introduced during Phase 6.

---

#### 39. Final Workspace Verification

Final verification should cover the complete workspace, including the previously established Core regression suite.

The final verification should include, as applicable:

```text
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo check --workspace
cargo test --workspace --all-targets
cargo test --workspace --doc
```

Phase 6 must not be considered complete merely because the newly added Phase 6 tests pass.

---

#### 40. Regression Protection

Phase 6 must preserve all established behavior from earlier phases.

The implementation must not casually rewrite previous phase behavior merely to simplify final integration.

The expected approach is:

```text
Existing Phase 0–5 behavior
        ↓
Add final integration
        ↓
Preserve established contracts
        ↓
Run complete regression
```

Any earlier-phase change must be justified by an actual integration or correctness requirement.

---

#### 41. Physical Implementation Replaceability

The final Indexing Engine must continue to preserve:

```text
Logical Query
      ↓
Indexing Planner
      ↓
Provider abstraction
      ↓
Physical implementation
```

The public logical contract must not require a specific physical technology.

Examples of physical implementation details that must remain hidden include:

```text
HNSW
IVF
Lucene
GIN
B-tree
specific database engine
specific storage layout
specific partitioning strategy
```

The physical implementation must remain replaceable behind the logical Indexing contract.

---

#### 42. Final Architectural Boundaries

The final ownership model is:

```text
Core
→ universal infrastructure mechanisms

Control Plane
→ global coordination and routing

Indexing
→ indexing-specific behavior

Source Engine
→ domain semantics

Provider
→ physical implementation
```

And the runtime relationship is:

```text
Control Plane
    ↓
global coordination
    ↓
Indexing
    ↓
local planning + workflow + execution
    ↓
Provider
    ↓
physical implementation
```

---

#### 43. Phase 6 Must Not Become

Phase 6 must not become:

```text
a second security framework
a second observability framework
a second Control Plane
a second runtime
a second configuration system
a persistent event bus
a second universal error system
a domain authorization engine
a physical storage implementation
a domain semantic engine
```

The phase exists to integrate and prove the architecture, not to duplicate infrastructure that Core already provides.

---

#### 44. Complete Phase 6 Mental Model

```text
                   NIZAAM CONTROL PLANE
                           │
                           ▼
                   INDEXING ENGINE
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            ▼            ▼
           SECURITY    CONTEXT      OBSERVABILITY
              │            │            │
              └────────────┼────────────┘
                           ▼
                    INDEXING CORE
                           │
       ┌───────────────────┼───────────────────┐
       │                   │                   │
       ▼                   ▼                   ▼
   Lifecycle           Capacity            Integrity
       │                   │                   │
       └───────────────────┼───────────────────┘
                           ▼
                        Recovery
                           │
                           ▼
                Build / Update / Query
                           │
                           ▼
                     Provider Layer
                           │
                           ▼
                 Physical Implementation
```

---

#### Completion Criteria

Phase 6 is complete only when:

#### Verification Checklist

- [ ] Core security boundary is actually enforced

- [ ] Authorization occurs before capability execution

- [ ] SecurityContext reaches Indexing correctly

- [ ] OperationContext is preserved

- [ ] Core EngineContext remains authoritative

- [ ] Core cancellation and deadline mechanisms remain authoritative

- [ ] Provenance/artifact integration is verified for the defined Indexing operations

- [ ] Artifact handling reuses the Core artifact boundary for the defined Indexing operations

- [ ] Core logging is integrated

- [ ] Core metrics are integrated

- [ ] Core tracing is integrated

- [ ] Core diagnostics are integrated

- [ ] Core UniversalEvent consumption/publication is integrated through the universal event interface

- [ ] Core health/readiness is integrated

- [ ] Core configuration mechanism is reused

- [ ] Control Plane integration is real

- [ ] Local runtime admission remains authoritative

- [ ] Cross-engine communication works through universal mechanisms

- [ ] No domain semantics leak into Indexing

- [ ] No domain semantics leak into Core

- [ ] Physical implementation remains replaceable

- [ ] Security bypass paths are rejected

- [ ] Architecture boundaries have executable tests

- [ ] Fault injection is deterministic

- [ ] Stress behavior remains bounded

- [ ] Real-Core-runtime E2E flows pass

- [ ] Full Indexing test suite passes

- [ ] Core regression suite remains passing

- [ ] Workspace formatting passes

- [ ] Clippy passes with warnings denied

- [ ] Workspace build/check passes

- [ ] Workspace unit/integration/doc tests pass

---

#### 46. Final Phase 6 Summary

The complete Indexing Engine progression is:

```text
Phase 0
How does Indexing become a Nizaam engine?

Phase 1
What is an index and where does it logically exist?

Phase 2
What data and contracts define an index?

Phase 3
How are indexes built, updated, validated and published?

Phase 4
How are indexes queried and retrieved consistently?

Phase 5
How does Indexing remain safe under pressure and failure?

Phase 6
Can the complete Indexing Engine safely participate
in the full Nizaam infrastructure?
```

Phase 6 is therefore the final integration, conformance, and hardening phase.

Its purpose is to prove that the work completed in Phases 0–5 correctly operates inside the Nizaam architecture, uses Core's existing universal mechanisms instead of duplicating them, preserves security and context boundaries, remains observable, integrates with health and configuration, communicates correctly with other engines, and survives the complete final verification suite.

##### Phase 6 Done When

- Full Indexing test suite passes.
- Core regression suite remains passing.
- Architecture boundaries are executable and verified.
- No domain semantics leak into Indexing or Core.
- Physical index implementation remains replaceable behind the logical contract.

---

## Cross-Phase Architectural Invariants

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
→ defines meaning, mappings, relationships, predicates

Indexing
→ defines indexing, index generation, storage, maintenance, retrieval

Storage/provider
→ defines physical persistence/implementation

```

Indexing must never become the owner of the semantic relationship model
merely because it indexes relationship records.

## 3. Semantic data is source-owned

Indexing may receive and index semantic or relationship records, but:

```text

Predicate meaning
Direction meaning
Cardinality meaning
Context meaning
Qualifier meaning
Evidence meaning
Provenance meaning
Authority meaning
Inference meaning

→ remain source-engine semantics

```

Indexing treats source-declared semantic fields as indexable input rather
than as relationships it owns or creates.

## 4. Indexable object granularity

Indexing must be able to assign indexes to different source-declared
object/record categories, including where required:

```text

Word

Mention / Occurrence

Sentence / Span

Passage

Document

Semantic record

Context record

Relationship record

```

These categories remain source-defined. Indexing does not implement them
as domain entities or semantic mapping models.

## 5. Relationship retrieval without relationship ownership

A relationship index or relationship-oriented query capability may exist
as a retrieval mechanism.

It must not:

```text

define predicates

define relationship truth

create semantic mappings

perform domain inference

interpret domain ontology

```

## 6. Similarity separation

Similarity retrieval is an Indexing capability. A source engine may also
define semantic similarity relationships. Those semantic relationships
remain source-owned and must not be confused with the Indexing similarity
structure.

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

or any other concrete implementation unless a later approved phase
explicitly freezes such a choice.

## 8. Shared provider abstraction boundary

The provider abstraction is one top-level Indexing module shared by build,
query, retrieval, and recovery workflows.

It owns reusable generic provider mechanisms, including generic ranking, while
physical provider implementations remain replaceable behind that boundary.

No workflow module should duplicate provider abstraction or generic ranking
logic.

## 9. Control Plane boundary

```text

Control Plane
→ global coordination / admission / routing

Indexing Engine
→ local indexing planning and execution

```

The Indexing Engine must never become a second Control Plane.

## 10. Reference-only boundary

Indexing returns object/index references and retrieval metadata. It does
not silently become the owner of domain object storage or semantic truth.

## 11. Version safety

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

## Explicitly Deferred Decisions

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

The architecture explicitly says exact namespace strings and physical

implementation should remain open until the logical definition is

complete.

## Completion Criteria

The Indexing Engine is complete only when:

- all approved implementation phases are implemented;

- all Indexing unit tests pass;

- all Indexing integration/conformance tests pass;

- full workspace regression tests pass;

- formatting and linting requirements pass;

- Index identity remains separate from physical indexing;

- logical index namespaces remain distinguishable;

- source-defined object and record categories can receive indexes;

- Word/Mention/Span/Passage/Document and other source-declared
categories are indexable without becoming Indexing-owned domain models;

- source-owned semantic and relationship records can be indexed
without Indexing creating or interpreting their meaning;

- index versions can be rebuilt and published safely;

- active indexes remain usable during rebuild;

- stale/corrupt/unavailable states are handled explicitly;

- resource limits remain bounded;

- failure recovery is deterministic;

- Indexing does not acquire KG/domain semantics;

- Indexing does not acquire a second Control Plane;

- physical index technology remains behind the logical contract;

- no unauthorized architectural decisions were introduced.

## Current Repository Scaffold

The current repository scaffold contains 20 directories and 84 files.
The phase plans above map the existing implementation tree to the phase
that owns each subsystem.

After the approved cleanup of semantic-mapping-owned modules, the target
implementation scaffold is 19 directories and 77 files.

These counts describe the pre-provider target snapshot. The approved top-level
provider abstraction is an additional architectural module, so the final
implementation file/directory count must be updated once its exact internal
file decomposition is frozen.

The retained current src/ implementation areas are:

```text

src/

├── bin/

├── build/

├── capacity/

├── configuration/

├── consistency/

├── engine/

├── identity/

├── index/

├── integrity/

├── lifecycle/

├── observability/

├── provider/

├── query/

├── recovery/

├── requirement/

└── security/

```

The semantic mapping/ module is intentionally absent from the target
tree.

The retained test areas are:

```text

tests/

├── build.rs

├── capacity.rs

├── common/

├── configuration.rs

├── conformance.rs

├── consistency.rs

├── e2e.rs

├── engine.rs

├── fault_injection.rs

├── identity.rs

├── index.rs

├── integration.rs

├── integrity.rs

├── lifecycle.rs

├── observability.rs

├── query.rs

├── recovery.rs

├── requirement.rs

├── security.rs

└── stress.rs

```

The semantic tests/mapping.rs test target is intentionally absent.

The presence of a retained file or folder in this scaffold does not mean
that its phase is implemented. It identifies the intended implementation
location for the phase mapping above.

## Current State

Architecture is established sufficiently to begin implementation.

The next implementation work should begin with Phase 0: Engine
Workspace & Integration Foundation, followed by the identity/namespace
/index-space model.

The current repository has both a library crate and standalone binary
entry points. Their final binary-target arrangement should be deliberately
confirmed before the package contract is frozen.

The Indexing Engine is explicitly scoped as a generic indexing
infrastructure engine. It assigns, stores, maintains, and retrieves
indexes for source-owned objects and records. Semantic mappings and their
meanings remain outside the Indexing Engine.

The existing mapping/ module, index/relationship.rs, and
tests/mapping.rs are no longer part of the target architecture because
they make semantic mapping ownership appear inside Indexing.
