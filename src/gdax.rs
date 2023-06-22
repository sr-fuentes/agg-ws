use crate::{
    app::{App, TradeSide},
    client::{Channel, ChannelType},
    error::{Error, Result},
    trades::Trade,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Subscriptions(Subscriptions),
    Heartbeat(Heartbeat),
    Ticker(Ticker),
    Snapshot(Snapshot),
    L2update(L2update),
}

/// Struct mapping for:
///
/// Subscriptions message from Coinbase Pro
/// {
/// "type": "subscriptions",
/// "channels": [
///     {
///         "name": "level2",
///         "product_ids": [
///             "ETH-USD",
///             "ETH-EUR"
///         ],
///     },
///     {
///         "name": "heartbeat",
///         "product_ids": [
///             "ETH-USD",
///             "ETH-EUR"
///         ],
///     },
///     {
///         "name": "ticker",
///         "product_ids": [
///             "ETH-USD",
///             "ETH-EUR",
///             "ETH-BTC"
///         ]
///     }
/// ]
/// }
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Subscriptions {
    pub channels: Vec<GdaxChannel>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct GdaxChannel {
    pub name: String,
    pub product_ids: Vec<String>,
}

/// Struct mapping for:
///
/// Heartbeat message from Coinbase Pro
/// {
///     "type": "heartbeat",
///     "sequence": 90,
///     "last_trade_id": 20,
///     "product_id": "BTC-USD",
///     "time": "2014-11-07T08:19:28.464459Z"
/// }
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Heartbeat {
    pub time: DateTime<Utc>,
    pub product_id: String,
    pub sequence: i64,
    pub last_trade_id: i64,
}

/// Struct mapping for:
///
/// Ticker messsage from Coinbase Pro
/// {
///     "type": "ticker",
///     "sequence": 37475248783,
///     "product_id": "ETH-USD",
///     "price": "1285.22",
///     "open_24h": "1310.79",
///     "volume_24h": "245532.79269678",
///     "low_24h": "1280.52",
///     "high_24h": "1313.8",
///     "volume_30d": "9788783.60117027",
///     "best_bid": "1285.04",
///     "best_bid_size": "0.46688654",
///     "best_ask": "1285.27",
///     "best_ask_size": "1.56637040",
///     "side": "buy",
///     "time": "2022-10-19T23:28:22.061769Z",
///     "trade_id": 370843401,
///     "last_size": "11.4396987"
/// }
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Ticker {
    pub sequence: u64,
    pub product_id: String,
    pub price: String,
    pub side: TradeSide,
    pub time: DateTime<Utc>,
    #[serde(alias = "last_size")]
    pub size: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Snapshot {
    pub product_id: String,
    pub bids: Vec<(Decimal, Decimal)>,
    pub asks: Vec<(Decimal, Decimal)>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct L2update {
    pub product_id: String,
    pub time: DateTime<Utc>,
    pub changes: Vec<(TradeSide, Decimal, Decimal)>,
}

impl App {
    #[tracing::instrument(skip(self, msg))]
    pub async fn handle_ws_msg_gdax(
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
                    tracing::info!("{:?}", response);
                    self.handle_ws_response_gdax(channel.clone(), response)
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
    pub async fn handle_ws_response_gdax(
        &mut self,
        channel: Channel,
        response: Response,
    ) -> Result<()> {
        match response {
            Response::Heartbeat(_) => {}
            Response::Subscriptions(_) => {}
            Response::Ticker(ticker) => {
                if channel.channel == ChannelType::Tape {
                    // Convert gdax ticker to trade and insert into trades state
                    let trade: Trade = ticker.try_into()?;
                    tracing::info!("Inserting: {:?}", trade);
                    self.insert_trade(channel, trade).await?;
                    tracing::info!("Inserted.");
                } else {
                    // Ticker message sent on a none tape channel
                    tracing::error!("Ticker message {:?} sent on channel {:?}", ticker, channel);
                    return Err(Error::ChannelResponseMismatch);
                }
            }
            Response::Snapshot(snapshot) => self.insert_gdax_snapshot(channel, snapshot).await,
            Response::L2update(l2update) => self.insert_gdax_l2update(channel, l2update).await,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Result, Value};

    use crate::gdax::{Response, Subscriptions};

    #[test]
    pub fn deserialize_sub() -> Result<()> {
        let data = r#"
        {
            "type":"subscriptions",
            "channels":
                [
                    {"name":"ticker","product_ids":["BTC-USD"]}
                ]
        }
        "#;

        let value = data.clone();
        let v: Value = serde_json::from_str(value)?;
        println!("Value: {:?}", v);

        let subscribe = data.clone();
        let v: Subscriptions = serde_json::from_str(subscribe)?;
        println!("Subscribe: {:?}", v);

        // let response = data.clone();
        // let v: Response = serde_json::from_str(response)?;
        // println!("Response: {:?}", v);

        let response_enum = data.clone();
        let v: Response = serde_json::from_str(response_enum)?;
        println!("Response: {:?}", v);

        Ok(())
    }
}
