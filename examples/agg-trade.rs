use std::time::Duration;

use agg_ws::client::{BlockingClient, Channel, ChannelType, Exchange};

fn main() {
    better_panic::install();
    tracing_subscriber::fmt::init();

    let client = BlockingClient::new();

    let channel = Channel {
        exchange: Exchange::Hyperliquid,
        channel: ChannelType::Tape,
        market: "BTC".to_string(),
    };

    let sub = client.start_and_subscribe(channel.clone());
    tracing::info!("Subscribe: {:?}", sub);

    for i in (1..4).rev() {
        tracing::info!("Receiving trades.. {} seconds remaining.", i * 5);
        std::thread::sleep(Duration::from_secs(5));
    }

    let tape = client.get_tape(channel.clone());

    if let Ok(t) = tape {
        for trade in t.iter().rev() {
            tracing::info!("{:?}", trade);
        }
    }

    let unsub = client.stop_and_unsubscribe(channel.clone());
    tracing::info!("Unsubscribe: {:?}", unsub);
    std::thread::sleep(Duration::from_secs(2));
}
