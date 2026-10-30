//! Styled wrappers for live collections used with the `sliced!` macro.
//!
//! This module provides wrapper types that store both a collection and its associated
//! non-determinism guard, allowing the nondet to be properly passed through during slicing.

#[cfg(stageleft_runtime)]
use std::marker::PhantomData;
use std::sync::Arc;

use stageleft::q;

use super::Slicable;
#[cfg(stageleft_runtime)]
use crate::forward_handle::{CycleCollection, CycleCollectionWithInitial};
use crate::forward_handle::{TickCycle, TickCycleHandle};
use crate::handoff_ref::{SlicedOptionalRef, SlicedSingletonRef};
use crate::live_collections::boundedness::{Bounded, Boundedness, IsBounded, Unbounded};
use crate::live_collections::keyed_singleton::{
    BoundedValue, KeyedSingletonBound, KeyedSingletonSnapshotBound,
};
use crate::live_collections::optional::OptionalBound;
use crate::live_collections::singleton::SingletonBound;
use crate::live_collections::stream::{ExactlyOnce, Ordering, Retries, TotalOrder};
use crate::location::Location;
use crate::location::tick::{DeferTick, Tick};
use crate::nondet::{NonDet, nondet};

/// Default style wrapper that stores a collection and its non-determinism guard.
///
/// This is used by the `sliced!` macro when no explicit style is specified. This style is
/// deprecated; use the explicit [`batch`] or [`snapshot`] styles instead.
pub struct Default<T> {
    pub(crate) collection: T,
    pub(crate) nondet: NonDet,
}

impl<T> Default<T> {
    /// Creates a new default-styled wrapper.
    pub fn new(collection: T, nondet: NonDet) -> Self {
        Self { collection, nondet }
    }
}

/// Helper function for unstyled `use` in `sliced!` macro - wraps the collection in Default style.
#[doc(hidden)]
#[deprecated(
    note = "use `use::batch(...)` for stream-like collections or `use::snapshot(...)` for singleton-like collections instead"
)]
pub fn default<T>(t: T, nondet: NonDet) -> Default<T> {
    Default::new(t, nondet)
}

/// Batch style wrapper that stores a stream-like collection and its non-determinism guard.
///
/// This is used by the `sliced!` macro when `use::batch(...)` is specified.
pub struct Batch<T, H = ()> {
    pub(crate) collection: T,
    pub(crate) nondet: NonDet<H>,
}

impl<T, H> Batch<T, H> {
    /// Creates a new batch-styled wrapper.
    pub fn new(collection: T, nondet: NonDet<H>) -> Self {
        Self { collection, nondet }
    }
}

/// Wraps a stream-like live collection (such as a [`Stream`](crate::live_collections::Stream),
/// [`KeyedStream`](crate::live_collections::KeyedStream), or a
/// [`KeyedSingleton`](crate::live_collections::KeyedSingleton) with bounded values) to be
/// sliced into non-deterministic batches of asynchronously arriving elements.
pub fn batch<T, H>(t: T, nondet: NonDet<H>) -> Batch<T, H> {
    Batch::new(t, nondet)
}

/// Snapshot style wrapper that stores a singleton-like collection and its non-determinism guard.
///
/// This is used by the `sliced!` macro when `use::snapshot(...)` is specified.
pub struct Snapshot<T, H = ()> {
    pub(crate) collection: T,
    pub(crate) nondet: NonDet<H>,
}

impl<T, H> Snapshot<T, H> {
    /// Creates a new snapshot-styled wrapper.
    pub fn new(collection: T, nondet: NonDet<H>) -> Self {
        Self { collection, nondet }
    }
}

/// Wraps a singleton-like live collection (such as a
/// [`Singleton`](crate::live_collections::Singleton),
/// [`Optional`](crate::live_collections::Optional), or a
/// [`KeyedSingleton`](crate::live_collections::KeyedSingleton) with asynchronously updated
/// values) to be sliced into non-deterministic snapshots of its continuously changing value.
pub fn snapshot<T, H>(t: T, nondet: NonDet<H>) -> Snapshot<T, H> {
    Snapshot::new(t, nondet)
}

/// Atomic style wrapper that stores a collection and its non-determinism guard.
///
/// This is used by the `sliced!` macro when `use::atomic(...)` is specified.
pub struct Atomic<T> {
    pub(crate) collection: T,
    pub(crate) nondet: NonDet,
}

impl<T> Atomic<T> {
    /// Creates a new atomic-styled wrapper.
    pub fn new(collection: T, nondet: NonDet) -> Self {
        Self { collection, nondet }
    }
}

