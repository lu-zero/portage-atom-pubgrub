use portage_atom::gentoo_interner::{DefaultInterner, Interned};
use portage_atom::{Dep, SlotOperator, UseDepKind, Version};
use pubgrub::VersionSet;

use crate::error::Error;
use crate::package::PortagePackage;
use crate::provider::PortageDependencyProvider;
use crate::use_config::{UseConfig, UseFlagState};

/// A resolved slot-operator binding.
///
/// After resolution, maps each `:=` / `:0=` dependency to the actual slot
/// chosen by the solver, for rebuild tracking.
///
/// See [PMS 8.3.3](https://projects.gentoo.org/pms/9/pms.html#slot_deps).
#[derive(Debug, Clone)]
pub struct SlotOperatorBinding {
    /// The package that declared the slot-operator dependency
    /// (e.g. `"app-misc/app-1.0"`).
    pub parent: String,
    /// The CPN of the target dependency (e.g. `"dev-libs/lib"`).
    pub target_cpn: String,
    /// The slot the solver assigned to the target (e.g. `Some("0")`).
    pub slot: Option<Interned<DefaultInterner>>,
    /// The slot name declared in the atom, if any (`:0=` → `Some("0")`,
    /// `:=` → `None`).
    pub declared_slot: Option<Interned<DefaultInterner>>,
    /// The operator string (`"="` for `:=` / `:0=`).
    pub operator: &'static str,
}

