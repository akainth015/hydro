# Data Models

## Compile-Time IR

### HydroNode — High-Level IR

The intermediate representation built by `hydro_lang` collection operations. Each live collection wraps a `RefCell<HydroNode>`.

```mermaid
classDiagram
    class HydroNode {
        <<enum>>
    }

    class Sources {
        Source(stream_expr, location)
        SingletonSource(expr, location)
        CycleSource(cycle_id, location)
        ExternalInput(port, location)
    }

    class Transforms {
        Map(f, input)
        FlatMap(f, input)
        Filter(f, input)
        FilterMap(f, input)
        Inspect(f, input)
        Enumerate(input)
        Sort(input)
        Unique(input)
        Cast(input)
    }

    class Aggregations {
        Fold(init, f, input)
        Reduce(f, input)
        FoldKeyed(init, f, input)
        ReduceKeyed(f, input)
        Scan(init, f, input)
    }

    class MultiInput {
        Join(left, right)
        CrossProduct(left, right)
        CrossSingleton(stream, singleton)
        Difference(pos, neg)
        AntiJoin(pos, neg)
        Chain(first, second)
        Partition(input, count)
    }

    class Timing {
        Batch(input)
        DeferTick(input)
        BeginAtomic(input)
        EndAtomic(input)
        YieldConcat(input)
    }

    class Network {
        Network~config, input, dest~
    }

    class Sharing {
        Tee~SharedNode~
    }

    HydroNode <|-- Sources
    HydroNode <|-- Transforms
    HydroNode <|-- Aggregations
    HydroNode <|-- MultiInput
    HydroNode <|-- Timing
    HydroNode <|-- Network
    HydroNode <|-- Sharing
```

Each node carries `HydroIrMetadata`:
- `location_id: LocationId` — which location this node runs on
- `collection_kind: CollectionKind` — Stream, Singleton, Optional, etc.
- `cardinality: Option<Cardinality>` — estimated output size
- `tag: Option<String>` — debug label
- `op_metadata: HydroIrOpMetadata` — backtrace, CPU usage, unique ID

### HydroRoot — Terminal IR Nodes

Terminal nodes that consume data (no downstream):

| Variant | Description |
|---|---|
| `ForEach(f, input)` | Side-effecting consumption |
| `SendExternal(port, input)` | Send to external process |
| `DestSink(sink_expr, input)` | Send to a futures::Sink |
| `CycleSink(cycle_id, input)` | Complete a forward reference cycle |
| `EmbeddedOutput(input)` | Output in embedded mode |
| `Null(input)` | Dropped/unused stream |

---

## DFIR Graph Model

### DfirGraph — Partitioned Dataflow Graph

The central compile-time graph representation in `dfir_lang`:

```mermaid
classDiagram
    class DfirGraph {
        nodes: SlotMap~GraphNodeId, GraphNode~
        operator_instances: SecondaryMap~GraphNodeId, OperatorInstance~
        graph: DiMulGraph~GraphNodeId, GraphEdgeId~
        ports: SecondaryMap~GraphEdgeId, (PortIndexValue, PortIndexValue)~
        node_loops: SecondaryMap~GraphNodeId, GraphLoopId~
        loop_nodes: SlotMap~GraphLoopId, Vec~GraphNodeId~~
        node_subgraph: SecondaryMap~GraphNodeId, GraphSubgraphId~
        subgraph_nodes: SlotMap~GraphSubgraphId, Vec~GraphNodeId~~
        subgraph_stratum: SecondaryMap~GraphSubgraphId, usize~
    }

    class GraphNode {
        <<enum>>
        Operator(Operator)
        Handoff(src_span, dst_span)
        ModuleBoundary(input)
    }

    class OperatorConstraints {
        name: &str
        hard_range_inn: RangeTrait
        hard_range_out: RangeTrait
        num_args: usize
        is_external_input: bool
        has_singleton_output: bool
        flo_type: Option~FloType~
        input_delaytype_fn: fn
        write_fn: WriteFn
    }

    class PortIndexValue {
        <<enum>>
        Int(usize)
        Path(Ident)
        Elided
    }

    DfirGraph --> GraphNode
    DfirGraph --> OperatorConstraints
    GraphNode --> PortIndexValue
```