/// Wraps a live collection to be treated atomically during slicing.
pub fn atomic<T>(t: T, nondet: NonDet) -> Atomic<T> {
    Atomic::new(t, nondet)
}

/// Reference style wrapper that stores a bounded singleton-like collection.
///
/// This is used by the `sliced!` macro when `use::reference(...)` is specified.
pub struct Reference<T> {
    pub(crate) collection: T,
}

impl<T> Reference<T> {
    /// Creates a new reference-styled wrapper.
    pub fn new(collection: T) -> Self {
        Self { collection }
    }
}

/// Wraps a top-level **bounded** singleton-like live collection (a
/// [`Singleton`](crate::live_collections::Singleton) or an
/// [`Optional`](crate::live_collections::Optional)), such as a configuration value fixed when
/// the flow is built, to be read by reference in every slice.
///
/// Unlike [`snapshot`], this takes no non-determinism guard: a bounded collection has a single
/// immutable value, so every slice observes that same value. Unbounded collections are
/// rejected at compile time (use [`snapshot`] for those).
///
/// Inside the `sliced!` body, the binding is a reference handle that can be captured in `q!()`
/// closures over bounded collections in the slice: a [`SlicedSingletonRef`] resolving to `&T`
/// for a singleton, or a [`SlicedOptionalRef`] resolving to `Option<&T>` for an optional (which,
/// being a value rather than a reference, must be captured by a `move` closure). The handle is
/// `Copy`, so it can be captured by any number of closures. The value is held behind an
/// [`Arc`] across ticks, so it is not cloned on each tick and `T` does not need to implement
/// [`Clone`].
///
/// ```rust
/// # #[cfg(feature = "deploy")] {
/// # use hydro_lang::prelude::*;
/// # use futures::StreamExt;
/// # tokio_test::block_on(hydro_lang::test_util::stream_transform_test(|process| {
/// let offset = process.singleton(q!(100));
/// let input = process.source_iter(q!(vec![1, 2, 3]));
/// sliced! {
///     let batch = use::batch(input, nondet!(/** test */));
///     let offset = use::reference(offset);
///     batch.map(q!(|x| x + *offset))
/// }
/// # }, |mut stream| async move {
/// // 101, 102, 103
/// # for w in vec![101, 102, 103] {
/// #     assert_eq!(stream.next().await.unwrap(), w);
/// # }
/// # }));
/// # }
/// ```
pub fn reference<T>(t: T) -> Reference<T> {
    Reference::new(t)
}

/// The slice produced by [`reference`], holding the per-tick copy of the shared ([`Arc`]) value.
///
/// Inside the `sliced!` body, the `use::reference(...)` binding is rebound to the handle
/// returned by `by_ref` ([`SlicedSingletonRef`] or [`SlicedOptionalRef`]).
pub struct ReferenceSlice<C> {
    collection: C,
}

impl<'a, T, L: Location<'a>>
    ReferenceSlice<crate::live_collections::Singleton<Arc<T>, Tick<L>, Bounded>>
{
    /// Creates a reference handle to the shared singleton value, which resolves to `&T` when
    /// captured inside `q!()` closures over bounded collections in the slice.
    pub fn by_ref(&self) -> SlicedSingletonRef<'a, '_, T, Tick<L>, Bounded> {
        SlicedSingletonRef::new(&self.collection.ir_node)
    }
}

impl<'a, T, L: Location<'a>>
    ReferenceSlice<crate::live_collections::Optional<Arc<T>, Tick<L>, Bounded>>
{
    /// Creates a reference handle to the shared optional value, which resolves to `Option<&T>`
    /// when captured inside `q!()` closures over bounded collections in the slice.
    pub fn by_ref(&self) -> SlicedOptionalRef<'a, '_, T, Tick<L>, Bounded> {
        SlicedOptionalRef::new(&self.collection.ir_node)
    }
}

/// Creates a stateful cycle with an initial value for use in `sliced!`.
///
/// The tick (which is the source of truth for lifetimes) is bound first, returning a
/// [`StateBuilder`] which accepts the user-provided initializer via [`StateBuilder::build`].
/// This two-step layout ensures that type errors caused by a bad initializer are attributed
/// to the initializer argument rather than the tick or the entire macro invocation.
///
/// The initial value is computed from a closure that receives the location
/// for the body of the slice.
///
/// The initial value is used on the first iteration, and subsequent iterations receive
/// the value assigned to the mutable binding at the end of the previous iteration.
#[cfg(stageleft_runtime)]
pub fn state<'t, S, L>(tick: &'t Tick<L>) -> StateBuilder<'t, S, L> {
    StateBuilder {
        tick,
        _phantom: PhantomData,
    }
}

