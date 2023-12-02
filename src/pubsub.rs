use crate::prelude::*;

use std::marker::PhantomData;

use futures_util::StreamExt;
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::{
    publisher::Publisher,
    subscription::{MessageStream, Subscription},
    topic::Topic,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct PubSubClients {
    pub player_actions: PubSubClient<PlayerAction>,
    pub host_actions: PubSubClient<HostAction>,
}

#[derive(Clone)]
pub struct PubSubClient<T> {
    topic: Topic,
    publisher: Publisher,
    subscription: Subscription,
    _t: PhantomData<T>,
}

impl Drop for PubSubClients {
    fn drop(&mut self) {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let _ = self.player_actions.publisher.shutdown().await;
                let _ = self.host_actions.publisher.shutdown().await;

                let _ = self.player_actions.subscription.delete(None).await;
                let _ = self.host_actions.subscription.delete(None).await;

                let _ = self.player_actions.topic.delete(None).await;
                let _ = self.host_actions.topic.delete(None).await;
            });
    }
}

pub struct DataStream<T> {
    unmapped: MessageStream,
    _t: PhantomData<T>,
}

impl<T> PubSubClient<T>
where
    T: TryInto<PubsubMessage, Error = crate::result::AppError>,
{
    pub fn new(topic: Topic, publisher: Publisher, subscription: Subscription) -> Self {
        return Self {
            topic,
            publisher,
            subscription,

            _t: PhantomData {},
        };
    }

    pub async fn publish(&self, data: T) -> Result {
        let _awaiter = self.publisher.publish(data.try_into()?).await;

        return Ok(());
    }

    pub async fn subscribe(&self) -> Result<DataStream<T>> {
        let stream = DataStream {
            unmapped: self.subscription.subscribe(None).await?,
            _t: PhantomData {},
        };

        return Ok(stream);
    }
}

impl<'a, T> DataStream<T>
where
    T: Clone + Deserialize<'a>,
{
    pub async fn next(&mut self) -> Option<T> {
        let message = self.unmapped.next().await?;
        let _ = message.ack().await;

        let leaked: &'a Vec<u8> = Box::leak(Box::new(message.message.data));

        let res: Result<T> = serde_json::from_slice(leaked).map_err(|e| e.into());

        let data = match res {
            Ok(data) => data.clone(),
            Err(e) => Err(e).expect("Unexpected error while parsing message"),
        };

        drop(unsafe { Box::from_raw((leaked as *const Vec<u8>) as *mut Vec<u8>) });

        return Some(data);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerAction {
    pub game_code: String,
    pub user_id: String,
    pub typ: PlayerActionType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlayerActionType {
    Join,
    Guess { item_id: u64 },
    ChangeGuess { from_item_id: u64, to_item_id: u64 },
    UndoGuess { item_id: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HostAction {
    pub game_code: String,
    pub typ: HostActionType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HostActionType {
    Lock,
    Unlock,
    Choose { item_id: u64 },
    Enable { item_id: u64 },
    Disable { item_id: u64 },
}

impl TryInto<PubsubMessage> for PlayerAction {
    type Error = crate::result::AppError;

    fn try_into(self) -> Result<PubsubMessage> {
        let data = serde_json::to_string(&self)?;

        return Ok(PubsubMessage {
            data: data.into(),
            ..Default::default()
        });
    }
}

impl TryInto<PubsubMessage> for HostAction {
    type Error = crate::result::AppError;

    fn try_into(self) -> Result<PubsubMessage> {
        let data = serde_json::to_string(&self)?;

        return Ok(PubsubMessage {
            data: data.into(),
            ..Default::default()
        });
    }
}
