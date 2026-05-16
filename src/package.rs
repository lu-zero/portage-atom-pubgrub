use std::fmt;

use portage_atom::Cpn;
use portage_atom::gentoo_interner::{DefaultInterner, Interned};

/// A PubGrub-compatible package identifier combining a CPN with an optional slot.
///
/// Slots are encoded into the package identity so that `dev-lang/python:3.11`
/// and `dev-lang/python:3.12` are distinct packages from the solver's
/// perspective — the solver can install both simultaneously.
///
/// Implements `Clone + Eq + Hash + Debug + Display`, satisfying pubgrub's
/// `Package` trait via blanket implementation.
///
/// See [PMS 8.3.3](https://projects.gentoo.org/pms/9/pms.html#slot_deps).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct PortagePackage {
    /// Category/package name.
    pub cpn: Cpn,
    /// Optional slot name (e.g. `"3.11"`, `"0"`).
    pub slot: Option<Interned<DefaultInterner>>,
}

impl PortagePackage {
    /// Create a new package from a CPN and optional slot.
    pub fn new(cpn: Cpn, slot: Option<Interned<DefaultInterner>>) -> Self {
        Self { cpn, slot }
    }

    /// Create an unslotted package.
    pub fn unslotted(cpn: Cpn) -> Self {
        Self { cpn, slot: None }
    }

    /// Create a slotted package.
    pub fn slotted(cpn: Cpn, slot: Interned<DefaultInterner>) -> Self {
        Self {
            cpn,
            slot: Some(slot),
        }
    }

    /// Returns the display string without slot suffix.
    pub fn cpn_str(&self) -> String {
        self.cpn.to_string()
    }
}

impl Ord for PortagePackage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cpn
            .cmp(&other.cpn)
            .then_with(|| match (&self.slot, &other.slot) {
                (Some(a), Some(b)) => a.as_str().cmp(b.as_str()),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => std::cmp::Ordering::Equal,
            })
    }
}

impl PartialOrd for PortagePackage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for PortagePackage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.slot {
            Some(slot) => write!(f, "{}:{}", self.cpn, slot),
            None => write!(f, "{}", self.cpn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_unslotted() {
        let cpn = Cpn::parse("dev-lang/rust").unwrap();
        let pkg = PortagePackage::unslotted(cpn);
        assert_eq!(pkg.to_string(), "dev-lang/rust");
    }

    #[test]
    fn display_slotted() {
        let cpn = Cpn::parse("dev-lang/python").unwrap();
        let slot = Interned::intern("3.12");
        let pkg = PortagePackage::slotted(cpn, slot);
        assert_eq!(pkg.to_string(), "dev-lang/python:3.12");
    }

    #[test]
    fn different_slots_are_different_packages() {
        let cpn = Cpn::parse("dev-lang/python").unwrap();
        let p1 = PortagePackage::slotted(cpn, Interned::intern("3.11"));
        let p2 = PortagePackage::slotted(Cpn::parse("dev-lang/python").unwrap(), Interned::intern("3.12"));
        assert_ne!(p1, p2);
    }

    #[test]
    fn same_slot_is_same_package() {
        let cpn = Cpn::parse("dev-lang/python").unwrap();
        let p1 = PortagePackage::slotted(cpn, Interned::intern("3.12"));
        let p2 = PortagePackage::slotted(Cpn::parse("dev-lang/python").unwrap(), Interned::intern("3.12"));
        assert_eq!(p1, p2);
    }
}