/// Builder returned by [`state`], which accepts the user-provided initializer.
#[cfg(stageleft_runtime)]
pub struct StateBuilder<'t, S, L> {
    tick: &'t Tick<L>,
    _phantom: PhantomData<S>,
}

#[cfg(stageleft_runtime)]
impl<'t, 'a, S, L: Location<'a>> StateBuilder<'t, S, L> {
    /// Supplies the initializer closure and creates the stateful cycle.
    ///
    /// The initializer takes the tick at the builder's `'t` lifetime (rather than a
    /// higher-ranked `for<'x>` bound), since the builder already stores the tick reference.
    /// This way, an initializer that requires a specific tick reference lifetime produces a
    /// borrow error directly on the tick, instead of a confusing "implementation of `Fn` is
    /// not general enough" error that blames an unrelated variable.
    #[expect(
        private_bounds,
        reason = "only Hydro collections can implement CycleCollectionWithInitial"
    )]
    pub fn build(self, initial_fn: impl FnOnce(&'t Tick<L>) -> S) -> (TickCycleHandle<'a, S>, S)
    where
        S: CycleCollectionWithInitial<'a, TickCycle, Location = Tick<L::DropConsistency>>,
    {
        let initial = initial_fn(self.tick);
        initial.location().clone().cycle_with_initial(initial)
    }
}

/// Creates a stateful cycle without an initial value for use in `sliced!`.
///
/// The tick (which is the source of truth for lifetimes) is bound first, returning a
/// [`StateNullBuilder`] which creates the cycle via [`StateNullBuilder::build`].
///
/// On the first iteration, the state will be null/empty. Subsequent iterations receive
/// the value assigned to the mutable binding at the end of the previous iteration.
#[cfg(stageleft_runtime)]
pub fn state_null<'t, S, L>(tick: &'t Tick<L>) -> StateNullBuilder<'t, S, L> {
    StateNullBuilder {
        tick,
        _phantom: PhantomData,
    }
}

/// Builder returned by [`state_null`], which creates the cycle.
#[cfg(stageleft_runtime)]
pub struct StateNullBuilder<'t, S, L> {
    tick: &'t Tick<L>,
    _phantom: PhantomData<S>,
}

#[cfg(stageleft_runtime)]
impl<'t, 'a, S, L: Location<'a>> StateNullBuilder<'t, S, L> {
    /// Creates the stateful cycle, which starts as null/empty on the first iteration.
    #[expect(
        private_bounds,
        reason = "only Hydro collections can implement CycleCollection"
    )]
    pub fn build(self) -> (TickCycleHandle<'a, S>, S)
    where
        S: CycleCollection<'a, TickCycle, Location = Tick<L::DropConsistency>> + DeferTick,
    {
        self.tick.cycle::<S, _>()
    }
}

// ============================================================================
// Default style Slicable implementations
//
// All of these drop consistency because they are performing non-deterministic
// batching / snapshotting.
// ============================================================================

