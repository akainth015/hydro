# Hydro Codebase Documentation Index

> **For AI Assistants:** This file is the primary entry point for understanding the Hydro codebase. Read this file first to determine which detailed documentation files to consult for specific questions. Each section below summarizes a documentation file and describes when to consult it.

## Project Summary

**Hydro** is a high-level distributed programming framework for Rust that provides compile-time safety guarantees for distributed systems. It uses a type system to enforce ordering, exactly-once delivery, and boundedness properties — analogous to how Rust enforces memory safety. Programs are compiled through staged programming (stageleft) into DFIR (Dataflow Intermediate Representation) dataflow graphs that execute on a stratum-based runtime scheduler.

The repository is a Rust 2024 monorepo with 22 workspace crates, pinned to Rust 1.93.1.

---

## Documentation Files

### [codebase_info.md](codebase_info.md)
<!-- tags: overview, structure, tooling, configuration, features -->

**Consult when:** You need basic project metadata, directory layout, technology stack, toolchain versions, feature flags, or configuration file purposes.

**Contains:** Project identity, technology stack table, full workspace directory tree, key configuration files and their purposes, feature flag reference.

**Key facts:**
- Rust 2024 edition, pinned to 1.93.1 stable
- 22 workspace crates organized into DFIR engine, Hydro framework, Deploy, and Foundations layers
- Uses cargo-nextest for testing, nightly rustfmt, cargo-smart-release for publishing
- Custom clippy rules ban nondeterministic iteration on HashMap/HashSet

---

### [architecture.md](architecture.md)
<!-- tags: architecture, design, compilation, type-system, safety, staging -->

**Consult when:** You need to understand the overall system design, compilation pipeline stages, type-level safety system, how staged programming works, the push-pull execution model, stratified scheduling, or deployment architecture.

**Contains:** Layered architecture diagram, 6 core design principles with diagrams, detailed compilation pipeline (FlowBuilder → BuiltFlow → DeployFlow → CompiledFlow → DeployResult), deployment backend architecture, non-determinism tracking system, algebraic property system.

**Key concepts:**
- Staged programming via `stageleft` and `q!()` macro
- Type parameters encode Boundedness, Ordering, and Retries
- Locations (Process, Cluster, External, Tick, Atomic) are typed
- Push-pull hybrid execution with double-buffered handoffs
- Stratified scheduling ensures correct non-monotonic computation

---

### [components.md](components.md)
<!-- tags: components, crates, modules, responsibilities -->

**Consult when:** You need to understand what a specific crate does, find which crate owns a particular responsibility, or understand the relationships between crates.

**Contains:** Component dependency map, detailed descriptions of all 22 crates organized by layer (User-Facing, DFIR Engine, Deploy, Foundations, Build Utilities), non-crate components (templates, docs, scripts).

**Key crate responsibilities:**
- `hydro_lang` — Core DSL, live collections, locations, compilation pipeline, networking
- `dfir_lang` — DFIR compiler (parse → graph → partition → codegen)
- `dfir_rs` — Runtime scheduler, handoffs, context, state management
- `lattices` — CRDT lattice types (Merge, SetUnion, MapUnion, etc.)
- `hydro_deploy/core` — Multi-cloud deployment (localhost, GCP, Azure, AWS, Docker, ECS)
- `hydro_std` — Distributed patterns (quorum, request-response, compartmentalize)

---

### [interfaces.md](interfaces.md)
<!-- tags: api, traits, methods, operators, networking, macros -->

**Consult when:** You need to understand the public API, available operators on collections, trait signatures, networking configuration, proc macro usage, or integration points.

**Contains:** FlowBuilder API, Location trait methods, complete operator tables for Stream/Singleton/Optional/KeyedStream, core trait signatures (Location, Deploy, Node, Host, Merge, Handoff, Pull, Push), networking builder pattern, proc macro interfaces (dfir_syntax!, setup!, q!()), external process I/O, simulator interface, visualization API.

**Key interfaces:**
- `Stream<T, Loc, Bound, Order, Retry>` — ~30 operators (map, filter, fold, join, send_bincode, etc.)
- `Location<'a>` trait — source_iter, source_stream, tick, forward_ref
- `Deploy<'a>` trait — o2o/o2m/m2o/m2m sink_source for network wiring
- `Merge` trait — lattice join operation
- TCP networking: `TCP.fail_stop().bincode()` builder pattern

---

### [data_models.md](data_models.md)
<!-- tags: types, structs, enums, ir, graph, lattice, runtime -->

