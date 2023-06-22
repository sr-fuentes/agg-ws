use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::{
    app::{App, TradeSide},
    client::Channel,
    gdax::{L2update, Snapshot as GdaxSnapshot},
    hyperliquid::L2Book,
    kraken::{L2updateAsk, L2updateBid, L2updateBoth, Snapshot as KrakenSnapshot},
};

#[derive(Debug, Clone)]
pub struct Book {
    pub bids: BTreeMap<Decimal, Decimal>,
    pub asks: BTreeMap<Decimal, Decimal>,
}

impl Book {
    pub fn new() -> Self {
        Book {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }
}

impl Default for Book {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub async fn insert_gdax_snapshot(&mut self, channel: Channel, snapshot: GdaxSnapshot) {
        let mut book = Book::new();
        book.bids.extend(snapshot.bids.into_iter());
        book.asks.extend(snapshot.asks.into_iter());
        let mut books = self.state.books.lock().unwrap();
        books.insert(channel, book);
    }

    pub async fn insert_gdax_l2update(&mut self, channel: Channel, l2update: L2update) {
        let mut books = self.state.books.lock().unwrap();
        for update in l2update.changes.iter() {
            match update.0 {
                TradeSide::Buy => {
                    if update.2 == Decimal::ZERO {
                        books.entry(channel.clone()).and_modify(|bt| {
                            bt.bids.remove(&update.1);
                        });
                    } else {
                        books.entry(channel.clone()).and_modify(|bt| {
                            bt.bids.insert(update.1, update.2);
                        });
                    };
                }
                TradeSide::Sell => {
                    if update.2 == Decimal::ZERO {
                        books.entry(channel.clone()).and_modify(|bt| {
                            bt.asks.remove(&update.1);
                        });
                    } else {
                        books.entry(channel.clone()).and_modify(|bt| {
                            bt.asks.insert(update.1, update.2);
                        });
                    }
                }
            }
        }
    }

    pub async fn insert_kraken_snapshot(&mut self, channel: Channel, snapshot: KrakenSnapshot) {
        let mut book = Book::new();
        book.bids
            .extend(snapshot.snapshot.bs.iter().map(|l| (l.price, l.volume)));
        book.asks
            .extend(snapshot.snapshot.r#as.iter().map(|l| (l.price, l.volume)));
        let mut books = self.state.books.lock().unwrap();
        books.insert(channel, book);
    }

    pub async fn insert_kraken_update_ask(&mut self, channel: Channel, update: L2updateAsk) {
        let mut books = self.state.books.lock().unwrap();
        for ask in update.ask.update.iter() {
            if ask.volume == Decimal::ZERO {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.asks.remove(&ask.price);
                });
            } else {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.asks.insert(ask.price, ask.volume);
                });
            };
        }
    }

    pub async fn insert_kraken_update_bid(&mut self, channel: Channel, update: L2updateBid) {
        let mut books = self.state.books.lock().unwrap();
        for bid in update.bid.update.iter() {
            if bid.volume == Decimal::ZERO {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.bids.remove(&bid.price);
                });
            } else {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.bids.insert(bid.price, bid.volume);
                });
            };
        }
    }

    pub async fn insert_kraken_update_both(&mut self, channel: Channel, update: L2updateBoth) {
        let mut books = self.state.books.lock().unwrap();
        for bid in update.bid.update.iter() {
            if bid.volume == Decimal::ZERO {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.bids.remove(&bid.price);
                });
            } else {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.bids.insert(bid.price, bid.volume);
                });
            };
        }
        for ask in update.ask.update.iter() {
            if ask.volume == Decimal::ZERO {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.asks.remove(&ask.price);
                });
            } else {
                books.entry(channel.clone()).and_modify(|bt| {
                    bt.asks.insert(ask.price, ask.volume);
                });
            };
        }
    }

    pub async fn insert_hyperliquid_snapshot(&mut self, channel: Channel, snapshot: L2Book) {
        let mut book = Book::new();
        book.bids
            .extend(snapshot.levels.bids.iter().map(|l| (l.px, l.sz)));
        book.asks
            .extend(snapshot.levels.asks.iter().map(|l| (l.px, l.sz)));
        let mut books = self.state.books.lock().unwrap();
        books.insert(channel, book);
    }
}
