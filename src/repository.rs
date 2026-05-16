use std::collections::HashMap;

use portage_atom::gentoo_interner::{DefaultInterner, Interned};
use portage_atom::{Cpn, Cpv};

/// Default state for an IUSE flag, from the `+`/`-` prefix in IUSE.
///
/// See [PMS 7.2](https://projects.gentoo.org/pms/9/pms.html#mandatory-ebuilddefined-variables).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IUseDefault {
    /// `+flag` — enabled by default.
    Enabled,
    /// `-flag` — disabled by default.
    Disabled,
}

/// Metadata for a single ebuild version, including its dependency trees.
#[derive(Clone)]
pub struct PackageVersions {
    /// Slot name for this version.
    pub slot: Option<Interned<DefaultInterner>>,
    /// Subslot name for this version.
    pub subslot: Option<Interned<DefaultInterner>>,
    /// Repository this version comes from.
    pub repo: Option<Interned<DefaultInterner>>,
    /// IUSE flags for this version (USE flags the package defines).
    pub iuse: Vec<Interned<DefaultInterner>>,
    /// IUSE default states — flags prefixed with `+` in IUSE default to enabled.
    pub iuse_defaults: HashMap<Interned<DefaultInterner>, IUseDefault>,
    /// Dependency trees by class.
    pub deps: PackageDeps,
}

/// Structured dependency trees separated by PMS class.
#[derive(Clone)]
pub struct PackageDeps {
    /// DEPEND — build-time dependencies.
    pub depend: Vec<portage_atom::DepEntry>,
    /// RDEPEND — runtime dependencies.
    pub rdepend: Vec<portage_atom::DepEntry>,
    /// BDEPEND — build-host dependencies (EAPI 7+).
    pub bdepend: Vec<portage_atom::DepEntry>,
    /// PDEPEND — post-merge dependencies.
    pub pdepend: Vec<portage_atom::DepEntry>,
    /// IDEPEND — install-time dependencies (EAPI 8+).
    pub idepend: Vec<portage_atom::DepEntry>,
}

/// Trait for a package repository that the solver can query.
///
/// Implementations provide package metadata sourced from ebuild caches,
/// as described in [PMS 7](https://projects.gentoo.org/pms/9/pms.html#mandatory-ebuilddefined-variables).
pub trait PackageRepository {
    /// Return all packages in the repository.
    fn all_packages(&self) -> Vec<Cpn>;

    /// Return all versions for a given CPN, with their metadata.
    fn versions_for(&self, cpn: &Cpn) -> Vec<(Cpv, PackageVersions)>;
}

/// A simple in-memory repository for testing.
#[derive(Clone)]
pub struct InMemoryRepository {
    packages: HashMap<Cpn, Vec<(Cpv, PackageVersions)>>,
}

impl Default for InMemoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRepository {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn add_version(
        &mut self,
        cpv: Cpv,
        slot: Option<Interned<DefaultInterner>>,
        subslot: Option<Interned<DefaultInterner>>,
        deps: PackageDeps,
    ) {
        self.add_version_full(cpv, slot, subslot, None, vec![], deps);
    }

    pub fn add_version_with_iuse(
        &mut self,
        cpv: Cpv,
        slot: Option<Interned<DefaultInterner>>,
        subslot: Option<Interned<DefaultInterner>>,
        iuse: Vec<Interned<DefaultInterner>>,
        deps: PackageDeps,
    ) {
        self.add_version_full(cpv, slot, subslot, None, iuse, deps);
    }

    pub fn add_version_full(
        &mut self,
        cpv: Cpv,
        slot: Option<Interned<DefaultInterner>>,
        subslot: Option<Interned<DefaultInterner>>,
        repo: Option<Interned<DefaultInterner>>,
        iuse: Vec<Interned<DefaultInterner>>,
        deps: PackageDeps,
    ) {
        let cpn = Cpn::parse(&format!("{}/{}", cpv.cpn.category, cpv.cpn.package)).unwrap();
        self.packages.entry(cpn).or_default().push((
            cpv,
            PackageVersions {
                slot,
                subslot,
                repo,
                iuse,
                iuse_defaults: HashMap::new(),
                deps,
            },
        ));
    }
}

impl PackageRepository for InMemoryRepository {
    fn all_packages(&self) -> Vec<Cpn> {
        self.packages.keys().cloned().collect()
    }

    fn versions_for(&self, cpn: &Cpn) -> Vec<(Cpv, PackageVersions)> {
        self.packages
            .get(cpn)
            .map(|v| {
                v.iter()
                    .map(|(cpv, meta)| {
                        (
                            cpv.clone(),
                            PackageVersions {
                                slot: meta.slot,
                                subslot: meta.subslot,
                                repo: meta.repo,
                                iuse: meta.iuse.clone(),
                                iuse_defaults: meta.iuse_defaults.clone(),
                                deps: PackageDeps {
                                    depend: meta.deps.depend.clone(),
                                    rdepend: meta.deps.rdepend.clone(),
                                    bdepend: meta.deps.bdepend.clone(),
                                    pdepend: meta.deps.pdepend.clone(),
                                    idepend: meta.deps.idepend.clone(),
                                },
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_repo() {
        let mut repo = InMemoryRepository::new();
        let cpv = Cpv::parse("dev-libs/openssl-3.0.0").unwrap();
        let cpn = Cpn::parse("dev-libs/openssl").unwrap();
        repo.add_version(
            cpv,
            Some(Interned::intern("0")),
            None,
            PackageDeps {
                depend: vec![],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        assert_eq!(repo.all_packages().len(), 1);
        assert_eq!(repo.versions_for(&cpn).len(), 1);
    }
}
