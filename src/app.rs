use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use futures::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::book::Book;
use crate::client::{Channel, ChannelType, ClientReq, ClientResp, ClientRespMsg, Exchange, State};
use crate::error::{Error, Result};
use crate::websocket::Websocket;

/// App manages all Client requests, Websocket messages and data State. App is created during the
/// initialization of a new Client. App can be updated by receiving requests from the Client as well
/// as with any messages from the Websockets.
///
/// Examples of Client requests:
///
///      Subscribe - Subscribe to a new websocket channel. Data is sent from Websocket to Client where it is stored in the App.
///
///      Unsubscribe - Unsubscribe and drop an existing websocket channel.
///
///      Trades - Returns the last 100 trades sent from the given Websocket channel.
///
///      Trades Agg - Returns the last 100 aggregated trades sent from the given Websocket channels.
///
///
#[derive(Debug)]
pub struct App {
    // Map of all subscribed websockets. If channel exists in keys, the value contains
    // a live and subscribed websocket stream. If channel does not exists, websocket
    // was unsubscribed and dropped or has never been opened.
    pub sockets: Mutex<HashMap<Channel, Websocket>>,
    // Data storage from websockets. Trade streams are stored in the trades hashmap.
    // Books are stored in the books hashmap.
    pub state: Arc<State>,
    // Used to send messages from websockets to client runtime where they are processed
    // by the App. Clone and use in each new socket subscription.
    pub ws_sender: mpsc::UnboundedSender<(Channel, Result<Message>)>,
    // Queue for new subscription requests. Used to adhere to rate limits for subscriptions
    // imposed by exchanges. If enough time has lapsed since last sub and there is a sub
    // in the queue - client will process the subscription.
    pub sub_queue: HashMap<Exchange, HashSet<Channel>>,
    // Used to send responses from App back to async client
    pub app_sender: Option<mpsc::UnboundedSender<Result<ClientRespMsg>>>,
}

