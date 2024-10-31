pub enum Environment {
    Development,
    Staging,
    Production,
}

impl Environment {
    pub(crate) fn from_str(env: &str) -> Self {
        match env.to_lowercase().as_str() {
            "staging" => Environment::Staging,
            "production" => Environment::Production,
            _ => Environment::Development,
        }
    }
}