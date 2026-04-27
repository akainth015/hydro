# Interfaces

## Public API Entry Points

### FlowBuilder — Program Construction

The primary entry point for building Hydro programs:

```rust
let mut flow = FlowBuilder::new();
let process = flow.process::<MyTag>();
let cluster = flow.cluster::<Workers>();
let external = flow.external::<Client>();
```

**Methods:**
- `process::<Tag>()` → `Process<'a, Tag>` — creates a single-machine location
- `cluster::<Tag>()` → `Cluster<'a, Tag>` — creates a multi-machine location group
- `external::<Tag>()` → `External<'a, Tag>` — creates an external I/O endpoint
- `finalize()` → `BuiltFlow<'a>` — finalizes the IR
- `optimize_with(f)` → `BuiltFlow<'a>` — finalizes with custom IR optimization pass

### Location Trait — Source Methods

All `Location<'a>` types provide data source methods:

| Method | Returns | Description |
|---|---|---|
| `source_iter(q!(vec))` | `Stream<T, L, Bounded, TotalOrder, ExactlyOnce>` | Finite iterator source |
| `source_stream(q!(stream))` | `Stream<T, L, Unbounded, TotalOrder, ExactlyOnce>` | Async stream source |
| `singleton(q!(value))` | `Singleton<T, L, Bounded>` | Single value |
| `source_interval(nondet!(dur))` | `Stream<(), L, Unbounded, NoOrder, ExactlyOnce>` | Timer ticks |
| `tick()` | `Tick<L>` | Enter a clock domain |
| `forward_ref()` | `(ForwardHandle, Stream)` | Forward reference for cycles |
| `spin()` | `Optional<(), L, Unbounded>` | Trigger on every tick |

### Tick Location — Cycle Methods

Inside a `Tick<L>`:

| Method | Returns | Description |
|---|---|---|
| `cycle::<S>()` | `(TickCycleHandle, S)` | Tick-deferred cycle |
| `cycle_with_initial(init)` | `(TickCycleHandle, S)` | Cycle with initial value |

---

## Live Collection Interfaces

### Stream<T, Loc, Bound, Order, Retry>

The primary collection type — a growing sequence of elements.

**Transform operators:**
| Method | Signature | Constraints |
|---|---|---|
| `map(f)` | `Stream<U, L, B, O, R>` | — |
| `flat_map(f)` | `Stream<U, L, B, O, R>` | — |
| `filter(f)` | `Stream<T, L, B, O, R>` | — |
| `filter_map(f)` | `Stream<U, L, B, O, R>` | — |
| `inspect(f)` | `Stream<T, L, B, O, R>` | — |
| `enumerate()` | `Stream<(usize, T), L, B, O, R>` | — |
| `chain(other)` | `Stream<T, L, B, MinOrder, MinRetry>` | — |
| `unique()` | `Stream<T, L, B, O, R>` | `T: Eq + Hash`, `B: Bounded` |
| `sort()` | `Stream<T, L, Bounded, TotalOrder, R>` | `T: Ord`, `B: Bounded` |

**Aggregation operators (require `Bounded`):**
| Method | Signature | Constraints |
|---|---|---|
| `fold(init, f, algebra)` | `Singleton<A, L, Bounded>` | Commutativity proof for `NoOrder` |
| `reduce(f, algebra)` | `Optional<T, L, Bounded>` | Commutativity proof for `NoOrder` |
| `count()` | `Singleton<usize, L, Bounded>` | — |
| `min()` / `max()` | `Optional<T, L, Bounded>` | `T: Ord` |

**Multi-stream operators:**
| Method | Signature | Description |
|---|---|---|
| `cross_product(other)` | `Stream<(T, U), L, B, ...>` | Cartesian product |
| `join(other)` | `Stream<(K, (V1, V2)), L, B, ...>` | Equi-join on key |
| `anti_join(other)` | `Stream<(K, V), L, B, ...>` | Anti-join |
| `difference(other)` | `Stream<T, L, B, ...>` | Set difference |

