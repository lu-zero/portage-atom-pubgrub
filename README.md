# portage-atom-pubgrub

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![Build Status](https://github.com/lu-zero/portage-atom-pubgrub/workflows/CI/badge.svg)](https://github.com/lu-zero/portage-atom-pubgrub/actions?query=workflow:CI)
[![Crates.io](https://img.shields.io/crates/v/portage-atom-pubgrub.svg)](https://crates.io/crates/portage-atom-pubgrub)
[![docs.rs](https://docs.rs/portage-atom-pubgrub/badge.svg)](https://docs.rs/portage-atom-pubgrub)

A Rust library that bridges [portage-atom](https://crates.io/crates/portage-atom) types with the [PubGrub](https://pubgrub-rs.github.io/pubgrub/) dependency solver, implementing the [Package Manager Specification (PMS) 9](https://projects.gentoo.org/pms/9/pms.html).

> **Warning**: This codebase was largely AI-generated and has not been thoroughly
> audited. It may contain bugs, incomplete PMS coverage, or surprising edge-case
> behaviour. Use at your own risk and please report issues.

## Overview

`portage-atom-pubgrub` provides a `DependencyProvider` implementation for the
PubGrub version solver that understands Gentoo Portage dependency semantics:

- **`PortagePackage`** — a PubGrub `Package` backed by a `Cpn` + optional slot
- **`PortageVersionSet`** — a PubGrub `VersionSet` mapping PMS operators (`>=`,
  `~`, `=*`, etc.) to `Ranges<Version>`
- **`PortageDependencyProvider`** — a PubGrub `DependencyProvider` over a
  package repository, with support for:
  - All five PMS dependency classes (DEPEND, RDEPEND, BDEPEND, PDEPEND, IDEPEND)
  - OR groups, exactly-one-of, at-most-one-of modelled as virtual choice packages
  - Slot and subslot operators (`:=`, `:*`)
  - USE-conditional dependencies with hybrid evaluation (eager for user-decided,
    virtual packages for solver-decided flags)
  - USE-dep constraints (`[ssl]`, `[-debug]`, `[ssl?]`, `[ssl=]`)
  - Repository constraints (`::gentoo`)
  - Installed package tracking (favored / locked)
  - Blocker detection and post-solve validation
  - Dependency graph with labeled edges and topological install ordering
  - Filtering of dependencies referencing packages absent from the repository

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
portage-atom-pubgrub = "0.1"
```

## Usage

```rust
use portage_atom_pubgrub::{
    PackageRepository, PortageDependencyProvider, PortagePackage, PortageVersionSet, UseConfig,
};
use portage_atom::{Cpn, Version};
use pubgrub::resolve;

// Implement PackageRepository for your data source
let repo = MyRepository::new();
let use_config = UseConfig::new();
let mut provider = PortageDependencyProvider::new(repo, use_config);

// Set up a root package with the target dependencies
let root = PortagePackage::unslotted(Cpn::parse("virtual/root").unwrap());
let root_ver = Version::parse("1").unwrap();
provider.add_root(root.clone(), root_ver.clone(), vec![
    (target_package, target_version_set),
]);

let solution = resolve(&provider, root, root_ver);
```

See `examples/resolve.rs` and `examples/resolve_conflicts.rs` for complete examples.

## PMS Operators to Version Sets

| PMS Operator | Version Set |
|---|---|
| `>=V` | `Ranges::higher_than(V)` |
| `>V` | `Ranges::strictly_higher_than(V)` |
| `<=V` | `Ranges::lower_than(V)` |
| `<V` | `Ranges::strictly_lower_than(V)` |
| `=V` | `Ranges::singleton(V)` |
| `=V*` (glob) | `Ranges::between(V, next_after_glob(V))` |
| `~V` (approximate) | `Ranges::between(V_norev, V_rev_bumped)` |

## Post-Solve Validation

The provider exposes methods for checks that happen after resolution:

- **`check_blockers()`** — validates that no installed packages conflict with
  resolved blockers
- **`check_use_deps()`** — validates USE-dep constraints against the solution
- **`check_repo_constraints()`** — validates repository constraints
- **`dependency_graph()`** — returns labeled edges with dependency class and
  topological install order (PDEPEND edges deferred)

## Related Projects

- [portage-atom](https://crates.io/crates/portage-atom) — Portage package atom parser
- [portage-atom-resolvo](https://crates.io/crates/portage-atom-resolvo) — Bridge to the resolvo SAT solver
- [pubgrub](https://crates.io/crates/pubgrub) — PubGrub version solving algorithm
- [PMS 9](https://projects.gentoo.org/pms/9/pms.html) — Package Manager Specification

## License

[MIT](LICENSE-MIT)
