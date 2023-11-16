use std::env;

use crate::Result;

use reqwest::Url;
use twitch_oauth2::{ClientId, ClientSecret};

#[derive(Debug, Clone)]
pub struct Config {
    pub server_protocol: String,
    pub server_domain: String,
    pub server_port: Option<u16>,
    pub server_host_uri: String,

    pub twitch_client_id: ClientId,
    pub twitch_client_secret: ClientSecret,
    pub twitch_callback_url: Url,

    pub db_connection_url: String,
    pub db_database: String,
}

pub fn load() -> Result<Config> {
    if cfg!(debug_assertions) {
        dotenv::dotenv()?;
    }

    let server_protocol = env::var("SERVER_PROTOCOL")?;
    let server_domain = env::var("SERVER_DOMAIN")?;
    let server_port = env::var("SERVER_PORT")?;
    let server_host_uri = env::var("SERVER_HOST_URI")?;

    let server_port = if &server_port == "" {
        None
    } else {
        Some(server_port.parse()?)
    };

    let twitch_callback_url = format!("{server_host_uri}/twitch/callback")
        .parse()
        .unwrap();

    return Ok(Config {
        server_protocol,
        server_domain,
        server_port,
        server_host_uri,

        twitch_client_id: env::var("TWITCH_CLIENT_ID")?.into(),
        twitch_client_secret: env::var("TWITCH_CLIENT_SECRET")?.into(),
        twitch_callback_url,

        db_connection_url: env::var("DB_CONNECTION_URL")?.into(),
        db_database: env::var("DB_DATABASE")?.into(),
    });
}
