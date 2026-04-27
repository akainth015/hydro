# AGENTS.md — Hydro Distributed Programming Framework

> Concise guide for AI agents working in this codebase. For detailed documentation, see `.agents/summary/index.md`.

## What This Project Is

Hydro is a Rust framework for writing distributed programs that are **correct by construction**. The type system enforces ordering, exactly-once delivery, and boundedness at compile time. Programs are written using a staged DSL (`q!()` macro from `stageleft`), compiled to DFIR (Dataflow Intermediate Representation) dataflow graphs, and executed on a stratum-based runtime scheduler.

Rust 2024 edition. Pinned to **Rust 1.93.1** via `rust-toolchain.toml`.

## Directory Map

```
hydro_lang/             ← Core framework: Stream, Singleton, Location, compilation pipeline
  src/live_collections/ ← Stream, KeyedStream, Singleton, Optional, KeyedSingleton
  src/location/         ← Process, Cluster, External, Tick, Atomic
  src/compile/          ← FlowBuilder → BuiltFlow → DeployFlow → CompiledFlow
  src/compile/ir/       ← HydroNode IR (~40 variants)
  src/networking/       ← TCP config builder (fail_stop, lossy, bincode)
  src/properties/       ← Algebraic property proofs (commutativity, idempotence)
  src/deploy/           ← Deploy backends (hydro_deploy, Docker, ECS, Maelstrom)
  src/sim/              ← Deterministic simulator + bolero fuzzing

dfir_lang/              ← DFIR compiler: parse → flat graph → partition → codegen
  src/graph/            ← DfirGraph, partitioning, operator definitions
  src/graph/ops/        ← ~80+ operator definitions (OperatorConstraints)
  src/parse.rs          ← Surface syntax parser

dfir_macro/             ← Proc macros: dfir_syntax!, DemuxEnum derive
dfir_rs/                ← Runtime: Dfir scheduler, Context, handoffs, state
  src/scheduled/        ← Scheduler, subgraphs, handoff implementations
dfir_pipes/             ← #![no_std] Pull/Push stream combinators

hydro_std/              ← Distributed patterns: quorum, request_response, compartmentalize
hydro_test/             ← Integration tests: cluster/, distributed/, maelstrom/, tutorials/

hydro_deploy/core/      ← Deployment: Localhost, GCP, Azure, AWS, Docker, ECS
hydro_deploy/hydro_deploy_integration/  ← Runtime-side deploy protocol

lattices/               ← Lattice types: Min, Max, SetUnion, MapUnion, Pair, etc.
  src/ght/              ← Generalized hash tries
lattices_macro/         ← #[derive(Lattice)]

variadics/              ← Variadic generics via tuple lists: var_expr!, var_type!, var_args!
variadics_macro/        ← Variadic proc macros

sinktools/              ← futures::Sink adaptors and SinkBuild API
copy_span/              ← Proc-macro span copying for error messages
include_mdtests/        ← Markdown-as-doctests proc macro
multiplatform_test/     ← #[multiplatform_test] attribute
hydro_build_utils/      ← Build-time nightly detection, snapshot helpers

template/               ← cargo-generate templates for new projects
website_playground/     ← WASM playground (compiles DFIR in browser)
docs/                   ← Docusaurus website (hydro.run)
benches/                ← Microbenchmarks
design_docs/            ← Historical design documents
```

## Key Architectural Patterns

### Staged Programming
All user expressions are wrapped in `q!(...)` (from `stageleft`). These are captured as AST at compile time and emitted into generated DFIR code per deployment location. This is not string-based codegen — it's type-checked Rust.

### Type-Level Safety Parameters
`Stream<T, Loc, Bound, Order, Retry>` — every collection carries:
- **Loc** — `Process<Tag>`, `Cluster<Tag>`, `Tick<L>`, `Atomic<L>`
- **Bound** — `Bounded` or `Unbounded`
- **Order** — `TotalOrder` or `NoOrder`
- **Retry** — `ExactlyOnce` or `AtLeastOnce`

Network sends weaken these parameters based on transport policy. `fold()` on `NoOrder` streams requires a commutativity proof.

