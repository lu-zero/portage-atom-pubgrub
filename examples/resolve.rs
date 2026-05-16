//! Example: resolve a set of real-world Portage dependencies using PubGrub.
//!
//! Models a small slice of the Gentoo tree with real package atoms, including
//! transitive deps, `|| ()` any-of (openssl vs libressl), versioned constraints,
//! and USE-conditional dependencies.
//!
//! Since PubGrub resolves from a single root package+version, we create a
//! `virtual/root` package whose dependencies are the packages we want installed.
//!
//! Runs the solver three times with different USE flag configurations.

use portage_atom::gentoo_interner::Interned;
use portage_atom::{Cpn, Cpv, Dep, DepEntry, Version};
use portage_atom_pubgrub::{
    InMemoryRepository, PackageDeps, PortageDependencyProvider, PortagePackage, PortageVersionSet,
    UseConfig,
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

fn build_repo() -> InMemoryRepository {
    let mut repo = InMemoryRepository::new();

    // -- sys-libs/zlib: two versions, single slot --
    repo.add_version(
        Cpv::parse("sys-libs/zlib-1.2.13").unwrap(),
        None,
        None,
        empty_deps(),
    );
    repo.add_version(
        Cpv::parse("sys-libs/zlib-1.3.1").unwrap(),
        None,
        None,
        empty_deps(),
    );

    // -- app-arch/bzip2 --
    repo.add_version(
        Cpv::parse("app-arch/bzip2-1.0.8-r4").unwrap(),
        None,
        None,
        empty_deps(),
    );

    // -- dev-libs/expat --
    repo.add_version(
        Cpv::parse("dev-libs/expat-2.6.2").unwrap(),
        None,
        None,
        empty_deps(),
    );

    // -- dev-libs/openssl: depends on zlib, weak-blocks libressl --
    repo.add_version(
        Cpv::parse("dev-libs/openssl-3.1.7").unwrap(),
        None,
        None,
        dep_entries(vec![
            DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2.13").unwrap()),
            DepEntry::Atom(Dep::parse("!dev-libs/libressl").unwrap()),
        ]),
    );
    repo.add_version(
        Cpv::parse("dev-libs/openssl-3.2.1").unwrap(),
        None,
        None,
        dep_entries(vec![
            DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2.13").unwrap()),
            DepEntry::Atom(Dep::parse("!dev-libs/libressl").unwrap()),
        ]),
    );

    // -- dev-libs/libressl: alternative TLS, needs zlib, strong-blocks openssl --
    repo.add_version(
        Cpv::parse("dev-libs/libressl-3.9.2").unwrap(),
        None,
        None,
        dep_entries(vec![
            DepEntry::Atom(Dep::parse("sys-libs/zlib").unwrap()),
            DepEntry::Atom(Dep::parse("!!dev-libs/openssl").unwrap()),
        ]),
    );

    // -- media-libs/libpng: needs >=zlib-1.2.13 --
    repo.add_version(
        Cpv::parse("media-libs/libpng-1.6.43").unwrap(),
        None,
        None,
        dep_entries(vec![DepEntry::Atom(
            Dep::parse(">=sys-libs/zlib-1.2.13").unwrap(),
        )]),
    );

    // -- dev-lang/python: depends on zlib, bzip2; xml USE flag pulls in expat --
    let python_deps = dep_entries(vec![
        DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2.13").unwrap()),
        DepEntry::Atom(Dep::parse("app-arch/bzip2").unwrap()),
        DepEntry::UseConditional {
            flag: Interned::intern("xml"),
            negate: false,
            children: vec![DepEntry::Atom(Dep::parse("dev-libs/expat").unwrap())],
        },
    ]);
    repo.add_version(
        Cpv::parse("dev-lang/python-3.11.9").unwrap(),
        None,
        None,
        python_deps.clone(),
    );
    repo.add_version(
        Cpv::parse("dev-lang/python-3.12.4").unwrap(),
        None,
        None,
        python_deps,
    );

    // -- dev-python/certifi: needs python --
    repo.add_version(
        Cpv::parse("dev-python/certifi-2024.2.2").unwrap(),
        None,
        None,
        dep_entries(vec![DepEntry::Atom(Dep::parse("dev-lang/python").unwrap())]),
    );

    // -- net-misc/curl --
    //   always: >=sys-libs/zlib-1.2.13
    //   always: || ( dev-libs/openssl dev-libs/libressl )
    //   ssl?  : dev-python/certifi
    repo.add_version(
        Cpv::parse("net-misc/curl-8.7.1").unwrap(),
        None,
        None,
        dep_entries(vec![
            DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2.13").unwrap()),
            DepEntry::AnyOf(vec![
                DepEntry::Atom(Dep::parse("dev-libs/openssl").unwrap()),
                DepEntry::Atom(Dep::parse("dev-libs/libressl").unwrap()),
            ]),
            DepEntry::UseConditional {
                flag: Interned::intern("ssl"),
                negate: false,
                children: vec![DepEntry::Atom(Dep::parse("dev-python/certifi").unwrap())],
            },
        ]),
    );

    // -- app-portage/gentoolkit: needs python:3.12, certifi --
    repo.add_version(
        Cpv::parse("app-portage/gentoolkit-0.6.3").unwrap(),
        None,
        None,
        dep_entries(vec![
            DepEntry::Atom(Dep::parse("dev-lang/python").unwrap()),
            DepEntry::Atom(Dep::parse("dev-python/certifi").unwrap()),
        ]),
    );

    // -- www-client/firefox: python, libpng, >=openssl-3.2 --
    repo.add_version(
        Cpv::parse("www-client/firefox-125.0.3").unwrap(),
        None,
        None,
        dep_entries(vec![
            DepEntry::Atom(Dep::parse("dev-lang/python").unwrap()),
            DepEntry::Atom(Dep::parse("media-libs/libpng").unwrap()),
            DepEntry::Atom(Dep::parse(">=dev-libs/openssl-3.2.0").unwrap()),
        ]),
    );

    // -- dev-python/sphinx: needs python --
    repo.add_version(
        Cpv::parse("dev-python/sphinx-7.0.0").unwrap(),
        None,
        None,
        dep_entries(vec![DepEntry::Atom(Dep::parse("dev-lang/python").unwrap())]),
    );

    // -- app-doc/pygdoc: || ( ( python:3.12 sphinx ) ( python:3.11 sphinx ) ) --
    repo.add_version(
        Cpv::parse("app-doc/pygdoc-1.0").unwrap(),
        None,
        None,
        dep_entries(vec![DepEntry::AnyOf(vec![
            DepEntry::AllOf(vec![
                DepEntry::Atom(Dep::parse("dev-lang/python").unwrap()),
                DepEntry::Atom(Dep::parse("dev-python/sphinx").unwrap()),
            ]),
            DepEntry::AllOf(vec![
                DepEntry::Atom(Dep::parse("dev-lang/python").unwrap()),
                DepEntry::Atom(Dep::parse("dev-python/sphinx").unwrap()),
            ]),
        ])]),
    );

    repo
}

fn print_deps(indent: usize, entries: &[DepEntry]) {
    let pad = " ".repeat(indent);
    for entry in entries {
        match entry {
            DepEntry::Atom(dep) => println!("{pad}{dep}"),
            DepEntry::AnyOf(children) => {
                println!("{pad}|| (");
                print_deps(indent + 4, children);
                println!("{pad})");
            }
            DepEntry::UseConditional {
                flag,
                negate,
                children,
            } => {
                let prefix = if *negate { "!" } else { "" };
                println!("{pad}{prefix}{flag}? (");
                print_deps(indent + 4, children);
                println!("{pad})");
            }
            DepEntry::ExactlyOneOf(children) => {
                println!("{pad}^^ (");
                print_deps(indent + 4, children);
                println!("{pad})");
            }
            DepEntry::AtMostOneOf(children) => {
                println!("{pad}?? (");
                print_deps(indent + 4, children);
                println!("{pad})");
            }
            DepEntry::AllOf(children) => {
                println!("{pad}(");
                print_deps(indent + 4, children);
                println!("{pad})");
            }
        }
    }
}

fn make_root_deps(atoms: &[&str]) -> Vec<(PortagePackage, PortageVersionSet)> {
    atoms
        .iter()
        .map(|s| {
            let cpn = Cpn::parse(s).unwrap();
            (PortagePackage::unslotted(cpn), PortageVersionSet::any())
        })
        .collect()
}

fn solve_and_print(repo: &InMemoryRepository, use_config: UseConfig, root_atoms: &[&str]) {
    let mut provider = PortageDependencyProvider::new(repo.clone(), use_config);

    let root = PortagePackage::unslotted(Cpn::parse("virtual/root").unwrap());
    let root_version = Version::parse("1").unwrap();
    provider.add_root(
        root.clone(),
        root_version.clone(),
        make_root_deps(root_atoms),
    );

    match pubgrub::resolve(&provider, root, root_version) {
        Ok(solution) => {
            let mut pkgs: Vec<_> = solution
                .iter()
                .filter(|(pkg, _)| pkg.cpn.category != "virtual")
                .map(|(pkg, ver)| format!("{}/{}-{}", pkg.cpn.category, pkg.cpn.package, ver))
                .collect();
            pkgs.sort();
            println!("  Solution ({} packages):", pkgs.len());
            for p in &pkgs {
                println!("    {p}");
            }

            let blockers = provider.check_blockers(&solution);
            if !blockers.is_empty() {
                println!("  Blocker conflicts:");
                for b in &blockers {
                    println!("    {b}");
                }
            }
        }
        Err(e) => {
            eprintln!("  Resolution failed: {e:?}");
        }
    }
}

fn main() {
    let repo = build_repo();

    println!("Repository:\n");
    println!("  sys-libs/zlib          1.2.13, 1.3.1");
    println!("  app-arch/bzip2         1.0.8-r4");
    println!("  dev-libs/expat         2.6.2");
    println!("  dev-libs/openssl       3.1.7, 3.2.1");
    println!("  dev-libs/libressl      3.9.2");
    println!("  media-libs/libpng      1.6.43");
    println!("  dev-lang/python        3.11.9, 3.12.4");
    println!("  dev-python/certifi     2024.2.2");
    println!("  net-misc/curl          8.7.1");
    println!("  app-portage/gentoolkit 0.6.3");
    println!("  www-client/firefox     125.0.3");
    println!("  dev-python/sphinx      7.0.0");
    println!("  app-doc/pygdoc         1.0");

    println!("\nDeclared dependencies:\n");
    let pkgs_with_deps: Vec<(&str, Vec<DepEntry>)> = vec![
        (
            "dev-libs/openssl-3.2.1",
            vec![
                DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2.13").unwrap()),
                DepEntry::Atom(Dep::parse("!dev-libs/libressl").unwrap()),
            ],
        ),
        (
            "dev-libs/libressl-3.9.2",
            vec![
                DepEntry::Atom(Dep::parse("sys-libs/zlib").unwrap()),
                DepEntry::Atom(Dep::parse("!!dev-libs/openssl").unwrap()),
            ],
        ),
        (
            "net-misc/curl-8.7.1",
            vec![
                DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2.13").unwrap()),
                DepEntry::AnyOf(vec![
                    DepEntry::Atom(Dep::parse("dev-libs/openssl").unwrap()),
                    DepEntry::Atom(Dep::parse("dev-libs/libressl").unwrap()),
                ]),
                DepEntry::UseConditional {
                    flag: Interned::intern("ssl"),
                    negate: false,
                    children: vec![DepEntry::Atom(Dep::parse("dev-python/certifi").unwrap())],
                },
            ],
        ),
        (
            "dev-lang/python-3.{11,12}",
            vec![
                DepEntry::Atom(Dep::parse(">=sys-libs/zlib-1.2.13").unwrap()),
                DepEntry::Atom(Dep::parse("app-arch/bzip2").unwrap()),
                DepEntry::UseConditional {
                    flag: Interned::intern("xml"),
                    negate: false,
                    children: vec![DepEntry::Atom(Dep::parse("dev-libs/expat").unwrap())],
                },
            ],
        ),
    ];

    for (name, deps) in &pkgs_with_deps {
        println!("  {name}:");
        print_deps(4, deps);
    }

    let root_atoms = [
        "net-misc/curl",
        "app-portage/gentoolkit",
        "www-client/firefox",
        "app-doc/pygdoc",
    ];

    println!("\nRoot requirements:");
    for a in &root_atoms {
        println!("  {a}");
    }

    // -- Solve with USE="ssl xml" --
    let mut config_ssl_xml = UseConfig::new();
    config_ssl_xml.enable(Interned::intern("ssl"));
    config_ssl_xml.enable(Interned::intern("xml"));
    println!(
        "\n{}\nUSE=\"ssl xml\"  (eager)\n{}",
        "=".repeat(60),
        "=".repeat(60),
    );
    solve_and_print(&repo, config_ssl_xml, &root_atoms);

    // -- Solve with USE="xml -ssl" --
    let mut config_xml = UseConfig::new();
    config_xml.enable(Interned::intern("xml"));
    println!(
        "\n{}\nUSE=\"xml -ssl\"  (eager)\n{}",
        "=".repeat(60),
        "=".repeat(60),
    );
    solve_and_print(&repo, config_xml, &root_atoms);

    // -- Solve with USE="-ssl -xml" --
    println!(
        "\n{}\nUSE=\"-ssl -xml\"  (minimal)\n{}",
        "=".repeat(60),
        "=".repeat(60),
    );
    solve_and_print(&repo, UseConfig::new(), &root_atoms);
}
