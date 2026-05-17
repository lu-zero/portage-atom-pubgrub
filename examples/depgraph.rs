//! Example: resolve dependencies and inspect the dependency graph,
//! installation order, and slot-operator bindings.
//!
//! Builds a small repo with slotted packages and slot-operator deps,
//! resolves, then prints:
//!
//! 1. The dependency graph with labeled edges (DEPEND, RDEPEND, etc.)
//! 2. Topological install order
//! 3. Slot-operator bindings (:=) from the solution

use portage_atom::gentoo_interner::Interned;
use portage_atom::{Cpn, Cpv, Dep, DepEntry};
use portage_atom_pubgrub::{
    DepClass, InMemoryRepository, PackageDeps, PortageDependencyProvider, PortagePackage,
    PortageVersionSet, UseConfig,
};

fn empty_deps() -> PackageDeps {
    PackageDeps {
        depend: vec![],
        rdepend: vec![],
        bdepend: vec![],
        pdepend: vec![],
        idepend: vec![],
    }
}

fn dep_entries(deps: Vec<DepEntry>) -> PackageDeps {
    PackageDeps {
        depend: deps,
        rdepend: vec![],
        bdepend: vec![],
        pdepend: vec![],
        idepend: vec![],
    }
}

fn main() {
    let mut repo = InMemoryRepository::new();

    // sys-libs/glibc: slot 2.38, no deps
    repo.add_version(
        Cpv::parse("sys-libs/glibc-2.38-r12").unwrap(),
        Some(Interned::intern("2.38")),
        None,
        empty_deps(),
    );

    // sys-libs/zlib: slot 0
    repo.add_version(
        Cpv::parse("sys-libs/zlib-1.3.1").unwrap(),
        Some(Interned::intern("0")),
        None,
        empty_deps(),
    );

    // dev-libs/openssl: slot 3, depends on zlib at build time
    repo.add_version(
        Cpv::parse("dev-libs/openssl-3.2.1").unwrap(),
        Some(Interned::intern("3")),
        None,
        dep_entries(vec![DepEntry::Atom(
            Dep::parse(">=sys-libs/zlib-1.2").unwrap(),
        )]),
    );

    // net-misc/curl: slot 0, depends on openssl (RDEPEND) and zlib (BDEPEND)
    repo.add_version(
        Cpv::parse("net-misc/curl-8.7.1").unwrap(),
        Some(Interned::intern("0")),
        None,
        PackageDeps {
            depend: vec![DepEntry::Atom(
                Dep::parse(">=dev-libs/openssl-3.0").unwrap(),
            )],
            rdepend: vec![DepEntry::Atom(Dep::parse("dev-libs/openssl").unwrap())],
            bdepend: vec![DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2").unwrap())],
            pdepend: vec![],
            idepend: vec![],
        },
    );

    // app-misc/myapp: depends on curl
    repo.add_version(
        Cpv::parse("app-misc/myapp-1.0").unwrap(),
        None,
        None,
        dep_entries(vec![DepEntry::Atom(Dep::parse("net-misc/curl").unwrap())]),
    );

    let use_config = UseConfig::new();
    let mut provider = PortageDependencyProvider::new(repo, use_config);

    let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
    let solution = provider
        .resolve_targets(vec![(myapp, PortageVersionSet::any())])
        .expect("resolution should succeed");

    let mut pkgs: Vec<_> = solution.iter().collect();
    pkgs.sort_by_key(|(p, v)| format!("{}-{}", p.cpn, v));

    println!("Resolved {} packages:\n", pkgs.len());
    for (pkg, ver) in &pkgs {
        println!("  {}-{}", pkg, ver);
    }

    // -- Dependency graph --
    let edges = provider.dependency_graph(&solution);
    println!("\nDependency graph ({} edges):\n", edges.len());
    for edge in &edges {
        let class = match edge.class {
            DepClass::Depend => "DEPEND",
            DepClass::Rdepend => "RDEPEND",
            DepClass::Bdepend => "BDEPEND",
            DepClass::Pdepend => "PDEPEND",
            DepClass::Idepend => "IDEPEND",
        };
        println!(
            "  {}-{} --[{}]--> {}-{}",
            edge.from.0, edge.from.1, class, edge.to.0, edge.to.1,
        );
    }

    // -- Install order --
    let order = provider.install_order(&solution);
    println!("\nInstall order:\n");
    for (i, (pkg, ver)) in order.iter().enumerate() {
        println!("  {:>2}. {}-{}", i + 1, pkg.cpn, ver);
    }

    // -- Slot-operator bindings --
    let bindings = provider.slot_operator_bindings(&solution);
    println!("\nSlot-operator bindings ({}):\n", bindings.len());
    if bindings.is_empty() {
        println!("  (none)");
    } else {
        for b in &bindings {
            let slot: &str = match &b.slot {
                Some(s) => s.as_str(),
                None => "?",
            };
            println!(
                "  {} --> {} (bound to slot {})",
                b.parent, b.target_cpn, slot,
            );
        }
    }
}