impl PortageDependencyProvider {
    /// Validate USE-dep constraints against a solution (post-solve check).
    ///
    /// For each package in the solution that declares USE-dep constraints,
    /// verify that the target package satisfies them according to
    /// [PMS 8.3.4](https://projects.gentoo.org/pms/9/pms.html#style-and-style-use-dependencies):
    ///
    /// - `[flag]` — target's flag must be enabled
    /// - `[-flag]` — target's flag must be disabled
    /// - `[flag?]` — if parent's flag is ON, target's flag must be ON
    /// - `[!flag?]` — if parent's flag is OFF, target's flag must be ON
    /// - `[flag=]` — target's flag must match parent's flag state
    /// - `[!flag=]` — target's flag must be opposite of parent's flag state
    ///
    /// Parent flag state is resolved from `use_config` (user-decided) or from
    /// the solution's virtual USE packages (solver-decided).
    pub fn check_use_deps(
        &self,
        solution: &pubgrub::SelectedDependencies<PortagePackage, Version>,
        use_config: &UseConfig,
    ) -> Vec<Error> {
        let mut errors = Vec::new();
        for (pkg, version) in solution.iter() {
            let Some(data) = self.packages.get(pkg) else {
                continue;
            };
            let Some(constraints) = data.use_deps.get(version) else {
                continue;
            };
            for constraint in constraints {
                let (target_pkg, target_vs) = &constraint.target;
                let target_entry = solution
                    .iter()
                    .find(|(p, v)| p.cpn == target_pkg.cpn && target_vs.contains(v));
                let Some((_, target_ver)) = target_entry else {
                    continue;
                };

                let target_iuse = self
                    .packages
                    .get(target_pkg)
                    .and_then(|d| d.iuse.get(target_ver))
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);

                for ud in &constraint.use_deps {
                    let target_flag_state = resolve_flag_state(target_iuse, ud, use_config);

                    let parent_flag_state = match ud.kind {
                        UseDepKind::Conditional
                        | UseDepKind::ConditionalInverse
                        | UseDepKind::Equal
                        | UseDepKind::EqualInverse => {
                            let parent_virtual_name = format!(
                                "virtual/USE_{}_{}",
                                constraint.parent_cpn_str.replace('/', "_"),
                                ud.flag.as_str()
                            );
                            resolve_parent_flag(
                                &ud.flag,
                                &parent_virtual_name,
                                use_config,
                                solution,
                            )
                        }
                        _ => UseFlagState::Disabled,
                    };

                    let satisfied = match ud.kind {
                        UseDepKind::Enabled => target_flag_state == UseFlagState::Enabled,
                        UseDepKind::Disabled => target_flag_state == UseFlagState::Disabled,
                        UseDepKind::Conditional => {
                            if parent_flag_state == UseFlagState::Enabled {
                                target_flag_state == UseFlagState::Enabled
                            } else {
                                true
                            }
                        }
                        UseDepKind::ConditionalInverse => {
                            if parent_flag_state == UseFlagState::Disabled {
                                target_flag_state == UseFlagState::Enabled
                            } else {
                                true
                            }
                        }
                        UseDepKind::Equal => target_flag_state == parent_flag_state,
                        UseDepKind::EqualInverse => target_flag_state != parent_flag_state,
                    };

                    if !satisfied {
                        errors.push(Error::UseDepConflict(
                            format!("{}-{}", pkg, version),
                            format!(
                                "{}: {}[{}] not satisfied (target={:?}, parent={:?})",
                                target_pkg,
                                match ud.kind {
                                    UseDepKind::Enabled => format!("+{}", ud.flag.as_str()),
                                    UseDepKind::Disabled => format!("-{}", ud.flag.as_str()),
                                    UseDepKind::Conditional => format!("{}?", ud.flag.as_str()),
                                    UseDepKind::ConditionalInverse =>
                                        format!("!{}?", ud.flag.as_str()),
                                    UseDepKind::Equal => format!("{}=", ud.flag.as_str()),
                                    UseDepKind::EqualInverse => format!("!{}=", ud.flag.as_str()),
                                },
                                ud.flag.as_str(),
                                target_flag_state,
                                parent_flag_state,
                            ),
                        ));
                    }
                }
            }
        }
        errors
    }

    /// Validate repository constraints against a solution (post-solve check).
    ///
    /// For each package in the solution that declares `::repo` constraints,
    /// verify the target package comes from the required repository.
    ///
    /// See [PMS 8.3.5](https://projects.gentoo.org/pms/9/pms.html#repository).
    pub fn check_repo_constraints(
        &self,
        solution: &pubgrub::SelectedDependencies<PortagePackage, Version>,
    ) -> Vec<Error> {
        let mut errors = Vec::new();
        for (pkg, version) in solution.iter() {
            let Some(data) = self.packages.get(pkg) else {
                continue;
            };
            let Some(constraints) = data.repo_constraints.get(version) else {
                continue;
            };
            for constraint in constraints {
                let (target_pkg, target_vs) = &constraint.target;
                let target_entry = solution
                    .iter()
                    .find(|(p, v)| p.cpn == target_pkg.cpn && target_vs.contains(v));
                let Some((target_pkg_key, target_ver)) = target_entry else {
                    continue;
                };
                let target_repo = self
                    .packages
                    .get(target_pkg_key)
                    .and_then(|d| d.repo.get(target_ver));
                match target_repo {
                    Some(r) if *r == constraint.repo => {}
                    _ => {
                        errors.push(Error::RepoConstraintConflict(
                            format!("{}-{}", pkg, version),
                            format!(
                                "{}::{} required but target comes from {:?}",
                                target_pkg,
                                constraint.repo.as_str(),
                                target_repo.map(|r| r.as_str()),
                            ),
                        ));
                    }
                }
            }
        }
        errors
    }

    /// Validate blockers against a solution (post-solve check).
    ///
    /// A blocker `!dev-libs/foo` means that if this package is installed,
    /// `dev-libs/foo` (with matching version/slot constraints if any) must
    /// NOT be in the solution.
    pub fn check_blockers(
        &self,
        solution: &pubgrub::SelectedDependencies<PortagePackage, Version>,
    ) -> Vec<Error> {
        let mut conflicts = Vec::new();
        for (pkg, version) in solution.iter() {
            let Some(data) = self.packages.get(pkg) else {
                continue;
            };
            let Some(blockers) = data.blockers.get(version) else {
                continue;
            };
            for blocker in blockers {
                let matches = solution.iter().any(|(sol_pkg, sol_ver)| {
                    if !blocker_cpn_slot_matches(sol_pkg, blocker) {
                        return false;
                    }
                    match &blocker.version {
                        None => true,
                        Some(v) => {
                            let op = blocker.op.unwrap_or(portage_atom::Operator::Equal);
                            version_matches_operator(sol_ver, op, blocker.glob, v)
                        }
                    }
                });
                if matches {
                    let strength = match blocker.blocker {
                        Some(portage_atom::Blocker::Strong) => "strong(!!)",
                        _ => "weak(!)",
                    };
                    conflicts.push(Error::BlockerConflict {
                        pkg: format!("{}-{}", pkg, version),
                        blocker: blocker.to_string(),
                        strength,
                    });
                }
            }
        }
        conflicts
    }

    /// Resolve slot-operator bindings from a solution.
    ///
    /// For each package in the solution that declared `:=` / `:0=` dependencies,
    /// look up the actual slot the solver assigned to each target and return
    /// the bindings. This information is used for rebuild tracking: when a
    /// dependency's slot changes, the parent package must be rebuilt.
    ///
    /// `:*` dependencies are excluded — they express "any slot" with no
    /// rebuild implications.
    ///
    /// See [PMS 8.3.3](https://projects.gentoo.org/pms/9/pms.html#slot_deps).
    pub fn slot_operator_bindings(
        &self,
        solution: &pubgrub::SelectedDependencies<PortagePackage, Version>,
    ) -> Vec<SlotOperatorBinding> {
        let mut bindings = Vec::new();
        for (pkg, version) in solution.iter() {
            let Some(data) = self.packages.get(pkg) else {
                continue;
            };
            let Some(ops) = data.slot_operator_deps.get(version) else {
                continue;
            };
            for op_dep in ops {
                let bound_slot = solution.iter().find_map(|(sol_pkg, sol_ver)| {
                    if sol_pkg.cpn == op_dep.target.0.cpn && op_dep.target.1.contains(sol_ver) {
                        Some((sol_pkg.slot, sol_ver.clone()))
                    } else {
                        None
                    }
                });
                if let Some((slot, _ver)) = bound_slot {
                    bindings.push(SlotOperatorBinding {
                        parent: format!("{}-{}", pkg, version),
                        target_cpn: format!("{}", op_dep.target.0.cpn),
                        slot,
                        declared_slot: op_dep.slot,
                        operator: match op_dep.operator {
                            SlotOperator::Equal => "=",
                            SlotOperator::Star => "*",
                        },
                    });
                }
            }
        }
        bindings
    }
}

