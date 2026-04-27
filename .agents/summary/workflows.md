# Workflows

## Compilation Pipeline

### End-to-End: User Code → Running Distributed System

```mermaid
sequenceDiagram
    participant User as User Code
    participant FB as FlowBuilder
    participant IR as HydroNode IR
    participant BF as BuiltFlow
    participant DF as DeployFlow
    participant DL as dfir_lang
    participant RT as dfir_rs Runtime
    participant HD as hydro_deploy

    User->>FB: flow.process(), flow.cluster()
    User->>IR: stream.map(), .filter(), .fold()
    Note over IR: Builds HydroNode tree<br/>via RefCell mutation

    FB->>BF: flow.finalize()
    Note over BF: Collects IR roots<br/>Runs unify_atomic_ticks()

    BF->>DF: built.with_process(p, spec)
    Note over DF: Associates locations<br/>with deploy specs

    DF->>DF: compile_network()
    Note over DF: Resolves Network nodes<br/>to sink/source pairs

    DF->>DL: emit() per location
    Note over DL: HydroNode → FlatGraphBuilder<br/>→ DfirGraph

    DL->>DL: partition_graph()
    Note over DL: Subgraph partitioning<br/>Handoff insertion<br/>Stratification

    DL->>DL: as_code()
    Note over DL: DfirGraph → Rust TokenStream

    DL->>RT: Generated code compiled
    Note over RT: Dfir::new() + subgraphs<br/>+ handoffs registered

    DF->>HD: deploy()
    Note over HD: Provision hosts<br/>Compile binaries<br/>Wire network

    HD->>RT: Launch binaries
    RT->>RT: run_available()
```

### DFIR Graph Compilation Detail

```mermaid
flowchart TD
    A["DFIR Surface Syntax<br/>(or HydroNode emit)"] --> B["Parse → DfirCode AST"]
    B --> C["FlatGraphBuilder::from_dfir()"]
    C --> D["Resolve variable references"]
    D --> E["Create GraphNode::Operator nodes"]
    E --> F["Create edges from -> connections"]
    F --> G["Handle loop {} blocks"]
    G --> H["eliminate_extra_unions_tees()"]
    H --> I["partition_graph()"]

    I --> J["Find barrier crossers<br/>(DelayType edges)"]
    J --> K["Union-Find subgraph merging<br/>(push/pull coloring)"]
    K --> L["Insert GraphNode::Handoff<br/>at subgraph boundaries"]
    L --> M["Stratification via<br/>topological sort"]
    M --> N["DfirGraph (partitioned)"]

    N --> O["as_code()"]
    O --> P["Generate Dfir::new()"]
    O --> Q["Generate make_edge()<br/>for each handoff"]
    O --> R["Generate add_subgraph_stratified()<br/>for each subgraph"]
    O --> S["Serialize DfirGraph as JSON<br/>(debug metadata)"]
```

---

## Runtime Execution

### Tick Execution Model

```mermaid
stateDiagram-v2
    [*] --> WaitForEvents

    WaitForEvents --> Stratum0: External event arrives
    
    state "Tick N" as TickN {
        Stratum0 --> Stratum1: All S0 subgraphs done
        Stratum1 --> Stratum2: Barrier (Stratum delay)
        Stratum2 --> CheckMore: All strata done

        state Stratum0 {
            [*] --> RunS0Subgraphs
            RunS0Subgraphs --> ScheduleSuccessors: Check handoffs
            ScheduleSuccessors --> RunS0Subgraphs: More ready
            ScheduleSuccessors --> [*]: Queue empty
        }

        state Stratum1 {
            [*] --> RunS1Subgraphs
            RunS1Subgraphs --> ScheduleS1Successors
            ScheduleS1Successors --> RunS1Subgraphs: More ready
            ScheduleS1Successors --> [*]: Queue empty
        }
    }

    CheckMore --> WaitForEvents: No more work
    CheckMore --> Stratum0: Data flowed to lower stratum
```

### Subgraph Execution

```mermaid
sequenceDiagram
    participant Sched as Scheduler
    participant SG as Subgraph Closure
    participant Recv as RecvCtx (input)
    participant Send as SendCtx (output)
    participant HO as Handoff

    Sched->>SG: run(context, handoffs)
    SG->>Recv: Read from input handoffs
    Note over SG: Pull iterator chain<br/>→ Push closure chain
    SG->>Send: Write to output handoffs
    Send->>HO: Data buffered in output Vec
    Sched->>HO: Check if non-empty
    alt Handoff has data
        Sched->>Sched: Schedule successor subgraph
    end
```

### Loop Execution

```mermaid
flowchart TD
    A["Enter loop block"] --> B["Push loop nonce"]
    B --> C["Run loop subgraphs"]
    C --> D{"allow_another_iteration?"}
    D -->|Yes| E["Increment iter_count"]
    E --> C
    D -->|No| F["Pop loop nonce"]
    F --> G["Continue to next stratum"]
```

---

## Development Workflows

### Local Development Cycle

```mermaid
flowchart LR
    A["Edit code"] --> B["cargo clippy"]
    B --> C["cargo nextest run"]
    C --> D{"Tests pass?"}
    D -->|No| A
    D -->|Yes| E["cargo +nightly fmt"]
    E --> F["precheck.bash"]
    F --> G{"All checks pass?"}
    G -->|No| A
    G -->|Yes| H["Commit + PR"]
```

### precheck.bash Workflow