### DiMulGraph — Directed Multigraph

```rust
struct DiMulGraph<V: Key, E: Key> {
    edges: SlotMap<E, (V, V)>,           // edge → (src, dst)
    succs: SecondaryMap<V, Vec<E>>,      // vertex → outgoing edges
    preds: SecondaryMap<V, Vec<E>>,      // vertex → incoming edges
}
```

All graph entities use `SlotMap` keys for O(1) lookup with generation-checked safety.

---

## Runtime Data Structures

### Dfir — Runtime Graph Instance

```mermaid
classDiagram
    class Dfir {
        subgraphs: SlotVec~SubgraphTag, SubgraphData~
        loop_data: SecondarySlotVec~LoopTag, LoopData~
        context: Context
        handoffs: SlotVec~HandoffTag, HandoffData~
        metrics: Rc~DfirMetrics~
    }

    class Context {
        current_tick: TickInstant
        current_stratum: usize
        stratum_queues: Vec~VecDeque~SubgraphId~~
        event_queue: VecDeque~Event~
        states: SlotVec~StateTag, StateData~
        loop_nonce_stack: Vec~LoopNonce~
    }

    class VecHandoff~T~ {
        input: Rc~RefCell~Vec~T~~~
        output: Rc~RefCell~Vec~T~~~
    }

    class TeeingHandoff~T~ {
        tees: Vec~VecHandoff~T~~
    }

    class SubgraphData {
        subgraph: Box~dyn Subgraph~
        stratum: usize
        loop_id: Option~LoopId~
        is_scheduled: bool
    }

    Dfir --> Context
    Dfir --> VecHandoff
    Dfir --> TeeingHandoff
    Dfir --> SubgraphData
```

### StateHandle — Operator State

```rust
struct StateHandle<T> {
    key: StateKey,
    _phantom: PhantomData<T>,
}
```

Operators access persistent state via `context.state_ref(handle)` / `context.state_mut(handle)`. State persists across ticks based on the operator's persistence lifetime (`'none`, `'loop`, `'tick`, `'static`, `'mutable`).

---

## Type-Level Models

### Boundedness

```mermaid
classDiagram
    class Boundedness {
        <<sealed trait>>
    }
    class Bounded {
        <<struct>>
    }
    class Unbounded {
        <<struct>>
    }
    Boundedness <|-- Bounded
    Boundedness <|-- Unbounded
```

### Ordering

```mermaid
classDiagram
    class Ordering {
        <<sealed trait>>
    }
    class TotalOrder {
        <<struct>>
    }
    class NoOrder {
        <<struct>>
    }
    Ordering <|-- TotalOrder
    Ordering <|-- NoOrder

    class MinOrder~Other~ {
        <<trait>>
        type Min: Ordering
    }
```

`MinOrder` computes the weaker ordering: `TotalOrder ∧ TotalOrder = TotalOrder`, otherwise `NoOrder`.

### Retries

```mermaid
classDiagram
    class Retries {
        <<sealed trait>>
    }
    class ExactlyOnce {
        <<struct>>
    }
    class AtLeastOnce {
        <<struct>>
    }
    Retries <|-- ExactlyOnce
    Retries <|-- AtLeastOnce

    class MinRetries~Other~ {
        <<trait>>
        type Min: Retries
    }
