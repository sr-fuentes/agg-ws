use rust_decimal::Decimal;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;

use crate::{
    app::App,
    client::{Channel, ChannelType},
    error::{Error, Result},
    trades::Trade as AppTrade,
};

#[derive(Clone, Deserialize, Debug)]
#[serde(tag = "channel", content = "data", rename_all = "camelCase")]
pub enum Response {
    SubscriptionResponse(Subscribe),
    Trades(Vec<Trade>),
    L2Book(L2Book),
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Subscribe {
    pub method: String,
    pub subscription: Subscription,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub r#type: String,
    pub coin: String,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub coin: String,
    pub side: String,
    pub px: String,
    pub sz: String,
    pub time: i64,
    pub hash: String,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct L2Book {
    pub coin: String,
    pub time: i64,
    pub levels: Levels,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Levels {
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub n: i8,
    pub px: Decimal,
    pub sz: Decimal,
}

impl App {
    #[tracing::instrument(skip(self, msg))]
    pub async fn handle_ws_msg_hyperliquid(
        &mut self,
        channel: Channel,
        msg: Result<Message>,
    ) -> Result<()> {
        match msg {
            Ok(m) => {
                // Update socket last message
                self.update_last(channel.clone())?;
                // Parse message
                tracing::debug!("Message: {:?}", m);
                if let Message::Text(text) = m {
                    if text == "Websocket connection established." {
                        tracing::debug!("Text: {:?}", text);
                        return Ok(());
                    }
                    let response: Response = match serde_json::from_str(&text) {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::error!("Could not parse messages {:?}", text);
                            tracing::error!("Error: {:?}", e);
                            return Err(Error::Serde(e));
                        }
                    };
                    tracing::debug!("Response: {:?}", response);
                    self.handle_ws_response_hyperliquid(channel.clone(), response)
                        .await?;
                } else {
                    tracing::warn!("Non-text Message: {:?}", m);
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
    pub async fn handle_ws_response_hyperliquid(
        &mut self,
        channel: Channel,
        response: Response,
    ) -> Result<()> {
        match response {
            Response::Trades(trades) => {
                if channel.channel == ChannelType::Tape {
                    // Convert to app trade and insert into state
                    for trade in trades.into_iter() {
                        tracing::debug!("Converting {:?}", trade);
                        let t: AppTrade = trade.try_into()?;
                        tracing::debug!("Inserting {:?}", t);
                        self.insert_trade(channel.clone(), t).await?;
                    }
                } else {
                    // Ticker message sent on a none tape channel
                    tracing::warn!("Trade message {:?} sent on channel {:?}", trades, channel);
                    return Err(Error::ChannelResponseMismatch);
                }
            }
            Response::L2Book(book) => {
                self.insert_hyperliquid_snapshot(channel, book).await;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Result, Value};

    use crate::hyperliquid::Response;

    pub fn messages(s: String) -> String {
        let sub = "{\"channel\":\"subscriptionResponse\",\"data\":{\"subscription\":{\"type\":\"trades\",\"coin\":\"BTC\"},\"method\":\"subscribe\"}}";
        let trades = "{\"channel\":\"trades\",\"data\":[{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26433.0\",\"sz\":\"0.03019\",\"time\":1686270879026,\"hash\":\"0x92c994c577ae6997692104025ad505012a006e4cca395d87c41cccc763aa215e\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26433.0\",\"sz\":\"0.02876\",\"time\":1686270879026,\"hash\":\"0x92c994c577ae6997692104025ad505012a006e4cca395d87c41cccc763aa215e\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26437.0\",\"sz\":\"0.03067\",\"time\":1686270876527,\"hash\":\"0xf7699d8eb8f5c19f79bb04025ad4fb012c00c61407da9739a2320e555f052171\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26437.0\",\"sz\":\"0.02822\",\"time\":1686270876527,\"hash\":\"0xf7699d8eb8f5c19f79bb04025ad4fb012c00c61407da9739a2320e555f052171\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26443.0\",\"sz\":\"0.03\",\"time\":1686270832485,\"hash\":\"0x3c2ff5514bfa9f7c0e5104025ad423015100f3d0fe4620f634fd3a6c372ac1c2\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26443.0\",\"sz\":\"0.02905\",\"time\":1686270832485,\"hash\":\"0x3c2ff5514bfa9f7c0e5104025ad423015100f3d0fe4620f634fd3a6c372ac1c2\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26445.0\",\"sz\":\"0.03112\",\"time\":1686270605587,\"hash\":\"0xa5d80e826f5416f9fef004025acf8b011000a3bda51f21e1aaa65a78a58b8b16\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26445.0\",\"sz\":\"0.03078\",\"time\":1686270605587,\"hash\":\"0xa5d80e826f5416f9fef004025acf8b011000a3bda51f21e1aaa65a78a58b8b16\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26424.0\",\"sz\":\"0.2318\",\"time\":1686270370783,\"hash\":\"0x5bf314d7b7eef816239c04025acab7013e009167dad1d23805d5f65d4eb9b486\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26424.0\",\"sz\":\"0.63384\",\"time\":1686270370339,\"hash\":\"0xbcc25b36b8a163220d7904025acab601a7007f2c9550662ea436860b5ce63b70\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26424.0\",\"sz\":\"0.05756\",\"time\":1686270370339,\"hash\":\"0xbcc25b36b8a163220d7904025acab601a7007f2c9550662ea436860b5ce63b70\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26424.0\",\"sz\":\"0.39899\",\"time\":1686270370339,\"hash\":\"0xb6202b23f385d245d71804025acab601a9001d215fd407bd6dfc5272803fb7d4\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26428.0\",\"sz\":\"0.28195\",\"time\":1686270369880,\"hash\":\"0x042b64021c53c26880a404025acab5014100de4fd7b2cb867043128c8a5ba979\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26427.0\",\"sz\":\"0.02776\",\"time\":1686270369880,\"hash\":\"0x042b64021c53c26880a404025acab5014100de4fd7b2cb867043128c8a5ba979\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26428.0\",\"sz\":\"0.48607\",\"time\":1686270369432,\"hash\":\"0x49361d2ee405efa558f504025acab4015a0010819d2f6af6e227a23bd8a1d5ed\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26428.0\",\"sz\":\"0.22225\",\"time\":1686270369432,\"hash\":\"0x8c258b0fcfe87a7d270604025acab4015d002c337099ff44cf2c6ae003d1e7eb\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26428.0\",\"sz\":\"0.3813\",\"time\":1686270369432,\"hash\":\"0x8c258b0fcfe87a7d270604025acab4015d002c337099ff44cf2c6ae003d1e7eb\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26431.0\",\"sz\":\"0.02737\",\"time\":1686270368980,\"hash\":\"0x80450b02ea566746749004025acab301270000e6b1f502d43c8597452ce97d52\"},{\"coin\":\"BTC\",\"side\":\"A\",\"px\":\"26432.0\",\"sz\":\"0.03267\",\"time\":1686270368525,\"hash\":\"0xbdb6cd669293450cea7604025acab201b600ad4189c7228d764851f637c75fa9\"}]}";
        let connection = "Websocket connection established.";
        let book = "{\"channel\":\"l2Book\",\"data\":{\"coin\":\"BTC\",\"time\":1686537736732,\"levels\":[[{\"px\":\"25748.0\",\"sz\":\"0.07332\",\"n\":2},{\"px\":\"25745.0\",\"sz\":\"1.58759\",\"n\":2},{\"px\":\"25741.0\",\"sz\":\"1.50368\",\"n\":2},{\"px\":\"25738.0\",\"sz\":\"0.71586\",\"n\":1},{\"px\":\"25736.0\",\"sz\":\"0.71842\",\"n\":1},{\"px\":\"25727.0\",\"sz\":\"1.61872\",\"n\":2},{\"px\":\"25714.0\",\"sz\":\"0.56045\",\"n\":1},{\"px\":\"25709.0\",\"sz\":\"0.53767\",\"n\":1},{\"px\":\"25705.0\",\"sz\":\"0.60063\",\"n\":1},{\"px\":\"25563.0\",\"sz\":\"0.57734\",\"n\":1},{\"px\":\"25562.0\",\"sz\":\"0.53548\",\"n\":1},{\"px\":\"25547.0\",\"sz\":\"0.60878\",\"n\":1},{\"px\":\"25508.0\",\"sz\":\"0.54988\",\"n\":1},{\"px\":\"25493.0\",\"sz\":\"0.55266\",\"n\":1},{\"px\":\"25492.0\",\"sz\":\"0.63969\",\"n\":1},{\"px\":\"25346.0\",\"sz\":\"0.63694\",\"n\":1},{\"px\":\"25345.0\",\"sz\":\"0.54101\",\"n\":1},{\"px\":\"25188.0\",\"sz\":\"0.62759\",\"n\":1},{\"px\":\"25187.0\",\"sz\":\"0.63564\",\"n\":1}],[{\"px\":\"25775.0\",\"sz\":\"0.03637\",\"n\":2},{\"px\":\"25797.0\",\"sz\":\"0.439\",\"n\":1},{\"px\":\"25801.0\",\"sz\":\"0.38807\",\"n\":1},{\"px\":\"25842.0\",\"sz\":\"0.4017\",\"n\":1},{\"px\":\"25860.0\",\"sz\":\"0.4169\",\"n\":1},{\"px\":\"25934.0\",\"sz\":\"0.58648\",\"n\":1},{\"px\":\"25935.0\",\"sz\":\"0.63012\",\"n\":1},{\"px\":\"25938.0\",\"sz\":\"0.4198\",\"n\":1},{\"px\":\"25950.0\",\"sz\":\"0.41782\",\"n\":1},{\"px\":\"25960.0\",\"sz\":\"0.58504\",\"n\":1},{\"px\":\"25977.0\",\"sz\":\"0.56065\",\"n\":1},{\"px\":\"26017.0\",\"sz\":\"0.57934\",\"n\":1},{\"px\":\"26024.0\",\"sz\":\"0.52938\",\"n\":1},{\"px\":\"26079.0\",\"sz\":\"0.57799\",\"n\":1},{\"px\":\"26082.0\",\"sz\":\"0.62211\",\"n\":1},{\"px\":\"26159.0\",\"sz\":\"0.56783\",\"n\":1},{\"px\":\"26217.0\",\"sz\":\"0.54541\",\"n\":1},{\"px\":\"26276.0\",\"sz\":\"0.54991\",\"n\":1},{\"px\":\"26277.0\",\"sz\":\"0.62702\",\"n\":1}]]}}";
        if s == "subscription".to_string() {
            sub.to_string()
        } else if s == "trades".to_string() {
            trades.to_string()
        } else if s == "connection".to_string() {
            connection.to_string()
        } else if s == "book".to_string() {
            book.to_string()
        } else {
            "none".to_string()
        }
    }

    #[test]
    pub fn deserialize_sub() -> Result<()> {
        let data = messages("subscription".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);
        Ok(())
    }

    #[test]
    pub fn deserialize_trades() -> Result<()> {
        let data = messages("trades".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);
        Ok(())
    }

    #[test]
    pub fn deserialize_connection() -> Result<()> {
        let data = messages("connection".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);
        Ok(())
    }

    #[test]
    pub fn deserialize_book() -> Result<()> {
        let data = messages("book".to_string());

        let v: Value = serde_json::from_str(&data)?;
        println!("Value: {:?}", v);

        let v: Response = serde_json::from_str(&data)?;
        println!("Response: {:?}", v);
        Ok(())
    }
}
