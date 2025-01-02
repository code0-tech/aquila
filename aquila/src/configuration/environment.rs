use std::str::FromStr;

pub enum Environment {
    Development,
    Staging,
    Production,
}

impl FromStr for Environment {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "staging" => Ok(Environment::Staging),
            "production" => Ok(Environment::Production),
            "development" => Ok(Environment::Development),
            _ => Err(())
        }
    }
}