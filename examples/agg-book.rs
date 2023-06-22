use std::time::Duration;

use agg_ws::client::{BlockingClient, Channel, ChannelType, Exchange};

fn main() {
    better_panic::install();
    tracing_subscriber::fmt::init();

    let client = BlockingClient::new();

    let channel = Channel {
        exchange: Exchange::Hyperliquid,
        channel: ChannelType::Book,
        market: "BTC".to_string(),
    };

    let sub = client.start_and_subscribe(channel.clone());
    tracing::info!("Subscribe: {:?}", sub);

    for i in (1..2).rev() {
        tracing::info!("Receiving updates.. {} seconds remaining.", i * 5);
        std::thread::sleep(Duration::from_secs(5));
    }

    let book = client.get_book(channel.clone());

    if let Ok(b) = book {
        println!("Bids");
        for l in b.bids.iter().rev().take(10) {
            println!("{:?}", l);
        }
        println!("Asks");
        for l in b.asks.iter().take(10) {
            println!("{:?}", l);
        }
    }

    let unsub = client.stop_and_unsubscribe(channel.clone());
    tracing::info!("Unsubscribe: {:?}", unsub);
    std::thread::sleep(Duration::from_secs(2));
}
