use std::fmt;
use std::num::NonZeroU64;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThemeOutputId(NonZeroU64);

impl ThemeOutputId {
    pub fn new(raw: u64) -> Self {
        match NonZeroU64::new(raw) {
            Some(id) => Self(id),
            None => panic!("theme output id must be non-zero"),
        }
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for ThemeOutputId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThemeViewId(NonZeroU64);

impl ThemeViewId {
    pub fn new(raw: u64) -> Self {
        match NonZeroU64::new(raw) {
            Some(id) => Self(id),
            None => panic!("theme view id must be non-zero"),
        }
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for ThemeViewId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}
