use std::collections::HashMap;

use portage_atom::gentoo_interner::{DefaultInterner, Interned};

use crate::repository::IUseDefault;

/// How a single USE flag should be evaluated during dependency conversion.
///
/// See [PMS 8.2](https://projects.gentoo.org/pms/9/pms.html#use-flag-dependent-dependencies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseFlagState {
    /// The flag is ON — `flag? ( deps )` includes deps, `!flag? ( deps )` skips.
    Enabled,
    /// The flag is OFF — `flag? ( deps )` skips deps, `!flag? ( deps )` includes.
    Disabled,
    /// The solver decides — create a virtual package with versions 0/1.
    SolverDecided,
}

/// Configuration for USE flag evaluation during dependency conversion.
///
/// See [PMS 8.2](https://projects.gentoo.org/pms/9/pms.html#use-flag-dependent-dependencies).
#[derive(Debug, Clone, Default)]
pub struct UseConfig {
    flags: HashMap<Interned<DefaultInterner>, UseFlagState>,
}

impl UseConfig {
    /// Create an empty config (all flags default to `Disabled`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a flag's state.
    pub fn set(&mut self, flag: Interned<DefaultInterner>, state: UseFlagState) {
        self.flags.insert(flag, state);
    }

    /// Enable a flag.
    pub fn enable(&mut self, flag: Interned<DefaultInterner>) {
        self.flags.insert(flag, UseFlagState::Enabled);
    }

    /// Disable a flag.
    pub fn disable(&mut self, flag: Interned<DefaultInterner>) {
        self.flags.insert(flag, UseFlagState::Disabled);
    }

    /// Mark a flag as solver-decided.
    pub fn solver_decide(&mut self, flag: Interned<DefaultInterner>) {
        self.flags.insert(flag, UseFlagState::SolverDecided);
    }

    /// Get the state of a flag. Unset flags default to `Disabled`.
    pub fn get(&self, flag: &Interned<DefaultInterner>) -> UseFlagState {
        self.flags
            .get(flag)
            .copied()
            .unwrap_or(UseFlagState::Disabled)
    }

    /// Get the state of a flag, falling back to an IUSE default if the flag
    /// is not explicitly configured.
    ///
    /// If the flag is set in the config, returns its state. Otherwise, if the
    /// IUSE default is `Enabled` (the `+` prefix), returns `Enabled`.
    /// Otherwise returns `Disabled`.
    pub fn get_with_iuse_default(
        &self,
        flag: &Interned<DefaultInterner>,
        iuse_default: Option<IUseDefault>,
    ) -> UseFlagState {
        match self.flags.get(flag) {
            Some(&state) => state,
            None => match iuse_default {
                Some(IUseDefault::Enabled) => UseFlagState::Enabled,
                _ => UseFlagState::Disabled,
            },
        }
    }

    /// Returns all solver-decided flags.
    pub fn solver_decided_flags(&self) -> Vec<Interned<DefaultInterner>> {
        self.flags
            .iter()
            .filter(|(_, s)| **s == UseFlagState::SolverDecided)
            .map(|(f, _)| *f)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_defaults_to_disabled() {
        let config = UseConfig::new();
        let flag = Interned::intern("ssl");
        assert_eq!(config.get(&flag), UseFlagState::Disabled);
    }

    #[test]
    fn enable_disable() {
        let mut config = UseConfig::new();
        let flag = Interned::intern("ssl");
        config.enable(flag);
        assert_eq!(config.get(&flag), UseFlagState::Enabled);
        config.disable(flag);
        assert_eq!(config.get(&flag), UseFlagState::Disabled);
    }

    #[test]
    fn solver_decided_flags() {
        let mut config = UseConfig::new();
        let ssl = Interned::intern("ssl");
        let debug = Interned::intern("debug");
        let test = Interned::intern("test");
        config.enable(ssl);
        config.solver_decide(debug);
        config.solver_decide(test);
        let decided = config.solver_decided_flags();
        assert_eq!(decided.len(), 2);
    }

    #[test]
    fn set_method() {
        let mut config = UseConfig::new();
        let flag = Interned::intern("ssl");
        config.set(flag, UseFlagState::Enabled);
        assert_eq!(config.get(&flag), UseFlagState::Enabled);
        config.set(flag, UseFlagState::SolverDecided);
        assert_eq!(config.get(&flag), UseFlagState::SolverDecided);
    }

    #[test]
    fn get_with_iuse_default_none_returns_disabled() {
        let config = UseConfig::new();
        let flag = Interned::intern("ssl");
        assert_eq!(
            config.get_with_iuse_default(&flag, None),
            UseFlagState::Disabled
        );
    }

    #[test]
    fn get_with_iuse_default_disabled() {
        let config = UseConfig::new();
        let flag = Interned::intern("ssl");
        assert_eq!(
            config.get_with_iuse_default(&flag, Some(IUseDefault::Disabled)),
            UseFlagState::Disabled
        );
    }

    #[test]
    fn get_with_iuse_default_overridden_by_config() {
        let mut config = UseConfig::new();
        let flag = Interned::intern("ssl");
        config.disable(flag);
        assert_eq!(
            config.get_with_iuse_default(&flag, Some(IUseDefault::Enabled)),
            UseFlagState::Disabled
        );
    }

    #[test]
    fn solver_decided_flags_empty() {
        let config = UseConfig::new();
        assert!(config.solver_decided_flags().is_empty());
    }
}
