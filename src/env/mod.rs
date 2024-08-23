pub mod environment {
    
    use std::str::FromStr;
    use log::error;

    pub fn get_env_with_default<T>(name: &str, default: T) -> T
    where
        T: FromStr,
    {
        let env_variable = match std::env::var(name) {
            Ok(env) => env,
            Err(find_error) => {
                error!("Env. Variable {name} wasn't found. Reason: {find_error}");
                return default;
            }
        };

        env_variable.parse::<T>().unwrap_or_else(|_| default)
    }

}