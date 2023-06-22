use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;

use crate::app::App;
use crate::client::{Channel, Exchange};
use crate::error::{Error, Result};
use crate::gdax::Ticker;
use crate::hyperliquid::Trade as HLTrade;
use crate::kraken::WsTrade;

#[derive(Debug, Clone)]
pub struct Trade {
    pub price: String,
    pub size: String,
    pub dt: DateTime<Utc>,
    pub exchange: Exchange,
}

impl TryFrom<Ticker> for Trade {
    type Error = Error;

    fn try_from(t: Ticker) -> Result<Self> {
        Ok(Self {
            price: t.price,
            size: t.size,
            dt: t.time,
            exchange: Exchange::Gdax,
        })
    }
}

impl TryFrom<WsTrade> for Trade {
    type Error = Error;

    fn try_from(t: WsTrade) -> Result<Self> {
        Ok(Self {
            price: t.price.to_string(),
            size: t.volume.to_string(),
            dt: Utc.timestamp_nanos((t.time * dec!(1000000000)).to_i64().unwrap()),
            exchange: Exchange::Kraken,
        })
    }
}

impl TryFrom<HLTrade> for Trade {
    type Error = Error;

    fn try_from(t: HLTrade) -> Result<Self> {
        Ok(Self {
            price: t.px,
            size: t.sz,
            dt: Utc.timestamp_millis_opt(t.time).unwrap(),
            exchange: Exchange::Hyperliquid,
        })
    }
}

impl App {
    #[tracing::instrument(skip(self))]
    pub async fn insert_trade(&mut self, channel: Channel, trade: Trade) -> Result<()> {
        let mut tapes = self.state.tapes.lock().unwrap();
        tapes.entry(channel).and_modify(|vd| {
            if vd.len() == vd.capacity() {
                vd.pop_front();
                vd.push_back(trade);
            } else {
                vd.push_back(trade);
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rust_decimal::prelude::*;
    use rust_decimal_macros::dec;

    #[test]
    pub fn convert_kraken_date() {
        let kraken_dt = dec!(1685895944.62050);
        let seconds = kraken_dt.trunc();
        let micros = kraken_dt.fract();
        let nanos = kraken_dt * dec!(1000000000);
        println!(
            "Dt: {:?}\nSeconds: {:?}\nMicros: {:?}",
            kraken_dt, seconds, micros
        );
        let dt = Utc.timestamp_nanos(nanos.to_i64().unwrap());
        println!("Dt: {:?}", dt);
    }

    #[test]
    pub fn convert_hyperliquid_date() {
        let hl_date = 1686270368980;
        let dt = Utc.timestamp_millis_opt(hl_date).unwrap();
        println!("Dt: {:?}", dt);
    }
}