**Networking:**
| Method | Signature | Description |
|---|---|---|
| `send_bincode(dest)` | `Stream<T, Dest, ...>` | Send via bincode serialization |
| `send_bytes(dest)` | `Stream<Bytes, Dest, ...>` | Send raw bytes |
| `broadcast_bincode(cluster)` | `Stream<T, Cluster, ...>` | Broadcast to all cluster members |

**Timing/batching:**
| Method | Signature | Description |
|---|---|---|
| `tick_batch()` | `Stream<T, Tick<L>, Bounded, ...>` | Batch into tick windows |
| `all_ticks()` | `Stream<T, L, Unbounded, ...>` | Flatten ticks back to unbounded |
| `defer_tick()` | `Stream<T, Tick<L>, Bounded, ...>` | Delay by one tick |
| `persist()` | `Stream<T, Tick<L>, Bounded, ...>` | Persist across ticks |

### Singleton<T, Loc, Bound>

A single value at a location.

| Method | Signature | Description |
|---|---|---|
| `map(f)` | `Singleton<U, L, B>` | Transform value |
| `filter(f)` | `Optional<T, L, B>` | Filter to optional |
| `cross_singleton(other)` | `Singleton<(T, U), L, B>` | Pair with another singleton |
| `zip(other)` | `Singleton<(T, U), L, B>` | Alias for cross_singleton |
| `continue_if(signal)` | `Singleton<T, L, B>` | Gate on optional signal |
| `continue_unless(signal)` | `Singleton<T, L, B>` | Gate on absence of signal |

### Optional<T, Loc, Bound>

Zero or one value at a location.

| Method | Signature | Description |
|---|---|---|
| `map(f)` | `Optional<U, L, B>` | Transform if present |
| `unwrap_or(default)` | `Singleton<T, L, B>` | Default if absent |
| `into_stream()` | `Stream<T, L, B, TotalOrder, ExactlyOnce>` | Convert to 0-or-1 element stream |

### KeyedStream<K, V, Loc, Bound, Order, Retry>

Stream of key-value pairs with key-aware operations.

| Method | Signature | Description |
|---|---|---|
| `fold_keyed(init, f, algebra)` | `KeyedSingleton<K, A, L, B>` | Per-key fold |
| `reduce_keyed(f, algebra)` | `KeyedSingleton<K, V, L, B>` | Per-key reduce |
| `join(other)` | `KeyedStream<K, (V1, V2), L, B, ...>` | Join on key |
| `anti_join_keys(other)` | `KeyedStream<K, V, L, B, ...>` | Anti-join on keys |

---

## Core Traits

### Location<'a>

```rust
pub trait Location<'a>: DynLocation<'a> {
    type Root: Location<'a>;
    fn tick(&self) -> Tick<Self>;
    fn source_iter<T>(...) -> Stream<T, Self, Bounded, TotalOrder, ExactlyOnce>;
    fn source_stream<T>(...) -> Stream<T, Self, Unbounded, TotalOrder, ExactlyOnce>;
    fn forward_ref<S>() -> (ForwardHandle<'a, S>, S);
    // ... more source methods
}
```

### Deploy<'a> — Deployment Backend

```rust
pub trait Deploy<'a> {
    type Process: Node<'a>;
    type Cluster: Node<'a>;
    type External: Node<'a>;
    type Port: Clone;
    type ExternalPort: Clone;

    fn o2o_sink_source(...) -> (ExprSink, ExprSource);
    fn o2m_sink_source(...) -> (ExprSink, ExprSource);
    fn m2o_sink_source(...) -> (ExprSink, ExprSource);
    fn m2m_sink_source(...) -> (ExprSink, ExprSource);
    fn cluster_ids(...) -> Expr;
    fn cluster_self_id(...) -> Expr;
}
```

### Node<'a> — Deployment Node

```rust
pub trait Node<'a> {
    fn next_port(&self) -> Port;
    fn build_metadata(&self) -> Expr;
    fn instantiate(&self) -> Expr;
}
```

