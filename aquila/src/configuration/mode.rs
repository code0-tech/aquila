/// Aquila Startup-Mode
/// 
/// STATIC: 
/// Aquila will start from configuration file
/// 
/// DYNAMIC: 
/// Aquila will be updated by releases (via request scheduler)
/// 
/// HYBRID
/// Aquila will be updated by updates (via stream)
pub enum Mode {
    STATIC,
    DYNAMIC,
    HYBRID,
}

impl Mode {
    pub(crate) fn from_str(string: &str) -> Mode {
        match string.to_lowercase().as_str() {
            "static" => Mode::STATIC,
            "dynamic" => Mode::DYNAMIC,
            "hybrid" => Mode::HYBRID,
            _ => Mode::STATIC,
        }
    }
}

impl PartialEq<Mode> for &Mode {
    fn eq(&self, other: &Mode) -> bool {
        match (*self, other) {
            (Mode::STATIC, Mode::STATIC) => true,
            (Mode::HYBRID, Mode::HYBRID) => true,
            (Mode::DYNAMIC, Mode::DYNAMIC) => true,
            _ => false
        }
    }
}