```mermaid
flowchart TD
    A["precheck.bash --all"] --> B["cargo +nightly fmt --all"]
    B --> C["cargo clippy<br/>(no-default-features)"]
    C --> D["cargo clippy<br/>(all-features)"]
    D --> E["cargo clippy<br/>(selected features)"]
    E --> F["cargo check --all-targets<br/>--no-default-features"]
    F --> G["cargo nextest run<br/>(INSTA_FORCE_PASS=1<br/>TRYBUILD=overwrite)"]
    G --> H["cargo test --doc"]
    H --> I{"--wasm?"}
    I -->|Yes| J["wasm-pack build<br/>website_playground"]
    J --> K["wasm-pack test<br/>dfir_rs"]
    I -->|No| L["cargo +nightly doc<br/>--cfg docsrs"]
    K --> L
```

### CI Pipeline

```mermaid
flowchart TD
    A["Push / PR"] --> B["pre_job<br/>(skip duplicates)"]
    B --> C["lint job<br/>(clippy + fmt)"]
    B --> D["test job<br/>(nextest + doctest)"]
    B --> E["test-wasm job"]

    C --> F{"Matrix"}
    F --> G["ubuntu + stable<br/>(sccache)"]
    F --> H["ubuntu + nightly"]
    F --> I["windows + stable"]
    F --> J["macos + stable"]

    D --> K{"Matrix"}
    K --> L["ubuntu + stable<br/>(+ Maelstrom + CDK synth)"]
    K --> M["ubuntu + nightly<br/>(auto-update snapshots)"]
    K --> N["windows + stable"]
    K --> O["macos + stable"]

    M -->|"Snapshot changes"| P["Create PR for<br/>snapshot updates"]
    M -->|"Failures"| Q["Create/update<br/>GitHub issue"]
```

### Release Workflow

```mermaid
sequenceDiagram
    participant Dev as Developer
    participant GH as GitHub Actions
    participant CSR as cargo-smart-release
    participant CR as crates.io

    Dev->>GH: Manual dispatch<br/>(version bump choice)
    GH->>CSR: Calculate version bumps
    CSR->>CSR: Generate changelogs
    CSR->>CSR: Update Cargo.toml versions
    CSR->>GH: Commit changes
    GH->>GH: Push to main<br/>(via Bot App)
    GH->>CR: Publish lockstep crates
    Note over CR: dfir_rs, dfir_lang,<br/>hydro_lang, hydro_std,<br/>hydro_deploy, etc.
    GH->>GH: Post-release validation
```

---

## Deployment Workflow

### Deploy Lifecycle

```mermaid
sequenceDiagram
    participant User as User Code
    participant DF as DeployFlow
    participant Dep as Deployment
    participant Host as Host(s)
    participant Svc as Service(s)

    User->>DF: deploy(&mut deployment)
    DF->>DF: build_inner()<br/>(compile DFIR graphs)
    DF->>DF: Generate trybuild binaries

    DF->>Dep: Register hosts + services
    Dep->>Host: collect_resources()
    Note over Host: Gather resource requests<br/>(ports, IPs, etc.)

    Dep->>Dep: provision()
    Note over Dep: Terraform apply<br/>SSH setup<br/>Binary compilation

    Dep->>Host: provision(resources)
    Host->>Svc: deploy()
    Svc->>Svc: ready()
    Svc->>Svc: start()

    Note over Svc: Running distributed system
```

### Network Wiring

```mermaid
flowchart TD
    A["compile_network()"] --> B{"Connection type?"}
    B -->|"Process → Process"| C["o2o_sink_source()"]
    B -->|"Process → Cluster"| D["o2m_sink_source()"]
    B -->|"Cluster → Process"| E["m2o_sink_source()"]
    B -->|"Cluster → Cluster"| F["m2m_sink_source()"]

    C --> G["TCP connection<br/>with configured policy"]
    D --> G
    E --> G
    F --> G

    G --> H["ServerStrategy<br/>(Direct/Many/Demux/Merge)"]
```

---

## Testing Workflows

### Snapshot Testing

```mermaid
flowchart TD
    A["Run tests"] --> B{"Snapshot exists?"}
    B -->|No| C["INSTA_FORCE_PASS=1<br/>creates new snapshot"]
    B -->|Yes| D{"Matches?"}
    D -->|Yes| E["Test passes"]
    D -->|No| F{"INSTA_UPDATE=always?"}
    F -->|Yes| G["Auto-update snapshot"]
    F -->|No| H["Test fails<br/>cargo insta review"]
```

### Simulator Testing

```mermaid
flowchart TD
    A["SimFlow::new(built)"] --> B["Configure inputs"]
    B --> C["run_to_completion()"]
    C --> D["Assert outputs"]

    A --> E["Fuzz testing"]
    E --> F["bolero + libfuzzer"]
    F --> G["Generate random inputs"]
    G --> C
```

### Trybuild Compile-Fail Tests

```mermaid
flowchart TD
    A["tests/compile-fail/*.rs"] --> B["trybuild::TestCases"]
    B --> C["Compile each file"]
    C --> D{"Compilation fails?"}
    D -->|Yes| E["Compare error output<br/>to .stderr snapshot"]
    D -->|No| F["Test fails<br/>(should have failed)"]
    E --> G{"Matches snapshot?"}
    G -->|Yes| H["Test passes"]
    G -->|No| I["TRYBUILD=overwrite<br/>to update"]
```
