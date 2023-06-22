use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    app::App,
    client::{Channel, ChannelType},
    error::{Error, Result},
    trades::Trade as AppTrade,
};

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(untagged, rename_all = "snake_case")]
pub enum Response {
    TaggedResp(TaggedResp),
    Trade(Trade),
    Snapshot(Snapshot),
    L2updateAsk(L2updateAsk),
    L2updateBid(L2updateBid),
    L2updateBoth(L2updateBoth),
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum TaggedResp {
    Heartbeat,
    SystemStatus(SystemStatus),
    SubscriptionStatus(SubscriptionStatus),
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Subscription {
    pub name: String,
    pub depth: Option<i64>,
    pub interval: Option<i64>,
    pub ratecounter: Option<bool>,
    pub snapshot: Option<bool>,
    pub token: Option<String>,
    pub consolidate_taker: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionStatus {
    pub channel_name: String,
    pub pair: Option<String>,
    pub reqid: Option<i64>,
    pub status: String,
    pub subscription: Subscription,
    pub error_message: Option<String>,
    pub channel_id: Option<i32>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(tag = "event", rename_all = "snake_case")]
pub struct SystemStatus {
    pub connection_id: Option<i64>,
    pub status: String,
    pub version: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Trade {
    pub channel_id: i32,
    pub trades: Vec<WsTrade>,
    pub channel_name: String,
    pub pair: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct WsTrade {
    pub price: Decimal,
    pub volume: Decimal,
    pub time: Decimal,
    pub side: String,
    pub order_type: String,
    pub misc: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Snapshot {
    pub channel_id: i32,
    pub snapshot: BidAsks,
    pub channel_name: String,
    pub pair: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct BidAsks {
    pub r#as: Vec<Level>,
    pub bs: Vec<Level>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct L2updateAsk {
    pub channel_id: i32,
    pub ask: Asks,
    pub channel_name: String,
    pub pair: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct L2updateBid {
    pub channel_id: i32,
    pub bid: Bids,
    pub channel_name: String,
    pub pair: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct L2updateBoth {
    pub channel_id: i32,
    pub ask: Asks,
    pub bid: Bids,
    pub channel_name: String,
    pub pair: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Asks {
    #[serde(alias = "a")]
    pub update: Vec<Level>,
    pub c: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Bids {
    #[serde(alias = "b")]
    pub update: Vec<Level>,
    pub c: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Level {
    pub price: Decimal,
    pub volume: Decimal,
    pub timestamp: Decimal,
    #[serde(default)]
    pub update_type: Option<String>,
}

impl App {
    #[tracing::instrument(skip(self, msg))]
    pub async fn handle_ws_msg_kraken(
        &mut self,
        channel: Channel,
        msg: Result<Message>,
    ) -> Result<()> {
        match msg {
            Ok(m) => {
                // Update socket last message
                self.update_last(channel.clone())?;
                // Parse message
                if let Message::Text(text) = m {
                    let response: Response = match serde_json::from_str(&text) {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::error!("Could not parse message {:?}", text);
                            tracing::error!("Error: {:?}", e);
                            return Err(Error::Serde(e));
                        }
                    };
                    tracing::debug!("{:?}", response);
                    self.handle_ws_response_kraken(channel.clone(), response)
                        .await?;
                } else {
                    tracing::warn!("Non-Text Message: {:?}", m);
                }
                Ok(())
            }
            Err(e) => {
                // Return Err
                tracing::error!("Error: {:?}", e);
                Err(e)
            }
        }
    }

    #[tracing::instrument(skip(self, response))]
    pub async fn handle_ws_response_kraken(
        &mut self,
        channel: Channel,
        response: Response,
    ) -> Result<()> {
        match response {
            Response::Trade(trade) => {
                if channel.channel == ChannelType::Tape {
                    // Convert kraken trade to trade and insert into trades state
                    for t in trade.trades.into_iter() {
                        let trade: AppTrade = t.try_into()?;
                        self.insert_trade(channel.clone(), trade).await?;
                    }
                } else {
                    // Ticker message sent on a none tape channel
                    tracing::error!("Trade message {:?} sent on channel {:?}", trade, channel);
                    return Err(Error::ChannelResponseMismatch);
                }
            }
            Response::Snapshot(snapshot) => self.insert_kraken_snapshot(channel, snapshot).await,
            Response::L2updateAsk(update) => self.insert_kraken_update_ask(channel, update).await,
            Response::L2updateBid(update) => self.insert_kraken_update_bid(channel, update).await,
            Response::L2updateBoth(update) => self.insert_kraken_update_both(channel, update).await,
            Response::TaggedResp(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Result, Value};

    use crate::kraken::Response;

    pub fn messages(s: String) -> String {
        let system_status = "{\"connectionID\":7697072686821276634,\"event\":\"systemStatus\",\"status\":\"online\",\"version\":\"1.9.1\"}";
        let subscription_status = "{\"channelID\":337,\"channelName\":\"trade\",\"event\":\"subscriptionStatus\",\"pair\":\"XBT/USD\",\"status\":\"subscribed\",\"subscription\":{\"name\":\"trade\"}}";
        let heartbeat = "{\"event\":\"heartbeat\"}";
        let update = "[336,{\"a\":[[\"25782.90000\",\"1.17100399\",\"1686499924.936167\"]],\"c\":\"3184832790\"},\"book-100\",\"XBT/USD\"]";
        if s == "system_status".to_string() {
            system_status.to_string()
        } else if s == "heartbeat" {
            heartbeat.to_string()
        } else if s == "subscription_status".to_string() {
            subscription_status.to_string()
        } else if s == "update".to_string() {
            update.to_string()
        } else {
            "none".to_string()
        }
    }

    #[test]
    pub fn deserialize_system_status() -> Result<()> {
        let data = messages("system_status".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);

        Ok(())
    }

    #[test]
    pub fn deserialize_sub_status() -> Result<()> {
        let data = messages("subscription_status".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);

        Ok(())
    }

    #[test]
    pub fn deserialize_heartbeat() -> Result<()> {
        let data = messages("heartbeat".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);

        Ok(())
    }

    #[test]
    pub fn deserialize_update() -> Result<()> {
        let data = messages("update".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);

        Ok(())
    }
}
