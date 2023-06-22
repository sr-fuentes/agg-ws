use tokio::time::Duration;

use agg_ws::client::{AsyncClient, Channel, ChannelType, ClientResp, ClientRespMsg, Exchange};

#[tokio::main]
async fn main() {
    better_panic::install();
    tracing_subscriber::fmt::init();

    let mut client = AsyncClient::new();

    let channel = Channel {
        exchange: Exchange::Gdax,
        channel: ChannelType::Tape,
        market: "BTC-USD".to_string(),
    };

    let req = client.start_and_subscribe(channel.clone()).await;
    tracing::info!("Subscribe request: {:?}", req);

    // Await next recv
    let resp = client.receiver.recv().await;
    tracing::info!("Subscribe response: {:?}", resp);

    for i in (1..4).rev() {
        tracing::info!("Receiving trades.. {} seconds remaining.", i * 5);
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let req = client.get_tape(channel.clone()).await;
    tracing::info!("Tape request: {:?}", req);

    // Await next recv
    let resp = client.receiver.recv().await;
    tracing::info!("Tape response: {:?}", resp);

    if let Some(Ok(ClientRespMsg { resp, .. })) = resp {
        if let ClientResp::Tape(t) = resp {
            for trade in t.iter().rev() {
                tracing::info!("{:?}", trade);
            }
        }
    }

    let req = client.stop_and_unsubscribe(channel.clone()).await;
    tracing::info!("Unsubscribe request: {:?}", req);
    // Await next recv
    let resp = client.receiver.recv().await;
    tracing::info!("Unsubscribe response: {:?}", resp);

    tokio::time::sleep(Duration::from_secs(2)).await;
}
