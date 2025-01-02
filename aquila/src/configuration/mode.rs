use std::str::FromStr;

/// Aquila Startup-Mode
/// 
/// STATIC: 
/// Aquila will start from configuration file
/// 
/// DYNAMIC: 
/// Aquila will be updated by releases (via request scheduler)
pub enum Mode {
    STATIC,
    DYNAMIC,
}

impl FromStr for Mode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "static" => Ok(Mode::STATIC),
            "dynamic" => Ok(Mode::DYNAMIC),
            _ => Err(()),
        }
    }
}

impl PartialEq<Mode> for &Mode {
    fn eq(&self, other: &Mode) -> bool {
        match (*self, other) {
            (Mode::STATIC, Mode::STATIC) => true,
            (Mode::DYNAMIC, Mode::DYNAMIC) => true,
            _ => false
        }
    }
}