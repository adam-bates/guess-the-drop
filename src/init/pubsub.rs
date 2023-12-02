use crate::{
    prelude::*,
    pubsub::{PubSubClient, PubSubClients},
};

use std::sync::Arc;

use google_cloud_googleapis::pubsub::v1::{ExpirationPolicy, PubsubMessage};
use google_cloud_pubsub::{
    client::{google_cloud_auth::credentials::CredentialsFile, Client, ClientConfig},
    subscription::SubscriptionConfig,
};

const TOPIC_FULLY_QUALIFIED_PREFIX: &str = "projects/guess-the-drop/topics";

const TOPIC_PLAYER_ACTIONS: &str = "player_actions";
const TOPIC_HOST_ACTIONS: &str = "host_actions";

pub async fn init_pubsub(cfg: &Config) -> Result<Arc<PubSubClients>> {
    let pubsub_creds = CredentialsFile::new_from_file(cfg.google_key_json_filepath.clone()).await?;

    let pubsub_config = ClientConfig::default()
        .with_credentials(pubsub_creds)
        .await?;

    let client = Client::new(pubsub_config).await?;

    let player_actions = init_action_client(&client, TOPIC_PLAYER_ACTIONS).await?;
    let host_actions = init_action_client(&client, TOPIC_HOST_ACTIONS).await?;

    return Ok(Arc::new(PubSubClients {
        player_actions,
        host_actions,
    }));
}

async fn init_action_client<T>(client: &Client, name: &str) -> Result<PubSubClient<T>>
where
    T: TryInto<PubsubMessage, Error = crate::result::AppError>,
{
    let topic = client.topic(name);

    if !topic.exists(None).await? {
        topic.create(None, None).await?;
    }

    let publisher = topic.new_publisher(None);

    let subscription = client.subscription(&format!("{name}-{}", EXECUTION_ID.as_str()));

    if !subscription.exists(None).await? {
        subscription
            .create(
                &format!("{}/{name}", TOPIC_FULLY_QUALIFIED_PREFIX),
                SubscriptionConfig {
                    expiration_policy: Some(ExpirationPolicy {
                        ttl: Some(prost_types::Duration {
                            seconds: 60 * 60 * 24, // 1 day (min)
                            nanos: 0,
                        }),
                    }),
                    ..Default::default()
                },
                None,
            )
            .await?;
    }

    return Ok(PubSubClient::new(topic, publisher, subscription));
}
