mod config;

pub const BOT_NAME: &str = "Lum";

fn main() {
    let config_handler = config::ConfigHandler::new(BOT_NAME.to_lowercase().as_str());
    let config = match config_handler.get_config() {
        Ok(config) => config,
        Err(err) => {
            panic!("Error reading config file: {}", err);
        }
    };

    println!("Config: {}", config);
}