impl<'a, T, L: Location<'a>, B: Boundedness, O: Ordering, R: Retries>
    Slicable<'a, L::DropConsistency> for Default<crate::live_collections::Stream<T, L, B, O, R>>
{
    type Slice = crate::live_collections::Stream<T, Tick<L::DropConsistency>, Bounded, O, R>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let _ = self.nondet;
        let out = self.collection.batch(
            tick,
            nondet!(/** justified by the guard stored in this style wrapper */),
        );
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, T, L: Location<'a>, B: SingletonBound> Slicable<'a, L::DropConsistency>
    for Default<crate::live_collections::Singleton<T, L, B>>
{
    type Slice = crate::live_collections::Singleton<T, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let _ = self.nondet;
        let out = self.collection.snapshot(
            tick,
            nondet!(/** justified by the guard stored in this style wrapper */),
        );
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, T, L: Location<'a>, B: OptionalBound> Slicable<'a, L::DropConsistency>
    for Default<crate::live_collections::Optional<T, L, B>>
{
    type Slice = crate::live_collections::Optional<T, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.snapshot(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>, B: Boundedness, O: Ordering, R: Retries>
    Slicable<'a, L::DropConsistency>
    for Default<crate::live_collections::KeyedStream<K, V, L, B, O, R>>
{
    type Slice =
        crate::live_collections::KeyedStream<K, V, Tick<L::DropConsistency>, Bounded, O, R>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let _ = self.nondet;
        let out = self.collection.batch(
            tick,
            nondet!(/** justified by the guard stored in this style wrapper */),
        );
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>, B: KeyedSingletonSnapshotBound<ValueBound = Unbounded>>
    Slicable<'a, L::DropConsistency>
    for Default<crate::live_collections::KeyedSingleton<K, V, L, B>>
{
    type Slice = crate::live_collections::KeyedSingleton<K, V, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let _ = self.nondet;
        let out = self.collection.snapshot(
            tick,
            nondet!(/** justified by the guard stored in this style wrapper */),
        );
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>> Slicable<'a, L::DropConsistency>
    for Default<crate::live_collections::KeyedSingleton<K, V, L, BoundedValue>>
{
    type Slice = crate::live_collections::KeyedSingleton<K, V, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let _ = self.nondet;
        let out = self.collection.batch(
            tick,
            nondet!(/** justified by the guard stored in this style wrapper */),
        );
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

// ============================================================================
// Batch style Slicable implementations (stream-like collections)
//
// All of these drop consistency because they are performing non-deterministic
// batching.
// ============================================================================

impl<'a, T, L: Location<'a>, B: Boundedness, O: Ordering, R: Retries>
    Slicable<'a, L::DropConsistency>
    for Batch<
        crate::live_collections::Stream<T, L, B, O, R>,
        Option<crate::sim_hooks::BatchHook<T, O, R, L::SimHookScope>>,
    >
{
    type Slice = crate::live_collections::Stream<T, Tick<L::DropConsistency>, Bounded, O, R>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.batch(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>, B: Boundedness, O: Ordering, R: Retries>
    Slicable<'a, L::DropConsistency>
    for Batch<
        crate::live_collections::KeyedStream<K, V, L, B, O, R>,
        Option<crate::sim_hooks::KeyedBatchHook<K, V, O, R, L::SimHookScope>>,
    >
{
    type Slice =
        crate::live_collections::KeyedStream<K, V, Tick<L::DropConsistency>, Bounded, O, R>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.batch(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>, B: KeyedSingletonBound<ValueBound = Bounded>>
    Slicable<'a, L::DropConsistency>
    for Batch<
        crate::live_collections::KeyedSingleton<K, V, L, B>,
        Option<crate::sim_hooks::KeyedBatchHook<K, V, TotalOrder, ExactlyOnce, L::SimHookScope>>,
    >
{
    type Slice = crate::live_collections::KeyedSingleton<K, V, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.batch(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

// ============================================================================
// Snapshot style Slicable implementations (singleton-like collections)
//
// All of these drop consistency because they are performing non-deterministic
// snapshotting.
// ============================================================================

impl<'a, T, L: Location<'a>, B: SingletonBound> Slicable<'a, L::DropConsistency>
    for Snapshot<
        crate::live_collections::Singleton<T, L, B>,
        Option<crate::sim_hooks::SnapshotHook<T, L::SimHookScope>>,
    >
{
    type Slice = crate::live_collections::Singleton<T, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.snapshot(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, T, L: Location<'a>, B: OptionalBound> Slicable<'a, L::DropConsistency>
    for Snapshot<crate::live_collections::Optional<T, L, B>>
{
    type Slice = crate::live_collections::Optional<T, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.snapshot(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>, B: KeyedSingletonSnapshotBound> Slicable<'a, L::DropConsistency>
    for Snapshot<
        crate::live_collections::KeyedSingleton<K, V, L, B>,
        Option<crate::sim_hooks::KeyedSnapshotHook<K, V, L::SimHookScope>>,
    >
{
    type Slice = crate::live_collections::KeyedSingleton<K, V, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }
    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.snapshot(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

// ============================================================================
// Atomic style Slicable implementations
// ============================================================================

impl<'a, T, L: Location<'a>, B: Boundedness, O: Ordering, R: Retries>
    Slicable<'a, L::DropConsistency>
    for Atomic<crate::live_collections::Stream<T, crate::location::Atomic<L>, B, O, R>>
{
    type Slice = crate::live_collections::Stream<T, Tick<L::DropConsistency>, Bounded, O, R>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;
    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().tick.l.drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let _ = self.nondet;
        let out = self.collection.batch_atomic(
            tick,
            nondet!(/** justified by the guard stored in this style wrapper */),
        );
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, T, L: Location<'a>, B: SingletonBound> Slicable<'a, L::DropConsistency>
    for Atomic<crate::live_collections::Singleton<T, crate::location::Atomic<L>, B>>
{
    type Slice = crate::live_collections::Singleton<T, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;
    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().tick.l.drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let _ = self.nondet;
        let out = self.collection.snapshot_atomic(
            tick,
            nondet!(/** justified by the guard stored in this style wrapper */),
        );
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, T, L: Location<'a>, B: OptionalBound> Slicable<'a, L::DropConsistency>
    for Atomic<crate::live_collections::Optional<T, crate::location::Atomic<L>, B>>
{
    type Slice = crate::live_collections::Optional<T, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;
    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().tick.l.drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.snapshot_atomic(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>, B: Boundedness, O: Ordering, R: Retries>
    Slicable<'a, L::DropConsistency>
    for Atomic<crate::live_collections::KeyedStream<K, V, crate::location::Atomic<L>, B, O, R>>
{
    type Slice =
        crate::live_collections::KeyedStream<K, V, Tick<L::DropConsistency>, Bounded, O, R>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;
    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().tick.l.drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.batch_atomic(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>, B: KeyedSingletonSnapshotBound<ValueBound = Unbounded>>
    Slicable<'a, L::DropConsistency>
    for Atomic<crate::live_collections::KeyedSingleton<K, V, crate::location::Atomic<L>, B>>
{
    type Slice = crate::live_collections::KeyedSingleton<K, V, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;
    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().tick.l.drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.snapshot_atomic(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

impl<'a, K, V, L: Location<'a>> Slicable<'a, L::DropConsistency>
    for Atomic<
        crate::live_collections::KeyedSingleton<K, V, crate::location::Atomic<L>, BoundedValue>,
    >
{
    type Slice = crate::live_collections::KeyedSingleton<K, V, Tick<L::DropConsistency>, Bounded>;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;
    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().tick.l.drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let out = self.collection.batch_atomic(tick, self.nondet);
        out.ir_node.borrow_mut().op_metadata_mut().backtrace = backtrace;
        out
    }
}

// ============================================================================
// Reference style Slicable implementations (bounded singleton-like collections)
//
// These drop consistency so that they compose with the other styles in the same `sliced!`
// block, but they introduce no non-determinism: a bounded collection has a single immutable
// value, which every slice observes. The value is wrapped in an `Arc` before entering the tick
// so that windowing it into each tick only clones the pointer, not the value. The snapshot is
// marked `sim_deterministic_snapshot` so that the simulator releases the value into every tick
// without exploring any choices.
// ============================================================================

impl<'a, T, L: Location<'a>, B: SingletonBound + IsBounded> Slicable<'a, L::DropConsistency>
    for Reference<crate::live_collections::Singleton<T, L, B>>
{
    type Slice = ReferenceSlice<
        crate::live_collections::Singleton<Arc<T>, Tick<L::DropConsistency>, Bounded>,
    >;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let shared: crate::live_collections::Singleton<Arc<T>, L, Bounded> = self
            .collection
            .make_bounded()
            .map(q!(|v| std::sync::Arc::new(v)));
        let out = shared.clone_into_tick(tick);
        {
            let mut node = out.ir_node.borrow_mut();
            let op_meta = node.op_metadata_mut();
            op_meta.backtrace = backtrace;
            op_meta.sim_deterministic_snapshot = true;
        }
        ReferenceSlice { collection: out }
    }
}

impl<'a, T, L: Location<'a>, B: OptionalBound + IsBounded> Slicable<'a, L::DropConsistency>
    for Reference<crate::live_collections::Optional<T, L, B>>
{
    type Slice = ReferenceSlice<
        crate::live_collections::Optional<Arc<T>, Tick<L::DropConsistency>, Bounded>,
    >;
    type Backtrace = crate::compile::ir::backtrace::Backtrace;

    fn get_location(&self) -> L::DropConsistency {
        self.collection.location().drop_consistency()
    }

    fn slice(self, tick: &Tick<L::DropConsistency>, backtrace: Self::Backtrace) -> Self::Slice {
        let shared: crate::live_collections::Optional<Arc<T>, L::DropConsistency, Bounded> = self
            .collection
            .make_bounded()
            .weaken_consistency()
            .map(q!(|v| std::sync::Arc::new(v)));
        let out = shared.clone_into_tick(tick);
        {
            let mut node = out.ir_node.borrow_mut();
            let op_meta = node.op_metadata_mut();
            op_meta.backtrace = backtrace;
            op_meta.sim_deterministic_snapshot = true;
        }
        ReferenceSlice { collection: out }
    }
}
