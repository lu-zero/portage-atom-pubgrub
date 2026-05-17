//! Bridge between [portage-atom](https://crates.io/crates/portage-atom) and the
//! [PubGrub](https://crates.io/crates/pubgrub) dependency solver.
//!
//! Maps PMS package atoms to PubGrub's `Package`, `Version`, and `VersionSet`
//! traits, and provides a `DependencyProvider` implementation backed by a
//! package repository.
//!
//! # USE Flag Handling
//!
//! USE conditionals in dependency strings are handled via a hybrid strategy:
//!
//! - **User-decided** flags (`enabled` / `disabled`) are eagerly evaluated at
//!   registration time — the solver never sees those branches.
//! - **Solver-decided** flags are encoded as virtual packages with two versions
//!   (0 = OFF, 1 = ON). PubGrub's one-version-per-package constraint provides
//!   mutual exclusion for free.

mod convert;
mod error;
mod graph;
mod package;
mod provider;
mod repository;
mod use_config;
mod validate;
mod version_set;

pub use error::{Error, Result};
pub use graph::{DepClass, DepEdge};
pub use package::PortagePackage;
pub use provider::{InstalledPackage, InstalledPolicy, PortageDependencyProvider};
pub use repository::{
    IUseDefault, InMemoryRepository, PackageDeps, PackageRepository, PackageVersions,
};
pub use use_config::{UseConfig, UseFlagState};
pub use validate::SlotOperatorBinding;
pub use version_set::PortageVersionSet;