### Compilation Pipeline
`FlowBuilder` → `BuiltFlow` (finalize IR) → `DeployFlow` (assign hosts, resolve network) → `CompiledFlow` (emit DFIR graphs) → `DeployResult` (provision + launch). Each stage is a distinct Rust type.

### DFIR Graph Compilation
`HydroNode` IR → `FlatGraphBuilder` → `DfirGraph` (partitioned into subgraphs with handoffs and strata) → Rust `TokenStream`. Operators are colored push/pull; handoffs inserted at boundaries.

### Runtime Execution
Stratum-based tick scheduler. Within a tick, strata execute in order (0, 1, 2...). Each stratum runs subgraphs to fixpoint via FIFO queues. Loops iterate until no new data. Handoffs are double-buffered `Vec<T>`.

## Repo-Specific Patterns and Gotchas

### Nondeterministic Iteration is Banned
`clippy.toml` bans `iter()`, `drain()`, `keys()`, `values()` on `HashMap`, `HashSet`, and `SparseSecondaryMap`. Use `BTreeMap`/`BTreeSet` or sort before iterating.

### NonDet Guard Required
APIs like `source_interval()`, `batch()`, `sample_every()` require a `NonDet` parameter via `nondet!(/// doc comment explaining why)`. The macro enforces a doc-comment.

### Snapshot Testing
- **insta** for graph visualization snapshots
- **trybuild** for compile-fail error message snapshots
- Auto-update: `INSTA_FORCE_PASS=1 INSTA_UPDATE=always TRYBUILD=overwrite`
- CI nightly auto-creates PRs for snapshot changes

### Test Serialization
`nextest.toml` serializes trybuild tests (`hydro_lang`, `hydro_std`, `hydro_test`) and integration tests (`deadlock_detector`, `two_pc_hf`, `kvs`, `echo_local`) to avoid conflicts.

### Formatting
Uses **nightly rustfmt** (`cargo +nightly fmt`). Config in `rustfmt.toml` enables doc comment formatting, module-level import grouping, and other unstable features.

### SlotMap-Based IDs
All graph entities (nodes, edges, subgraphs, loops, handoffs, states) use `SlotMap` keys for O(1) lookup with generation-checked safety. This is pervasive in `dfir_lang` and `dfir_rs`.

### Persistence Lifetimes
DFIR operators have persistence lifetimes: `'none`, `'loop`, `'tick`, `'static`, `'mutable`. These control how operator state persists across ticks/iterations.

### Feature Flag Layering
- `runtime_support` — minimal re-exports for generated code (no compiler deps)
- `build` — enables DFIR compilation (adds dfir_lang, backtrace, ctor)
- `deploy` — enables deployment (adds hydro_deploy, trybuild)
- `sim` — enables deterministic simulator (adds bolero)

## Config Files Worth Reading

| File | Why |
|---|---|
| `clippy.toml` | Banned nondeterministic methods — will cause CI failures if violated |
| `.config/nextest.toml` | Test group serialization — explains why some tests run slowly |
| `.cargo/config.toml` | Default RUST_LOG levels, rust-lld linker config |
| `rust-toolchain.toml` | Pinned Rust version + required components/targets |
| `.vscode/settings.json` | Snapshot auto-update env vars for IDE test runs, `nondet!` highlighting |
| `precheck.bash` | Local CI simulation — run before submitting PRs |

## Detailed Documentation

For deeper information, consult `.agents/summary/index.md` which provides a table of contents with guidance on which file to read for specific questions:

- **architecture.md** — System design, compilation pipeline, type safety system
- **components.md** — What each crate does and how they relate
- **interfaces.md** — Public API, operators, traits, networking
- **data_models.md** — IR nodes, graph structures, lattice types, runtime data
- **workflows.md** — Compilation flow, runtime execution, CI/CD, testing
- **dependencies.md** — Internal/external deps, policies, versioning

## Custom Instructions

<!-- This section is maintained by developers and agents during day-to-day work.
     It is NOT auto-generated by codebase-summary and MUST be preserved during refreshes.
     Add project-specific conventions, gotchas, and workflow requirements here. -->
