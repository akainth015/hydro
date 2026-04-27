# Architecture

## Layered Architecture

Hydro follows a layered compiler architecture where high-level distributed programs are progressively lowered through intermediate representations to executable dataflow graphs.

```mermaid
graph TB
    subgraph "User Code"
        A["Hydro DSL<br/>(hydro_lang)"]
    end

    subgraph "High-Level Layer"
        B["HydroNode IR<br/>(compile/ir)"]
        C["Network Resolution<br/>(compile_network)"]
    end

    subgraph "Compilation"
        D["FlatGraphBuilder<br/>(dfir_lang)"]
        E["DfirGraph<br/>(partitioned)"]
        F["Rust TokenStream<br/>(codegen)"]
    end

    subgraph "Runtime"
        G["Dfir Scheduler<br/>(dfir_rs)"]
        H["Subgraphs + Handoffs"]
        I["Pull/Push Operators<br/>(dfir_pipes)"]
    end

    subgraph "Deployment"
        J["Deploy Backends<br/>(hydro_deploy)"]
    end

    A -->|"staged compilation<br/>(stageleft q!)"| B
    B -->|"compile_network()"| C
    C -->|"emit()"| D
    D -->|"partition_graph()"| E
    E -->|"as_code()"| F
    F -->|"compile + deploy"| G
    G --> H
    H --> I
    J -.->|"provisions hosts<br/>wires network"| G
```

## Core Design Principles

### 1. Staged Programming (stageleft)

User code is written using the `q!(...)` quoting macro from `stageleft`. Expressions inside `q!()` are captured as AST at compile time and emitted into generated DFIR code for each deployment location. This enables:
- Type-safe code generation without string templates
- IDE support (autocomplete, type checking) for distributed programs
- Zero-cost abstractions — generated code is monomorphized Rust

### 2. Type-Level Distributed Safety

The type system encodes distributed properties as phantom type parameters:

```mermaid
classDiagram
    class Stream~T, Loc, Bound, Order, Retry~ {
        +map(f) Stream
        +filter(f) Stream
        +fold(init, f) Singleton
        +send_bincode(dest) Stream
    }

    class Boundedness {
        <<trait>>
    }
    class Bounded
    class Unbounded

    class Ordering {
        <<trait>>
    }
    class TotalOrder
    class NoOrder

    class Retries {
        <<trait>>
    }
    class ExactlyOnce
    class AtLeastOnce

    Boundedness <|-- Bounded
    Boundedness <|-- Unbounded
    Ordering <|-- TotalOrder
    Ordering <|-- NoOrder
    Retries <|-- ExactlyOnce
    Retries <|-- AtLeastOnce

    Stream --> Boundedness
    Stream --> Ordering
    Stream --> Retries
```

When data crosses network boundaries, the type parameters are weakened based on transport properties (e.g., TCP lossy → `NoOrder`, `AtLeastOnce`). Operations like `fold` on unordered streams require algebraic property proofs (commutativity, idempotence).

### 3. Location-Typed Computation

Every computation is associated with a `Location` — a typed representation of where code runs:

```mermaid
graph TB
    subgraph "Top-Level Locations"
        P["Process&lt;'a, Tag&gt;<br/>Single machine"]
        C["Cluster&lt;'a, Tag&gt;<br/>Group of machines"]
        E["External&lt;'a, Tag&gt;<br/>External I/O"]
    end

    subgraph "Nested Locations"
        T["Tick&lt;L&gt;<br/>Clock domain"]
        AT["Atomic&lt;L&gt;<br/>Atomicity wrapper"]
    end

    P --> T
    C --> T
    T --> AT
```

Phantom tag types (e.g., `Process<'a, Leader>` vs `Process<'a, Follower>`) distinguish locations at the type level, preventing accidental cross-location data access.

### 4. Lattice-Based CRDTs

The `lattices` crate provides algebraic types where merge is associative, commutative, and idempotent — the foundation for conflict-free replicated data types:

```mermaid
graph TB
    L["Lattice trait<br/>(Merge + LatticeOrd + IsBot + IsTop)"]
    L --> MN["Min&lt;T&gt;"]
    L --> MX["Max&lt;T&gt;"]
    L --> SU["SetUnion&lt;S&gt;"]
    L --> MU["MapUnion&lt;M&gt;"]
    L --> P["Pair&lt;A, B&gt;"]
    L --> DP["DomPair&lt;K, V&gt;"]
    L --> WB["WithBot&lt;L&gt;"]
    L --> WT["WithTop&lt;L&gt;"]
    L --> CF["Conflict&lt;T&gt;"]
    L --> UF["UnionFind"]
```

### 5. Push-Pull Dataflow Execution

The DFIR runtime uses a hybrid push-pull execution model within subgraphs:

