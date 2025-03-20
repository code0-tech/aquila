use configuration::config::Config;

pub mod authorization;
pub mod configuration;
pub mod server;
pub mod stream;

fn main() {
    let config = Config::new();
    config.print_config();
}
