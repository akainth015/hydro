# Codebase Information

## Project Identity

- **Name:** Hydro
- **Repository:** [github.com/hydro-project/hydro](https://github.com/hydro-project/hydro)
- **Website:** [hydro.run](https://hydro.run)
- **License:** Apache-2.0
- **Edition:** Rust 2024
- **Pinned Toolchain:** Rust 1.93.1 (stable), nightly for `rustfmt` only

## Purpose

Hydro is a high-level distributed programming framework for Rust. It helps developers write scalable distributed services that are **correct by construction** — the type system enforces distributed safety properties (ordering, exactly-once delivery, boundedness) at compile time, analogous to how Rust enforces memory safety.

Under the covers, Hydro compiles to the **Dataflow Intermediate Representation (DFIR)**, a compiler and low-level runtime for stream processing that enables automatic vectorization and efficient scheduling.

## Technology Stack

| Layer | Technology |
|---|---|
| Language | Rust 2024 edition |
| Build System | Cargo workspace (22 crates) |
| Test Runner | cargo-nextest |
| Snapshot Testing | insta + trybuild |
| Formatting | nightly rustfmt |
| Linting | clippy (custom rules banning nondeterministic iteration) |
| CI | GitHub Actions (sccache, matrix: ubuntu/windows/macos × stable/nightly) |
| Release | cargo-smart-release (automated changelogs, crates.io publishing) |
| Documentation | Docusaurus (website), rustdoc (API docs) |
| WASM | wasm-pack + wasm-bindgen (website playground) |
| Distributed Testing | Maelstrom v0.2.4 (Jepsen-style) |
| Fuzzing | bolero + libfuzzer (via `cargo-sim` script) |
| Staged Programming | stageleft 0.13.4 (external crate) |

## Workspace Structure

```
hydro/
├── Cargo.toml              # Workspace root
├── rust-toolchain.toml     # Pinned Rust 1.93.1
├── clippy.toml             # Custom lint rules (ban nondeterministic iteration)
├── rustfmt.toml            # Nightly formatting config
├── .cargo/config.toml      # Linker, env vars, tracing levels
├── .config/nextest.toml    # Test groups and serialization constraints
├── precheck.bash           # Local CI simulation
├── cargo-sim               # Fuzzing harness
│
├── dfir_lang/              # DFIR compiler (parse → graph → codegen)
├── dfir_macro/             # DFIR proc macros (dfir_syntax!, DemuxEnum)
├── dfir_rs/                # DFIR runtime (scheduler, handoffs, context)
├── dfir_pipes/             # Pull/Push stream combinators (#![no_std])
│
├── hydro_lang/             # Core Hydro framework (Stream, Location, Deploy)
├── hydro_std/              # Distributed patterns (quorum, request-response)
├── hydro_test/             # Integration tests and examples
├── hydro_test_embedded/    # Embedded test utilities
├── hydro_test_template/    # Test template
│
├── hydro_deploy/
│   ├── core/               # Deployment framework (localhost, GCP, Azure, AWS)
│   └── hydro_deploy_integration/  # Runtime-side deploy protocol
│
├── lattices/               # Lattice types for CRDTs (Merge, LatticeOrd)
├── lattices_macro/         # #[derive(Lattice)]
├── variadics/              # Variadic generics via tuple lists
├── variadics_macro/        # Variadic proc macros
│
├── sinktools/              # Sink adaptors (extends futures::Sink)
├── copy_span/              # Proc-macro span copying
├── include_mdtests/        # Markdown-as-doctests proc macro
├── multiplatform_test/     # Cross-platform test attribute
├── hydro_build_utils/      # Build-time nightly detection + snapshot helpers
├── example_test/           # Example test crate
│
├── benches/                # Microbenchmarks
├── website_playground/     # WASM playground for hydro.run
├── template/               # cargo-generate project templates
├── design_docs/            # Historical design documents
├── docs/                   # Docusaurus website source
├── cdk/                    # CDK infrastructure
├── infrastructure_cdk/     # Infrastructure CDK
└── scripts/                # Build/release/validation scripts
```

## Key Configuration Files

| File | Purpose |
|---|---|
| `Cargo.toml` | Workspace members, shared deps, build profiles, workspace lints |
| `rust-toolchain.toml` | Pins Rust 1.93.1, includes wasm32 + musl targets |
| `clippy.toml` | Bans nondeterministic iteration on HashMap/HashSet/SparseSecondaryMap |
| `rustfmt.toml` | Nightly features: doc comment formatting, import grouping |
| `.cargo/config.toml` | rust-lld linker, RUST_LOG levels, DFIR env vars |
| `.config/nextest.toml` | Serial test groups for trybuild and integration tests |
| `precheck.bash` | Local CI: fmt → clippy → nextest → doctest → wasm → rustdoc |

## Feature Flags

| Feature | Scope | Purpose |
|---|---|---|
| `build` | hydro_lang | Enables DFIR compilation pipeline |
| `deploy` | hydro_lang | Full deployment support |
| `sim` | hydro_lang | Deterministic simulator (bolero fuzzing) |
| `viz` | hydro_lang | Graph visualization (Mermaid, Graphviz) |
| `docker_deploy` | hydro_lang | Docker container deployment |
| `ecs_deploy` | hydro_lang | AWS ECS deployment |
| `maelstrom` | hydro_lang | Maelstrom distributed testing |
| `embedded_runtime` | hydro_lang | In-process embedded deployment |
| `runtime_support` | hydro_lang | Re-exports for generated code |
| `runtime_measure` | hydro_lang | CPU measurement via procfs |
| `std` | sinktools, variadics | Standard library support |
| `variadics` | sinktools | Variadic demux sinks |
