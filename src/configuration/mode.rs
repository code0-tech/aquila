use std::str::FromStr;

/// Aquila Startup-Mode
///
/// STATIC:
/// Aquila will start from configuration file
///
/// DYNAMIC:
/// Aquila will be updated by releases (via request scheduler)
#[derive(PartialEq, Debug)]
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