pub(crate) fn blocker_cpn_slot_matches(sol_pkg: &PortagePackage, blocker: &Dep) -> bool {
    if sol_pkg.cpn != blocker.cpn {
        return false;
    }
    match &blocker.slot_dep {
        None => true,
        Some(portage_atom::SlotDep::Slot { slot: Some(sd), .. }) => sol_pkg.slot == Some(sd.slot),
        Some(portage_atom::SlotDep::Slot { slot: None, .. }) => true,
        Some(portage_atom::SlotDep::Operator(_)) => true,
    }
}

pub(crate) fn version_matches_operator(
    candidate: &Version,
    op: portage_atom::Operator,
    glob: bool,
    target: &Version,
) -> bool {
    use std::cmp::Ordering;
    let cmp = candidate.cmp(target);
    match op {
        portage_atom::Operator::Equal => {
            if glob {
                candidate.glob_matches(target)
            } else {
                cmp == Ordering::Equal
            }
        }
        portage_atom::Operator::GreaterOrEqual => cmp != Ordering::Less,
        portage_atom::Operator::Greater => cmp == Ordering::Greater,
        portage_atom::Operator::LessOrEqual => cmp != Ordering::Greater,
        portage_atom::Operator::Less => cmp == Ordering::Less,
        portage_atom::Operator::Approximate => {
            let mut base = target.clone();
            base.revision = portage_atom::Revision::default();
            candidate >= &base && candidate < target
        }
    }
}

pub(crate) fn resolve_flag_state(
    target_iuse: &[Interned<DefaultInterner>],
    ud: &portage_atom::UseDep,
    use_config: &UseConfig,
) -> UseFlagState {
    let flag_defined = target_iuse.iter().any(|f| f.as_str() == ud.flag.as_str());
    if flag_defined {
        use_config.get(&ud.flag)
    } else {
        match ud.default {
            Some(portage_atom::UseDefault::Enabled) => UseFlagState::Enabled,
            Some(portage_atom::UseDefault::Disabled) => UseFlagState::Disabled,
            None => UseFlagState::Disabled,
        }
    }
}

