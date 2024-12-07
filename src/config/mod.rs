#[derive(Debug)]
pub struct Config {
    pub uti: String,
    pub suffix: String,
}

use std::fmt;

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.uti, self.suffix)
    }
}

impl Config {
    pub fn new(uti: String, suffix: String) -> Config {
        Config { uti, suffix }
    }
}