**Consult when:** You need to understand specific data structures, the IR node types, graph representation, runtime data structures, type-level models (Boundedness, Ordering, Retries), lattice type hierarchy, or deployment models.

**Contains:** HydroNode IR enum variants (~40 variants), HydroRoot terminal nodes, DfirGraph structure (SlotMap-based), DiMulGraph adjacency representation, Dfir runtime instance structure, Context fields, VecHandoff/TeeingHandoff, StateHandle, type-level marker types, algebraic property types, LocationId/MemberId, lattice type hierarchy with merge semantics table, ServerStrategy enum, deployment resource lifecycle.

**Key data structures:**
- `HydroNode` — ~40 IR variants (Sources, Transforms, Aggregations, MultiInput, Timing, Network, Sharing)
- `DfirGraph` — SlotMap-based graph with nodes, edges, subgraphs, loops, strata
- `Dfir<'a>` — Runtime instance with subgraphs, handoffs, context, metrics
- Lattice types — Min, Max, SetUnion, MapUnion, Pair, DomPair, WithBot, WithTop, Conflict

---

### [workflows.md](workflows.md)
<!-- tags: workflows, compilation, execution, ci, testing, deployment, release -->

**Consult when:** You need to understand how the compilation pipeline executes step-by-step, the runtime tick/stratum execution model, development workflows, CI pipeline structure, release process, deployment lifecycle, or testing workflows.

**Contains:** End-to-end compilation sequence diagram, DFIR graph compilation flowchart, tick execution state diagram, subgraph execution sequence, loop execution flow, local development cycle, precheck.bash workflow, CI pipeline (lint/test/wasm matrix), release workflow with cargo-smart-release, deployment lifecycle sequence, network wiring flow, snapshot/simulator/trybuild testing workflows.

**Key workflows:**
- Compilation: FlowBuilder → BuiltFlow → DeployFlow → CompiledFlow → deploy
- Runtime: Stratum-based tick execution with FIFO queues and loop iteration
- CI: Matrix (ubuntu/windows/macos × stable/nightly), sccache, auto-snapshot updates
- Release: Manual dispatch → cargo-smart-release → crates.io publish
- Testing: nextest + insta snapshots + trybuild compile-fail + bolero fuzzing

---

### [dependencies.md](dependencies.md)
<!-- tags: dependencies, crates, versions, policies, no_std, versioning -->

**Consult when:** You need to understand dependency relationships between workspace crates, external dependency purposes, dependency policies (nondeterministic iteration ban, no_std support), feature flag conventions, or lockstep versioning groups.

**Contains:** Internal dependency graph with feature-gated edges, external dependency tables organized by category (staged programming, async runtime, serialization, proc macros, graph data structures, cloud providers, testing), dependency policies, no_std crate list, feature flag conventions, build-time dependency patterns, lockstep versioning groups.

**Key policies:**
- HashMap/HashSet/SparseSecondaryMap nondeterministic iteration is banned via clippy
- `dfir_pipes`, `variadics`, `sinktools` support no_std
- Lockstep versioning: dfir_* crates together, hydro_lang + hydro_std together, hydro_deploy + integration together

---

## Quick Reference: Where to Find Information

| Question | File(s) to Consult |
|---|---|
| What does crate X do? | [components.md](components.md) |
| How do I use Stream/Singleton/etc.? | [interfaces.md](interfaces.md) |
| What operators are available? | [interfaces.md](interfaces.md) |
| How does compilation work? | [architecture.md](architecture.md), [workflows.md](workflows.md) |
| What are the type-level safety guarantees? | [architecture.md](architecture.md), [data_models.md](data_models.md) |
| How does the runtime execute? | [workflows.md](workflows.md), [data_models.md](data_models.md) |
| What is the IR structure? | [data_models.md](data_models.md) |
| How do lattices work? | [data_models.md](data_models.md), [components.md](components.md) |
| How do I deploy a program? | [workflows.md](workflows.md), [interfaces.md](interfaces.md) |
| What are the project's dependencies? | [dependencies.md](dependencies.md) |
| How does CI/testing work? | [workflows.md](workflows.md) |
| What toolchain/config is used? | [codebase_info.md](codebase_info.md) |
| What feature flags exist? | [codebase_info.md](codebase_info.md), [dependencies.md](dependencies.md) |
| How do I add a new crate? | [codebase_info.md](codebase_info.md) (workspace structure), RELEASING.md |
