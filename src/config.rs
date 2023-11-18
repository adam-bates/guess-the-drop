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

    pub r2_bucket: String,
    pub r2_account_id: String,
    pub r2_bucket_public_url: String,

    pub r2_s3_access_key_id: String,
    pub r2_s3_secret_access_key: String,
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

        db_connection_url: env::var("DB_CONNECTION_URL")?,
        db_database: env::var("DB_DATABASE")?,

        r2_bucket: env::var("R2_BUCKET")?,
        r2_bucket_public_url: env::var("R2_BUCKET_PUBLIC_URL")?,
        r2_account_id: env::var("R2_ACCOUNT_ID")?,

        r2_s3_access_key_id: env::var("R2_S3_ACCESS_KEY_ID")?,
        r2_s3_secret_access_key: env::var("R2_S3_SECRET_ACCESS_KEY")?,
    });
}