impl App {
    pub fn new(
        ws_sender: mpsc::UnboundedSender<(Channel, Result<Message>)>,
        app_sender: Option<mpsc::UnboundedSender<Result<ClientRespMsg>>>,
    ) -> Self {
        Self {
            sockets: Mutex::new(HashMap::new()),
            state: Arc::new(State::new()),
            ws_sender,
            sub_queue: HashMap::new(),
            app_sender,
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn handle_client_req(&mut self, req: ClientReq) {
        match req {
            ClientReq::Start { channel, resp, .. } => {
                // Create hashmap entry for the state
                let state_setup = match channel.channel {
                    ChannelType::Tape => {
                        let mut tapes = self.state.tapes.lock().unwrap();
                        if !tapes.contains_key(&channel) {
                            tapes.insert(channel.clone(), VecDeque::with_capacity(100));
                            Ok(())
                        } else {
                            Err(Error::ChannelAlreadySubscribed)
                        }
                    }
                    ChannelType::Book => {
                        let mut books = self.state.books.lock().unwrap();
                        if !books.contains_key(&channel) {
                            books.insert(channel.clone(), Book::new());
                            Ok(())
                        } else {
                            Err(Error::ChannelAlreadySubscribed)
                        }
                    }
                };
                let response = match state_setup {
                    Ok(_) => {
                        match Websocket::new(self.ws_sender.clone(), channel.clone()).await {
                            Ok(ws) => {
                                // Store the socket
                                tracing::info!("Websocket created for channel.");
                                let mut sockets = self.sockets.lock().unwrap();
                                sockets.insert(channel.clone(), ws);
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(e),
                };
                // Ignore errors - send response via oneshot or mpsc channel based on async or block
                match resp {
                    Some(r) => {
                        let _ = r.send(response);
                    }
                    None => {
                        let client_resp_msg = match response {
                            Ok(_) => Ok(ClientRespMsg {
                                channel,
                                resp: ClientResp::Subscribed,
                            }),
                            Err(e) => Err(e),
                        };
                        let _ = self.app_sender.as_ref().unwrap().send(client_resp_msg);
                    }
                }
            }
            ClientReq::Stop { channel, resp } => {
                let mut sockets = self.sockets.lock().unwrap();
                let socket = sockets.remove(&channel);
                let response = match socket {
                    Some(mut ws) => {
                        // Send unsub message
                        let unsub = channel.unsubscribe_message();
                        let _ = ws.write.send(Message::Text(unsub.to_string())).await;
                        // Send the kill shot to the socket
                        let _ = ws.killshot.send(true);
                        Ok(())
                    }
                    None => Err(Error::SocketDoesNotExist),
                };
                // Ignore errors - send response via oneshot or mpsc channel based on async or block
                match resp {
                    Some(r) => {
                        let _ = r.send(response);
                    }
                    None => {
                        let client_resp_msg = match response {
                            Ok(_) => Ok(ClientRespMsg {
                                channel,
                                resp: ClientResp::Unsubscribed,
                            }),
                            Err(e) => Err(e),
                        };
                        let _ = self.app_sender.as_ref().unwrap().send(client_resp_msg);
                    }
                }
            }
            ClientReq::Tape { channel, resp } => {
                let tapes = self.state.tapes.lock().unwrap();
                let tape = tapes.get(&channel);
                let response = match tape {
                    Some(t) => {
                        let t = t.clone();
                        Ok(t)
                    }
                    None => Err(Error::ChannelDoesNotExist),
                };
                // Ignore errors - send response via oneshot or mpsc channel based on async or block
                match resp {
                    Some(r) => {
                        let _ = r.send(response);
                    }
                    None => {
                        let client_resp_msg = match response {
                            Ok(trades) => Ok(ClientRespMsg {
                                channel,
                                resp: ClientResp::Tape(trades),
                            }),
                            Err(e) => Err(e),
                        };
                        let _ = self.app_sender.as_ref().unwrap().send(client_resp_msg);
                    }
                }
            }
            ClientReq::Book { channel, resp } => {
                let books = self.state.books.lock().unwrap();
                let book = books.get(&channel);
                let response = match book {
                    Some(b) => {
                        let b = b.clone();
                        Ok(b)
                    }
                    None => Err(Error::ChannelDoesNotExist),
                };
                match resp {
                    Some(r) => {
                        let _ = r.send(response);
                    }
                    None => {
                        let client_resp_msg = match response {
                            Ok(book) => Ok(ClientRespMsg {
                                channel,
                                resp: ClientResp::Book(book),
                            }),
                            Err(e) => Err(e),
                        };
                        let _ = self.app_sender.as_ref().unwrap().send(client_resp_msg);
                    }
                }
            }
            ClientReq::Last { channel, resp } => {
                let sockets = self.sockets.lock().unwrap();
                let response = match sockets.get(&channel) {
                    Some(ws) => Ok(ws.last_message),
                    None => Err(Error::SocketDoesNotExist),
                };
                match resp {
                    Some(r) => {
                        let _ = r.send(response);
                    }
                    None => {
                        let client_resp_msg = match response {
                            Ok(dt) => Ok(ClientRespMsg {
                                channel,
                                resp: ClientResp::Last(dt),
                            }),
                            Err(e) => Err(e),
                        };
                        let _ = self.app_sender.as_ref().unwrap().send(client_resp_msg);
                    }
                }
            }
        }
    }

    #[tracing::instrument(skip(self, msg))]
    pub async fn handle_ws_msg(&mut self, msg: (Channel, Result<Message>)) {
        let (channel, msg) = (msg.0, msg.1);
        tracing::info!("Msg: {:?}", msg);
        match channel.exchange {
            Exchange::Gdax => self
                .handle_ws_msg_gdax(channel, msg)
                .await
                .expect("Expected gdax msg handled."),
            Exchange::Kraken => self
                .handle_ws_msg_kraken(channel, msg)
                .await
                .expect("Expected kraken msg handled."),
            Exchange::Hyperliquid => self
                .handle_ws_msg_hyperliquid(channel, msg)
                .await
                .expect("Expect hyperliquid msg handled."),
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn update_last(&mut self, channel: Channel) -> Result<()> {
        let mut sockets = self.sockets.lock().unwrap();
        sockets.entry(channel).and_modify(|ws| {
            ws.last_message = Utc::now();
        });
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeSide {
    Buy,
    Sell,
}
