//! Example: demonstrate dependency resolution **failure modes** with PubGrub.
//!
//! Each scenario builds a tiny repository, attempts to solve, and prints
//! the error produced by PubGrub's solver.

use portage_atom::{Cpn, Cpv, Dep, DepEntry};
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

fn add_pkg(repo: &mut InMemoryRepository, cpv: &str, deps: PackageDeps) {
    repo.add_version(Cpv::parse(cpv).unwrap(), None, None, deps);
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

fn try_solve(title: &str, repo: &InMemoryRepository, root_atoms: &[&str]) {
    println!("\n{}", "=".repeat(60));
    println!("{title}");
    println!("{}", "=".repeat(60));

    let use_config = UseConfig::new();
    let mut provider = PortageDependencyProvider::new(repo.clone(), use_config);

    match provider.resolve_targets(make_root_deps(root_atoms)) {
        Ok(_solution) => {
            println!("  Resolved successfully (unexpected for this example).");
        }
        Err(e) => {
            println!("  No solution: {e:?}");
        }
    }
}

fn main() {
    // -- 1. Missing dependency --
    {
        let mut repo = InMemoryRepository::new();
        add_pkg(
            &mut repo,
            "app-misc/hello-1.0",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse("dev-lib/nonexistent").unwrap(),
            )]),
        );
        try_solve(
            "1. Missing dependency — no candidates at all",
            &repo,
            &["app-misc/hello"],
        );
    }

    // -- 2. Version conflict --
    {
        let mut repo = InMemoryRepository::new();
        add_pkg(&mut repo, "dev-lib/foo-1.0", empty_deps());
        add_pkg(
            &mut repo,
            "app-misc/myapp-1.0",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse(">=dev-lib/foo-2.0").unwrap(),
            )]),
        );
        try_solve(
            "2. Version conflict — needs >=2.0, only 1.0 exists",
            &repo,
            &["app-misc/myapp"],
        );
    }

    // -- 3. Mutual blockers --
    // PubGrub does not model blockers as solver constraints.
    // Blockers are validated post-solve via check_blockers().
    {
        let mut repo = InMemoryRepository::new();
        add_pkg(
            &mut repo,
            "dev-libs/openssl-3.2.1",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse("!dev-libs/libressl").unwrap(),
            )]),
        );
        add_pkg(
            &mut repo,
            "dev-libs/libressl-3.9.2",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse("!!dev-libs/openssl").unwrap(),
            )]),
        );
        add_pkg(
            &mut repo,
            "app-misc/myapp-1.0",
            dep_entries(vec![
                DepEntry::Atom(Dep::parse("dev-libs/openssl").unwrap()),
                DepEntry::Atom(Dep::parse("dev-libs/libressl").unwrap()),
            ]),
        );
        println!("\n{}", "=".repeat(60));
        println!(
            "3. Mutual blockers — app requires both openssl and libressl\n   \
             (Post-solve blocker check)"
        );
        println!("{}", "=".repeat(60));

        let use_config = UseConfig::new();
        let mut provider = PortageDependencyProvider::new(repo.clone(), use_config);

        match provider.resolve_targets(make_root_deps(&["app-misc/myapp"])) {
            Ok(solution) => {
                let blockers = provider.check_blockers(&solution);
                if blockers.is_empty() {
                    println!("  Resolved with no blocker conflicts (unexpected).");
                } else {
                    println!("  Resolved but blockers detected post-solve:");
                    for b in &blockers {
                        println!("    {b}");
                    }
                }
            }
            Err(e) => {
                println!("  Error: {e:?}");
            }
        }
    }

    // -- 4. No version satisfies all constraints --
    {
        let mut repo = InMemoryRepository::new();
        add_pkg(&mut repo, "dev-lib/foo-1.0", empty_deps());
        add_pkg(&mut repo, "dev-lib/foo-2.0", empty_deps());
        add_pkg(&mut repo, "dev-lib/foo-3.0", empty_deps());
        add_pkg(
            &mut repo,
            "app-misc/app-a-1.0",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse(">=dev-lib/foo-3.0").unwrap(),
            )]),
        );
        add_pkg(
            &mut repo,
            "app-misc/app-b-1.0",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse("<dev-lib/foo-2.0").unwrap(),
            )]),
        );
        try_solve(
            "4. No single foo version satisfies both >=3.0 and <2.0",
            &repo,
            &["app-misc/app-a", "app-misc/app-b"],
        );
    }

    // -- 5. Diamond dependency conflict --
    {
        let mut repo = InMemoryRepository::new();
        add_pkg(&mut repo, "dev-lib/foo-1.0", empty_deps());
        add_pkg(&mut repo, "dev-lib/foo-2.0", empty_deps());
        // A needs =foo-1.0 (exact)
        add_pkg(
            &mut repo,
            "app-misc/left-1.0",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse("=dev-lib/foo-1.0").unwrap(),
            )]),
        );
        // B needs >=foo-2.0
        add_pkg(
            &mut repo,
            "app-misc/right-1.0",
            dep_entries(vec![DepEntry::Atom(
                Dep::parse(">=dev-lib/foo-2.0").unwrap(),
            )]),
        );
        try_solve(
            "5. Diamond conflict — left needs =foo-1.0, right needs >=foo-2.0",
            &repo,
            &["app-misc/left", "app-misc/right"],
        );
    }
}