### Host — Deployment Target

```rust
pub trait Host: Send + Sync {
    fn target_type(&self) -> HostTargetType;
    fn request_port(&mut self, hint: &PortNetworkHint);
    fn collect_resources(&self, resource_batch: &mut ResourceBatch);
    fn provision(&mut self, resource_result: &ResourceResult) -> impl Future;
    fn strategy_as_server(&self, connection_from: &dyn Host) -> ServerStrategy;
    fn can_connect_to(&self, other: &dyn Host) -> bool;
    fn launched(&self) -> Option<Arc<dyn LaunchedHost>>;
}
```

### Merge — Lattice Join

```rust
pub trait Merge<Other> {
    type Defused;
    fn merge(&mut self, other: Other) -> bool;
    fn merge_owned(self, other: Other) -> Self::Defused;
}
```

### Handoff — Inter-Subgraph Communication

```rust
pub trait Handoff: Default + HandoffMeta {
    type Inner;
    fn take_inner(&self) -> Self::Inner;
    fn borrow_mut_swap(&self) -> RefMut<'_, Self::Inner>;
    fn borrow_mut_give(&self) -> RefMut<'_, Self::Inner>;
}
```

### Pull / Push — Stream Combinators (dfir_pipes)

```rust
pub trait Pull<Ctx: Context> {
    type Item;
    type CanPend: Toggle;
    type CanEnd: Toggle;
    fn poll_next(self: Pin<&mut Self>, ctx: &mut Ctx) -> PollNext<Self::Item>;
}

pub trait Push<Ctx: Context> {
    type Item;
    fn push(&mut self, ctx: &mut Ctx, item: Self::Item);
    fn flush(&mut self, ctx: &mut Ctx);
}
```

---

## Networking Interface

### Configuration Builder Pattern

```rust
// TCP with fail-stop semantics and bincode serialization
TCP.fail_stop().bincode()

// TCP with lossy delivery
TCP.lossy(nondet!(/* reason */)).bincode()

// TCP with lossy + delayed-forever (weakest guarantees)
TCP.lossy_delayed_forever().bincode()
```

### Transport Failure Policies

| Policy | Ordering | Retries | Description |
|---|---|---|---|
| `FailStop` | `TotalOrder` | `ExactlyOnce` | TCP with crash detection |
| `Lossy` | `NoOrder` | `AtLeastOnce` | Messages may be lost/reordered |
| `LossyDelayedForever` | `NoOrder` | `AtLeastOnce` | Messages may be arbitrarily delayed |

---

## Proc Macro Interfaces

### dfir_syntax! — DFIR Surface Syntax

```rust
let mut df = dfir_syntax! {
    source_iter(vec![1, 2, 3])
        -> map(|x| x * 2)
        -> filter(|&x| x > 2)
        -> for_each(|x| println!("{}", x));
};
df.run_available();
```

### setup! — Hydro Crate Initialization

```rust
hydro_lang::setup!();  // In lib.rs of a Hydro crate
```

Initializes stageleft support and test infrastructure.

### q!() — Staged Expression Quoting

```rust
process.source_iter(q!(vec![1, 2, 3]))
    .map(q!(|x| x * 2))
    .for_each(q!(|x| println!("{}", x)));
```

Captures Rust expressions as AST for code generation at each deployment location.

---

## Integration Points

### External Process I/O

```rust
let external = flow.external::<Client>();
let (port, stream) = external.source_bincode::<T>();
// `port` can be connected from outside the Hydro program
// `stream` receives data from the external source
```

### Simulator Interface

```rust
let sim = SimFlow::new(built_flow);
sim.run_to_completion();  // Deterministic execution
sim.fuzz(|input| { ... });  // Bolero-based fuzzing
```

### Visualization

```rust
let built = flow.finalize();
let mermaid = built.to_mermaid();      // Mermaid diagram string
let graphviz = built.to_graphviz();    // DOT format string
let json = built.to_json();            // JSON graph representation
```
