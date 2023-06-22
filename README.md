# agg-ws
Async and Sync Websocket client for connecting to multiple cryptocurrency exchanges.

## Installation

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone repo
git clone https://github.com/sr-fuentes/agg-ws.git

# Run example
cd agg-ws
cargo run --example agg-trade
```

## Usage

Create client to suit the application runtime

```rust
# Blocking Client
let client = BlockingClient::new();

# Async Client
let client = AsyncClient::new();
```

Subscribe to channel

```rust
# agg-ws opens a new websocket connection for each channel. 
# Channels are defined by the Channel struct containing the exchange, feed and market.
let channel = Channel {
    exchange: Exchange::Kraken, # Enum. See full list of supported exchanges in client.rs
    channel: ChannelType::Tape, # Enum. Tape and Book currently supported
    market: "SOL/USD".to_string(), # String. Market ticker for exchange
}

# Blocking Client
let _ = client.start_and_subscribe(channel);

# Async Client
let _ = client.start_and_subscribe(channel).await;

```



Query client for websocket data

```rust
# Blocking Client
let tape = client.get_tape(channel);

if let Ok(t) = tape {
    for trade in t.iter().rev() {
        println!("{:?}", trade);
    }
}
    
# Async Client
let _ = client.get_tape(channel).await;
let resp = client.receiver.recv().await;
    
if let Some(Ok(ClientRespMsg { resp, .. })) = resp {
    if let ClientResp::Tape(t) = resp {
        for trade in t.iter().rev() {
            println!("{:?}", trade);
        }
    }
}    
```

Full examples can be found in the examples directory.

## Sample Application

[agg-ws-term](https://github.com/sr-fuentes/agg-ws-term) is a terminal ui application that uses agg-ws to subscribe and display trade history and book updates.

<img width="1182" alt="agg-ws-term" src="https://github.com/sr-fuentes/agg-ws/assets/29989568/c98f3b1c-5e24-4180-8eff-bc4a51695faf">

