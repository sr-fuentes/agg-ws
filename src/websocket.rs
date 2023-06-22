use chrono::{DateTime, Utc};
use futures::SinkExt;
use tokio::net::TcpStream;
use tokio::runtime::Builder;
use tokio::sync::mpsc;
use tokio::time;
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

use crate::client::{Channel, Exchange};
use crate::error::{Error, Result};
use futures::{stream::SplitSink, StreamExt};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[derive(Debug)]
pub struct Websocket {
    pub write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    pub killshot: mpsc::UnboundedSender<bool>,
    pub last_message: DateTime<Utc>,
}

impl Websocket {
    pub async fn new(
        sender: mpsc::UnboundedSender<(Channel, Result<Message>)>,
        channel: Channel,
    ) -> Result<Self> {
        tracing::info!("Opening socket for {:?}", channel);
        let url = match channel.exchange {
            Exchange::Kraken => Url::parse("wss://ws.kraken.com").unwrap(),
            Exchange::Gdax => Url::parse("wss://ws-feed.pro.coinbase.com").unwrap(),
            Exchange::Hyperliquid => Url::parse("wss://api.hyperliquid.xyz/ws").unwrap(),
        };

        let (ws_stream, _) = connect_async(url).await?;

        let (mut write, mut read) = ws_stream.split();

        // Create oneshot channel to await shutdown message
        let (kill_tx, mut kill_rx) = mpsc::unbounded_channel();

        let sub = channel.subscribe_message();
        write.send(Message::Text(sub.to_string())).await?;

        // Build a new runtime for the new thread
        // The runtime is created before spawning the thread to more cleanly forward errors if the
        // .unwrap() panics.
        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        std::thread::spawn(move || {
            rt.block_on(async move {
                let mut interval = time::interval(Duration::from_secs(1));
                loop {
                    tokio::select! {
                        msg_resp = read.next() => {
                            match msg_resp {
                                Some(msg_opt) => {
                                    match msg_opt {
                                        Ok(msg) => {
                                            let _ = sender.send((channel.clone(), Ok(msg)));
                                        },
                                        Err(e) => {
                                            let _ = sender.send((channel.clone(), Err(Error::Tungstenite(e))));
                                        },
                                    };
                                }
                                None => {
                                    tracing::warn!("Channel websocket closed by exchange.");
                                    break;
                                }
                            }
                        }
                        Some(k) = kill_rx.recv() => {
                            if k {
                                tracing::info!("Killshot received. Dropping socket for channel: {:?}.", channel);
                                break;
                            } else {
                                tracing::error!("Killshot false.");
                            }
                        }
                        _ = interval.tick() => (),
                    }
                }
            });
        });

        Ok(Self {
            write,
            killshot: kill_tx,
            last_message: Utc::now(),
        })
    }
}