```

### Algebraic Properties

```mermaid
classDiagram
    class AggFuncAlgebra~C, I, M~ {
        <<phantom type>>
        C: Commutativity
        I: Idempotence
        M: Monotonicity
    }

    class Proved {
        <<struct>>
    }
    class NotProved {
        <<struct>>
    }

    class ValidCommutativityFor~Order~ {
        <<trait>>
    }
    class ValidIdempotenceFor~Retry~ {
        <<trait>>
    }

    NotProved ..|> ValidCommutativityFor : for TotalOrder
    Proved ..|> ValidCommutativityFor : for any Order
    NotProved ..|> ValidIdempotenceFor : for ExactlyOnce
    Proved ..|> ValidIdempotenceFor : for any Retry
```

---

## Location Models

### LocationId

```rust
enum LocationId {
    Process(usize),
    Cluster(usize),
    External(usize),
}
```

### MemberId

```rust
struct MemberId<Tag> {
    id: u32,
    _phantom: PhantomData<Tag>,
}
```

Type-safe cluster member addressing. `Tag` matches the `Cluster<'a, Tag>` to prevent cross-cluster member confusion.

---

## Lattice Types

### Core Lattice Hierarchy

```mermaid
classDiagram
    class Lattice {
        <<trait alias>>
        Merge + LatticeOrd + IsBot + IsTop
    }

    class Min~T: Ord~ {
        val: T
        +merge(other) bool
    }

    class Max~T: Ord~ {
        val: T
        +merge(other) bool
    }

    class SetUnion~S: Set~ {
        set: S
        +merge(other) bool
    }

    class MapUnion~M: Map~ {
        map: M
        +merge(other) bool
    }

    class Pair~A: Lattice, B: Lattice~ {
        a: A
        b: B
        +merge(other) bool
    }

    class DomPair~K: Ord, V: Lattice~ {
        key: K
        val: V
        +merge(other) bool
    }

    class WithBot~L: Lattice~ {
        val: Option~L~
        +merge(other) bool
    }

    class WithTop~L: Lattice~ {
        val: Option~L~
        +merge(other) bool
    }

    class Conflict~T~ {
        val: Option~T~
        +merge(other) bool
    }

    Lattice <|.. Min
    Lattice <|.. Max
    Lattice <|.. SetUnion
    Lattice <|.. MapUnion
    Lattice <|.. Pair
    Lattice <|.. DomPair
    Lattice <|.. WithBot
    Lattice <|.. WithTop
    Lattice <|.. Conflict
```

### Merge Semantics

| Type | Merge Operation | Bot | Top |
|---|---|---|---|
| `Min<T>` | `min(self, other)` | `T::MAX` | `T::MIN` |
| `Max<T>` | `max(self, other)` | `T::MIN` | `T::MAX` |
| `SetUnion<S>` | Set union | `∅` | — |
| `MapUnion<M>` | Union with per-key merge | `∅` | — |
| `Pair<A, B>` | Component-wise merge | `(⊥_A, ⊥_B)` | `(⊤_A, ⊤_B)` |
| `DomPair<K, V>` | Higher key dominates; equal keys merge values | — | — |
| `WithBot<L>` | Adds explicit bottom (`None`) | `None` | — |
| `WithTop<L>` | Adds explicit top (`None` = ⊤) | — | `None` |
| `Conflict<T>` | Equal values merge; different values → ⊤ (conflict) | `None` | conflict state |

---

## Deployment Models

### ServerStrategy

```rust
enum ServerStrategy {
    Direct(PortConfig),
    Many(Vec<PortConfig>),
    Demux(HashMap<u32, ServerStrategy>),
    Merge(Vec<ServerStrategy>),
    Tagged(Box<ServerStrategy>, u32),
    Null,
}
```

### HostTargetType

```rust
enum HostTargetType {
    Local,
    Linux(LinuxCompileType),
}

enum LinuxCompileType {
    Musl,
    Gnu,
}
```

### ResourcePool / ResourceBatch / ResourceResult

The deployment resource lifecycle:
1. `ResourceBatch` — collects resource requests from hosts/services
2. `ResourcePool` — provisions resources (Terraform, SSH, etc.)
3. `ResourceResult` — provides provisioned resource handles back to hosts
