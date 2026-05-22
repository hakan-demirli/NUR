use std::fmt;

#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u64);

impl Version {
    pub const ZERO: Self = Self(0);

    pub fn new(n: u64) -> Self {
        Self(n)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn bump(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }

    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_zero() {
        assert_eq!(Version::default(), Version::ZERO);
        assert_eq!(Version::default().get(), 0);
    }

    #[test]
    fn bump_increments_by_one() {
        let mut v = Version::default();
        v.bump();
        assert_eq!(v.get(), 1);
        v.bump();
        v.bump();
        assert_eq!(v.get(), 3);
    }

    #[test]
    fn next_returns_incremented_without_mutating() {
        let v = Version::new(5);
        assert_eq!(v.next().get(), 6);
        assert_eq!(v.get(), 5);
    }

    #[test]
    fn versions_are_ordered_by_value() {
        assert!(Version::new(1) < Version::new(2));
        assert!(Version::new(10) > Version::new(9));
    }

    #[test]
    fn versions_wrap_on_overflow_without_panicking() {
        let mut v = Version::new(u64::MAX);
        v.bump();
        assert_eq!(v.get(), 0);
    }
}