```mermaid
graph LR
    subgraph "Pull Region"
        S1["source_iter"] --> M1["map"] --> F1["filter"]
    end

    subgraph "Handoff"
        H["VecHandoff<br/>(double-buffered)"]
    end

    subgraph "Push Region"
        FE["for_each"] 
    end

    F1 --> H --> FE
```

- **Pull operators** are composed as Rust iterators (lazy evaluation)
- **Push operators** are composed as closure chains (eager evaluation)
- **Handoffs** (double-buffered `Vec<T>`) connect subgraphs across push-pull boundaries

### 6. Stratified Scheduling

The runtime executes subgraphs in strata to handle non-monotonic operations correctly:

```mermaid
sequenceDiagram
    participant S0 as Stratum 0<br/>(sources)
    participant S1 as Stratum 1<br/>(monotone ops)
    participant S2 as Stratum 2<br/>(non-monotone ops)
    participant Loop as Loop Block

    Note over S0,S2: Tick N
    S0->>S1: data via handoffs
    S1->>S1: run to fixpoint
    S1->>S2: barrier (Stratum delay)
    S2->>S2: run to fixpoint

    Note over Loop: Loop iteration
    Loop->>Loop: repeat until no new data

    Note over S0,S2: Tick N+1
    S0->>S1: new external events
```

## Compilation Pipeline Detail

```mermaid
flowchart LR
    subgraph "Stage 1: Build"
        FB["FlowBuilder<br/>create locations<br/>build IR trees"]
    end

    subgraph "Stage 2: Finalize"
        BF["BuiltFlow<br/>collect IR roots<br/>optimize"]
    end

    subgraph "Stage 3: Deploy Spec"
        DF["DeployFlow<br/>assign hosts<br/>resolve network"]
    end

    subgraph "Stage 4: Compile"
        CF["CompiledFlow<br/>emit DFIR graphs<br/>generate binaries"]
    end

    subgraph "Stage 5: Run"
        DR["DeployResult<br/>provision + launch"]
    end

    FB -->|"finalize()"| BF
    BF -->|"with_process/cluster()"| DF
    DF -->|"compile()"| CF
    CF -->|"deploy()"| DR
```

Each stage is a distinct type (`FlowBuilder` → `BuiltFlow` → `DeployFlow` → `CompiledFlow` → `DeployResult`), enforcing correct ordering via Rust's type system.

## Deployment Architecture

```mermaid
graph TB
    subgraph "Deploy Trait"
        DT["Deploy&lt;'a&gt;<br/>o2o/o2m/m2o/m2m sink_source<br/>cluster_ids, cluster_self_id"]
    end

    subgraph "Backends"
        LH["LocalhostHost"]
        GCP["GcpComputeEngineHost"]
        AZ["AzureHost"]
        AWS["AwsEc2Host"]
        DK["Docker"]
        ECS["ECS"]
        EM["Embedded"]
        SIM["Simulator"]
        ML["Maelstrom"]
    end

    DT --> LH
    DT --> GCP
    DT --> AZ
    DT --> AWS
    DT --> DK
    DT --> ECS
    DT --> EM
    DT --> SIM
    DT --> ML
```

The `Deploy<'a>` trait abstracts over deployment targets. Each backend implements host provisioning, binary compilation/copying, network wiring, and service lifecycle management.

## Non-Determinism Tracking

Hydro requires explicit documentation of every non-determinism source via the `NonDet` type and `nondet!` macro:

```rust
// Must explain WHY this is non-deterministic
let timer = process.source_interval(nondet!(
    /// Timer for heartbeat — ordering of heartbeats relative to
    /// other messages is non-deterministic
    Duration::from_secs(1)
));
```

APIs like `source_interval()`, `batch()`, `sample_every()` require a `NonDet` parameter. The `nondet!` macro enforces a doc-comment explanation.

## Algebraic Property System

For operations on unordered/at-least-once streams, the type system requires proofs:

```mermaid
graph LR
    subgraph "Properties"
        CP["CommutativeProof"]
        IP["IdempotentProof"]
        MP["MonotoneProof"]
        OP["OrderPreservingProof"]
    end

    subgraph "Validation"
        VO["ValidCommutativityFor&lt;Order&gt;"]
        VI["ValidIdempotenceFor&lt;Retry&gt;"]
    end

    CP --> VO
    IP --> VI

    subgraph "Usage"
        F["stream.fold(init, f, algebra)"]
    end

    VO --> F
    VI --> F
```

- `NotProved` commutativity is valid for `TotalOrder` streams (order guaranteed)
- `NotProved` commutativity requires `Proved` for `NoOrder` streams
- `ManualProof` + `manual_proof!` macro allows human-written justifications
