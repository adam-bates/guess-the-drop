use reqwest::Url;
use shuttle_secrets::SecretStore;
use twitch_oauth2::{ClientId, ClientSecret};

#[derive(Debug, Clone)]
pub struct Config {
    pub server_protocol: String,
    pub server_domain: String,
    pub server_port: String,
    pub server_host_uri: String,

    pub twitch_client_id: ClientId,
    pub twitch_client_secret: ClientSecret,
    pub twitch_callback_url: Url,
}

pub fn build(secrets: SecretStore) -> Config {
    let server_protocol = secrets.get("SERVER_PROTOCOL").unwrap();
    let server_domain = secrets.get("SERVER_DOMAIN").unwrap();
    let server_port = secrets.get("SERVER_PORT").unwrap();

    let server_port_postfix = if &server_port == "" {
        "".to_string()
    } else {
        format!(":{server_port}")
    };

    let server_host_uri = format!("{server_protocol}://{server_domain}{server_port_postfix}");

    let twitch_callback_url = format!("{server_host_uri}/twitch/callback")
        .parse()
        .unwrap();

    return Config {
        server_protocol,
        server_domain,
        server_port,
        server_host_uri,

        twitch_client_id: secrets.get("TWITCH_CLIENT_ID").unwrap().into(),
        twitch_client_secret: secrets.get("TWITCH_CLIENT_SECRET").unwrap().into(),

        twitch_callback_url,
    };
}
