use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio::runtime::Builder;
use tokio::sync::oneshot::Receiver;
use tokio::sync::{mpsc, oneshot};
use tokio::time;
use tokio::time::Duration;

use crate::app::App;
use crate::book::Book;
use crate::error::{Error, Result};
use crate::trades::Trade;

pub type Responder<T> = oneshot::Sender<Result<T>>;

#[derive(Debug)]
pub struct State {
    // Trade storage from trade channels to create tape. Only 100 trades are stored for each stream.
    // Trades are mapped to App trade struct preserving original precision. If larger trade hist
    // is needed use candles which will by default include tape
    pub tapes: Mutex<HashMap<Channel, VecDeque<Trade>>>,
    // Book storage for Bids / Asks and checksums
    pub books: Mutex<HashMap<Channel, Book>>,
    // Candle storage for trades and candles for a given base interval duration. Higher resolutions
    // can be resampled from the base interval.
    // candles: Mutex<HashMap<Channel, Candle>,
}

impl State {
    pub fn new() -> Self {
        Self {
            tapes: Mutex::new(HashMap::new()),
            books: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct BlockingClient {
    spawn: mpsc::UnboundedSender<ClientReq>,
}

impl BlockingClient {
    #[tracing::instrument]
    pub fn new() -> Self {
        tracing::info!("Creating new Client instance.");
        // Set up a channel for communicating to client -> forward to app
        let (send, mut recv) = mpsc::unbounded_channel();
        // Set up a channel for sending ws messages from socket to app
        let (ws_send, mut ws_recv) = mpsc::unbounded_channel();

        // Set up map for websockets
        let mut app = App::new(ws_send, None);

        // Build a new runtime for the new thread
        // The runtime is created before spawning the thread to more cleanly forward errors if the
        // .unwrap() panics.
        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        std::thread::spawn(move || {
            rt.block_on(async move {
                let mut interval = time::interval(Duration::from_secs(15));
                loop {
                    tokio::select! {
                        req = recv.recv() => {
                            if let Some(r) = req {
                                app.handle_client_req(r).await;
                            }
                        }
                        msg = ws_recv.recv() => {
                            if let Some(m) = msg {
                                app.handle_ws_msg(m).await;
                            }
                        }
                        _ = interval.tick() => (),
                    }
                }
                // Once all senders have gone out of scope,
                // the `.recv()` call returns None and it will
                // exit from the while loop and shut down the
                // thread.
            });
        });

        Self { spawn: send }
    }

    fn request<T>(&self, req: ClientReq, resp_rx: Receiver<Result<T>>) -> Result<T> {
        match self.spawn.send(req) {
            Ok(_) => resp_rx.blocking_recv()?,
            Err(_) => Err(Error::UnexpectedShutdown),
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn start_and_subscribe(&self, channel: Channel) -> Result<()> {
        tracing::info!("Starting socket with channel subscription.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = ClientReq::Start {
            channel,
            resp: Some(resp_tx),
        };
        self.request(req, resp_rx)
    }

    #[tracing::instrument(skip(self))]
    pub fn stop_and_unsubscribe(&self, channel: Channel) -> Result<()> {
        tracing::info!("Stopping socket with channel subscription.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = ClientReq::Stop {
            channel,
            resp: Some(resp_tx),
        };
        self.request(req, resp_rx)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_tape(&self, channel: Channel) -> Result<VecDeque<Trade>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = ClientReq::Tape {
            channel,
            resp: Some(resp_tx),
        };
        self.request(req, resp_rx)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_book(&self, channel: Channel) -> Result<Book> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = ClientReq::Book {
            channel,
            resp: Some(resp_tx),
        };
        self.request(req, resp_rx)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_last(&self, channel: Channel) -> Result<DateTime<Utc>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = ClientReq::Last {
            channel,
            resp: Some(resp_tx),
        };
        self.request(req, resp_rx)
    }
}

#[derive(Debug)]
pub struct AsyncClient {
    pub spawn: mpsc::UnboundedSender<ClientReq>,
    // All client requests responses are sent here. Handling not covered in lib.
    pub receiver: mpsc::UnboundedReceiver<Result<ClientRespMsg>>,
}

impl AsyncClient {
    #[tracing::instrument]
    pub fn new() -> Self {
        tracing::info!("Creating new Client instance.");
        // Set up a channel for communicating to client -> forward to app
        let (send, mut recv) = mpsc::unbounded_channel();
        // Set up a channel for sending ws messages from socket to app
        let (ws_send, mut ws_recv) = mpsc::unbounded_channel();
        // Set up a channel for sending messages to app via client and receive from app
        let (app_send, app_recv) = mpsc::unbounded_channel();

        // Set up map for websockets
        let mut app = App::new(ws_send, Some(app_send));

        // Build a new runtime for the new thread
        // The runtime is created before spawning the thread to more cleanly forward errors if the
        // .unwrap() panics.
        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        std::thread::spawn(move || {
            rt.block_on(async move {
                let mut interval = time::interval(Duration::from_secs(15));
                loop {
                    tokio::select! {
                        req = recv.recv() => {
                            if let Some(r) = req {
                                app.handle_client_req(r).await;
                            }
                        }
                        msg = ws_recv.recv() => {
                            if let Some(m) = msg {
                                app.handle_ws_msg(m).await;
                            }
                        }
                        _ = interval.tick() => (),
                    }
                }
                // Once all senders have gone out of scope,
                // the `.recv()` call returns None and it will
                // exit from the while loop and shut down the
                // thread.
            });
        });

        Self {
            spawn: send,
            receiver: app_recv,
        }
    }

    async fn request(&mut self, req: ClientReq) -> Result<()> {
        match self.spawn.send(req) {
            Ok(_) => Ok(()),
            Err(_) => Err(Error::UnexpectedShutdown),
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn start_and_subscribe(&mut self, channel: Channel) -> Result<()> {
        tracing::info!("Starting socket with channel subscription.");
        let req = ClientReq::Start {
            channel,
            resp: None,
        };
        self.request(req).await?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub async fn stop_and_unsubscribe(&mut self, channel: Channel) -> Result<()> {
        tracing::info!("Stopping socket with channel subscription.");
        let req = ClientReq::Stop {
            channel,
            resp: None,
        };
        self.request(req).await?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_tape(&mut self, channel: Channel) -> Result<()> {
        // tracing::info!("Getting tape for {:?}", channel);
        let req = ClientReq::Tape {
            channel,
            resp: None,
        };
        self.request(req).await?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_book(&mut self, channel: Channel) -> Result<()> {
        // tracing::info!("Getting book for {:?}", channel);
        let req = ClientReq::Book {
            channel,
            resp: None,
        };
        self.request(req).await?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_last(&mut self, channel: Channel) -> Result<()> {
        // tracing::info!("Getting book for {:?}", channel);
        let req = ClientReq::Last {
            channel,
            resp: None,
        };
        self.request(req).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ClientReq {
    Start {
        channel: Channel,
        resp: Option<Responder<()>>,
    },
    Stop {
        channel: Channel,
        resp: Option<Responder<()>>,
    },
    Tape {
        channel: Channel,
        resp: Option<Responder<VecDeque<Trade>>>,
    },
    Book {
        channel: Channel,
        resp: Option<Responder<Book>>,
    },
    Last {
        channel: Channel,
        resp: Option<Responder<DateTime<Utc>>>,
    },
}

#[derive(Debug)]
pub struct ClientRespMsg {
    pub channel: Channel,
    pub resp: ClientResp,
}

#[derive(Debug)]
pub enum ClientResp {
    Subscribed,
    Unsubscribed,
    Tape(VecDeque<Trade>),
    Book(Book),
    Last(DateTime<Utc>),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Exchange {
    Gdax,
    Kraken,
    Hyperliquid,
}

impl Exchange {
    pub fn as_display(&self) -> &'static str {
        match self {
            Exchange::Gdax => "Coinbase",
            Exchange::Kraken => "Kraken",
            Exchange::Hyperliquid => "Hyperliquid",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum ChannelType {
    Book,
    Tape,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Channel {
    pub exchange: Exchange,
    pub channel: ChannelType,
    pub market: String,
}

impl Channel {
    pub fn subscribe_message(&self) -> Value {
        match self.channel {
            ChannelType::Tape => self.subscribe_message_tape(),
            ChannelType::Book => self.subscribe_message_book(),
        }
    }

    pub fn subscribe_message_book(&self) -> Value {
        match self.exchange {
            Exchange::Gdax => {
                json!(
                {"type": "subscribe",
                "channels":
                    [{"name": "level2_batch",
                    "product_ids": [self.market]}
                    ]
                })
            }
            Exchange::Kraken => {
                json!({
                    "event": "subscribe",
                    "pair": [self.market],
                    "subscription": {
                        "name": "book",
                        "depth": 100
                    },
                })
            }
            Exchange::Hyperliquid => {
                json!({
                    "method": "subscribe", "subscription": {"type": "l2Book", "coin": self.market}
                })
            }
        }
    }

    pub fn subscribe_message_tape(&self) -> Value {
        match self.exchange {
            Exchange::Gdax => {
                json!(
                {"type": "subscribe",
                "channels":
                    [{"name": "ticker",
                    "product_ids": [self.market]}
                    ]
                })
            }
            Exchange::Kraken => {
                json!({
                    "event": "subscribe",
                    "pair": [self.market],
                    "subscription": {
                        "name": "trade",
                    },
                })
            }
            Exchange::Hyperliquid => {
                json!({
                    "method": "subscribe", "subscription": {"type": "trades", "coin": self.market}
                })
            }
        }
    }

    pub fn unsubscribe_message(&self) -> Value {
        match self.channel {
            ChannelType::Tape => self.unsubscribe_message_tape(),
            ChannelType::Book => self.unsubscribe_message_book(),
        }
    }

    pub fn unsubscribe_message_book(&self) -> Value {
        match self.exchange {
            Exchange::Gdax => {
                json!(
                {"type": "unsubscribe",
                "channels":
                    [{"name": "level2_batch",
                    "product_ids": [self.market]}
                    ]
                })
            }
            Exchange::Kraken => {
                json!({
                    "event": "subscribe",
                    "pair": [self.market],
                    "subscription": {
                        "name": "book",
                        "depth": 100
                    },
                })
            }
            Exchange::Hyperliquid => {
                json!({
                    "method": "subscribe", "subscription": {"type": "l2Book", "coin": self.market}
                })
            }
        }
    }

    pub fn unsubscribe_message_tape(&self) -> Value {
        match self.exchange {
            Exchange::Gdax => {
                json!(
                {"type": "unsubscribe",
                "channels":
                    [{"name": "ticker",
                    "product_ids": [self.market]}
                    ]
                })
            }
            Exchange::Kraken => {
                json!({
                    "event": "unsubscribe",
                    "pair": [self.market],
                    "subscription": {
                        "name": "trade",
                    },
                })
            }
            Exchange::Hyperliquid => {
                json!({
                    "method": "unsubscribe", "subscription": {"type": "trades", "coin": self.market}
                })
            }
        }
    }
}