pub(crate) fn resolve_parent_flag(
    flag: &Interned<DefaultInterner>,
    virtual_name: &str,
    use_config: &UseConfig,
    solution: &pubgrub::SelectedDependencies<PortagePackage, Version>,
) -> UseFlagState {
    let config_state = use_config.get(flag);
    if config_state != UseFlagState::SolverDecided {
        return config_state;
    }

    if let Ok(virtual_cpn) = portage_atom::Cpn::parse(virtual_name) {
        for (pkg, ver) in solution.iter() {
            if pkg.cpn == virtual_cpn {
                let ver_str = ver.to_string();
                if ver_str == "1" {
                    return UseFlagState::Enabled;
                } else {
                    return UseFlagState::Disabled;
                }
            }
        }
    }

    UseFlagState::Disabled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{InMemoryRepository, PackageDeps};
    use crate::version_set::PortageVersionSet;
    use portage_atom::gentoo_interner::Interned;
    use portage_atom::{Cpn, Dep, DepEntry};

    fn empty_deps() -> PackageDeps {
        PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        }
    }

    #[test]
    fn slot_operator_binding_bare_equals() {
        let mut repo = InMemoryRepository::new();
        let slot_0 = Interned::<DefaultInterner>::intern("0");

        repo.add_version(
            portage_atom::Cpv::parse("app-misc/app-1.0").unwrap(),
            Some(slot_0),
            None,
            PackageDeps {
                depend: vec![DepEntry::Atom(Dep::parse("dev-libs/lib:=").unwrap())],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/lib-1.0").unwrap(),
            Some(slot_0),
            None,
            empty_deps(),
        );

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let app = PortagePackage::slotted(Cpn::parse("app-misc/app").unwrap(), slot_0);
        let solution = provider
            .resolve_targets(vec![(app, PortageVersionSet::any())])
            .unwrap();
        let bindings = provider.slot_operator_bindings(&solution);
        assert_eq!(bindings.len(), 1, "should have one slot-operator binding");
        assert_eq!(bindings[0].operator, "=");
        assert_eq!(bindings[0].declared_slot, None);
        assert_eq!(bindings[0].slot, Some(slot_0));
    }

    #[test]
    fn slot_operator_binding_explicit_slot() {
        let mut repo = InMemoryRepository::new();
        let slot_0 = Interned::<DefaultInterner>::intern("0");

        repo.add_version(
            portage_atom::Cpv::parse("app-misc/app-1.0").unwrap(),
            Some(slot_0),
            None,
            PackageDeps {
                depend: vec![DepEntry::Atom(Dep::parse("dev-libs/lib:0=").unwrap())],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/lib-1.0").unwrap(),
            Some(slot_0),
            None,
            empty_deps(),
        );

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let app = PortagePackage::slotted(Cpn::parse("app-misc/app").unwrap(), slot_0);
        let solution = provider
            .resolve_targets(vec![(app, PortageVersionSet::any())])
            .unwrap();
        let bindings = provider.slot_operator_bindings(&solution);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].operator, "=");
        assert_eq!(bindings[0].declared_slot, Some(slot_0));
    }
    #[test]
    fn check_blockers_detects_conflict() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: vec![DepEntry::Atom(Dep::parse("!dev-libs/libressl").unwrap())],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/libressl-3.9.0").unwrap(),
            None,
            None,
            empty(),
        );

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let openssl = PortagePackage::unslotted(Cpn::parse("dev-libs/openssl").unwrap());
        let libressl = PortagePackage::unslotted(Cpn::parse("dev-libs/libressl").unwrap());
        let solution = provider
            .resolve_targets(vec![
                (openssl, PortageVersionSet::any()),
                (libressl, PortageVersionSet::any()),
            ])
            .unwrap();
        let conflicts = provider.check_blockers(&solution);
        assert!(
            !conflicts.is_empty(),
            "should detect blocker conflict between openssl and libressl"
        );
    }

    #[test]
    fn check_blockers_no_conflict_when_clean() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: vec![DepEntry::Atom(Dep::parse("!dev-libs/libressl").unwrap())],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/foo-1.0").unwrap(),
            None,
            None,
            empty(),
        );

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let openssl = PortagePackage::unslotted(Cpn::parse("dev-libs/openssl").unwrap());
        let foo = PortagePackage::unslotted(Cpn::parse("app-misc/foo").unwrap());
        let solution = provider
            .resolve_targets(vec![
                (openssl, PortageVersionSet::any()),
                (foo, PortageVersionSet::any()),
            ])
            .unwrap();
        let conflicts = provider.check_blockers(&solution);
        assert!(
            conflicts.is_empty(),
            "no blocker conflict expected: {conflicts:?}"
        );
    }

    #[test]
    fn check_use_deps_enabled_satisfied() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version_with_iuse(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            vec![Interned::intern("ssl")],
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/myapp-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: DepEntry::parse("dev-libs/openssl[ssl]").unwrap(),
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );

        let mut use_config = UseConfig::new();
        use_config.enable(Interned::intern("ssl"));

        let mut provider = PortageDependencyProvider::new(repo, use_config.clone());
        let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
        let solution = provider
            .resolve_targets(vec![(myapp, PortageVersionSet::any())])
            .unwrap();
        let errors = provider.check_use_deps(&solution, &use_config);
        assert!(errors.is_empty(), "unexpected USE-dep errors: {errors:?}");
    }

    #[test]
    fn check_use_deps_enabled_violated() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/myapp-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: DepEntry::parse("dev-libs/openssl[ssl]").unwrap(),
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );

        let use_config = UseConfig::new();

        let mut provider = PortageDependencyProvider::new(repo, use_config.clone());
        let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
        let solution = provider
            .resolve_targets(vec![(myapp, PortageVersionSet::any())])
            .unwrap();
        let errors = provider.check_use_deps(&solution, &use_config);
        assert!(
            !errors.is_empty(),
            "should detect USE-dep violation for [ssl] when ssl is off"
        );
    }

    #[test]
    fn check_use_deps_conditional_with_parent_on() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/myapp-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: DepEntry::parse("dev-libs/openssl[ssl?]").unwrap(),
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );

        let mut use_config = UseConfig::new();
        use_config.enable(Interned::intern("ssl"));

        let mut provider = PortageDependencyProvider::new(repo, use_config.clone());
        let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
        let solution = provider
            .resolve_targets(vec![(myapp, PortageVersionSet::any())])
            .unwrap();
        let errors = provider.check_use_deps(&solution, &use_config);
        assert!(
            !errors.is_empty(),
            "[ssl?] with parent ssl=ON should require target ssl=ON"
        );
    }

    #[test]
    fn check_use_deps_conditional_with_parent_off() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/myapp-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: DepEntry::parse("dev-libs/openssl[ssl?]").unwrap(),
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );

        let use_config = UseConfig::new();

        let mut provider = PortageDependencyProvider::new(repo, use_config.clone());
        let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
        let solution = provider
            .resolve_targets(vec![(myapp, PortageVersionSet::any())])
            .unwrap();
        let errors = provider.check_use_deps(&solution, &use_config);
        assert!(
            errors.is_empty(),
            "[ssl?] with parent ssl=OFF should be unconstrained"
        );
    }

    #[test]
    fn check_use_deps_equal_matches() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/myapp-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: DepEntry::parse("dev-libs/openssl[ssl=]").unwrap(),
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );

        let use_config = UseConfig::new();

        let mut provider = PortageDependencyProvider::new(repo, use_config.clone());
        let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
        let solution = provider
            .resolve_targets(vec![(myapp, PortageVersionSet::any())])
            .unwrap();
        let errors = provider.check_use_deps(&solution, &use_config);
        assert!(
            errors.is_empty(),
            "[ssl=] with both parent/target ssl=OFF should match"
        );
    }

    #[test]
    fn check_repo_constraint_satisfied() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };
        let gentoo = Interned::intern("gentoo");

        repo.add_version_full(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            Some(gentoo),
            vec![],
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/myapp-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: DepEntry::parse("dev-libs/openssl::gentoo").unwrap(),
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
        let solution = provider
            .resolve_targets(vec![(myapp, PortageVersionSet::any())])
            .unwrap();
        let errors = provider.check_repo_constraints(&solution);
        assert!(
            errors.is_empty(),
            "repo constraint should be satisfied: {errors:?}"
        );
    }

    #[test]
    fn check_repo_constraint_violated() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version_full(
            portage_atom::Cpv::parse("dev-libs/openssl-3.0.0").unwrap(),
            None,
            None,
            Some(Interned::intern("other")),
            vec![],
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/myapp-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: DepEntry::parse("dev-libs/openssl::gentoo").unwrap(),
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let myapp = PortagePackage::unslotted(Cpn::parse("app-misc/myapp").unwrap());
        let solution = provider
            .resolve_targets(vec![(myapp, PortageVersionSet::any())])
            .unwrap();
        let errors = provider.check_repo_constraints(&solution);
        assert!(
            !errors.is_empty(),
            "should detect repo constraint violation"
        );
    }

    #[test]
    fn check_blockers_respects_slot() {
        use super::*;

        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        let slot_0 = Interned::<DefaultInterner>::intern("0");
        let slot_1 = Interned::<DefaultInterner>::intern("1");

        repo.add_version(
            portage_atom::Cpv::parse("app-misc/blocker-pkg-1.0").unwrap(),
            Some(slot_0),
            None,
            PackageDeps {
                depend: vec![DepEntry::Atom(Dep::parse("!app-misc/target:0").unwrap())],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/target-1.0").unwrap(),
            Some(slot_0),
            None,
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("app-misc/target-1.0").unwrap(),
            Some(slot_1),
            None,
            empty(),
        );

        // Part 1: target:0 should conflict with blocker !target:0
        let blocker_pkg =
            PortagePackage::slotted(Cpn::parse("app-misc/blocker-pkg").unwrap(), slot_0);
        let target_slot0 = PortagePackage::slotted(Cpn::parse("app-misc/target").unwrap(), slot_0);

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let solution = provider
            .resolve_targets(vec![
                (blocker_pkg.clone(), PortageVersionSet::any()),
                (target_slot0, PortageVersionSet::any()),
            ])
            .unwrap();
        let conflicts = provider.check_blockers(&solution);
        assert!(
            !conflicts.is_empty(),
            "blocker !target:0 should conflict with target:0 in solution"
        );

        // Part 2: !target:0 should NOT conflict when only target:1 is in solution
        let mut repo2 = InMemoryRepository::new();
        repo2.add_version(
            portage_atom::Cpv::parse("app-misc/blocker-pkg-1.0").unwrap(),
            Some(slot_0),
            None,
            PackageDeps {
                depend: vec![DepEntry::Atom(Dep::parse("!app-misc/target:0").unwrap())],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        repo2.add_version(
            portage_atom::Cpv::parse("app-misc/target-1.0").unwrap(),
            Some(slot_1),
            None,
            empty(),
        );

        let target_slot1 = PortagePackage::slotted(Cpn::parse("app-misc/target").unwrap(), slot_1);
        let mut provider2 = PortageDependencyProvider::new(repo2, UseConfig::new());
        let solution2 = provider2
            .resolve_targets(vec![
                (blocker_pkg, PortageVersionSet::any()),
                (target_slot1, PortageVersionSet::any()),
            ])
            .unwrap();
        let conflicts2 = provider2.check_blockers(&solution2);
        assert!(
            conflicts2.is_empty(),
            "blocker !target:0 should NOT conflict with target:1 in solution"
        );
    }

    #[test]
    fn check_blockers_version_matched() {
        let mut repo = InMemoryRepository::new();
        let empty = || PackageDeps {
            depend: vec![],
            rdepend: vec![],
            bdepend: vec![],
            pdepend: vec![],
            idepend: vec![],
        };

        repo.add_version(
            portage_atom::Cpv::parse("app-misc/app-1.0").unwrap(),
            None,
            None,
            PackageDeps {
                depend: vec![DepEntry::Atom(Dep::parse("!>=dev-libs/lib-2.0").unwrap())],
                rdepend: vec![],
                bdepend: vec![],
                pdepend: vec![],
                idepend: vec![],
            },
        );
        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/lib-1.0").unwrap(),
            None,
            None,
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/lib-2.0").unwrap(),
            None,
            None,
            empty(),
        );
        repo.add_version(
            portage_atom::Cpv::parse("dev-libs/lib-3.0").unwrap(),
            None,
            None,
            empty(),
        );

        let mut provider = PortageDependencyProvider::new(repo, UseConfig::new());
        let app = PortagePackage::unslotted(Cpn::parse("app-misc/app").unwrap());
        let lib_pkg = PortagePackage::unslotted(Cpn::parse("dev-libs/lib").unwrap());
        let solution = provider
            .resolve_targets(vec![
                (app, PortageVersionSet::any()),
                (lib_pkg, PortageVersionSet::any()),
            ])
            .unwrap();
        let lib_ver = solution
            .get(&PortagePackage::unslotted(
                Cpn::parse("dev-libs/lib").unwrap(),
            ))
            .unwrap();

        let conflicts = provider.check_blockers(&solution);
        if lib_ver >= &Version::parse("2.0").unwrap() {
            assert!(
                !conflicts.is_empty(),
                "blocker !>=lib-2.0 should conflict with lib-{}",
                lib_ver
            );
        }
    }
}
