mod initializer;

use crate::initializer::{Initializer, ROUTER};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use kaspa_p2p_lib::pb::kaspad_message::Payload;
use kaspa_p2p_lib::pb::{KaspadMessage, PingMessage, RequestAddressesMessage};
use kaspa_p2p_lib::{make_message, Hub, Router};
use kaspa_utils::hex::ToHex;
use kaspa_utils::networking::IpAddress;
use serde::Serialize;
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;

#[derive(Subcommand, Clone, Debug)]
enum RequestType {
    Version,
    Ping { nonce: u64 },
    Addresses,
}

#[derive(Parser)]
struct Cli {
    #[clap(short = 's', long, default_value = "localhost:16111", help = "The ip:port of a kaspad instance")]
    url: String,
    #[clap(short, long, default_value = "mainnet", help = "The network type and suffix, e.g. 'testnet-11'")]
    pub network: String,
    #[clap(subcommand)]
    pub request: RequestType,
}

#[derive(Eq, PartialEq, Hash, Serialize)]
struct Version {
    pub protocol_version: u32,
    pub network: String,
    pub services: u64,
    pub timestamp: Option<DateTime<Utc>>,
    pub address: Option<NetAddress>,
    pub id: String,
    pub user_agent: String,
    pub disable_relay_tx: bool,
    pub subnetwork_id: Option<String>,
}

#[derive(Eq, PartialEq, Hash, Serialize)]
struct Pong {
    nonce: u64,
}

#[derive(Eq, PartialEq, Hash, Serialize)]
struct NetAddress {
    ip: IpAddr,
    port: u16,
}

#[tokio::main]
async fn main() {
    let cli_args = Arc::new(Cli::parse());

    let (sender, mut receiver) = mpsc::channel(10);
    let initializer = Arc::new(Initializer::new(cli_args.clone(), sender));
    let adaptor = kaspa_p2p_lib::Adaptor::client_only(Hub::new(), initializer, Default::default());

    if let Err(e) = adaptor.connect_peer_with_retries(cli_args.url.clone(), 3, Duration::from_secs(1)).await {
        panic!("Failed to connect to {}: {:?}", cli_args.url, e);
    }

    let router = ROUTER.read().unwrap().clone().unwrap();

    match cli_args.request {
        RequestType::Version => req_version(&mut receiver).await,
        RequestType::Ping { nonce } => req_ping(&mut receiver, router, nonce).await,
        RequestType::Addresses => req_addresses(&mut receiver, router).await,
    }
    adaptor.terminate_all_peers().await;
}

async fn req_version(receiver: &mut Receiver<KaspadMessage>) {
    loop {
        if let Some(msg) = receiver.recv().await {
            if let Some(Payload::Version(version_msg)) = msg.payload {
                let version = Version {
                    protocol_version: version_msg.protocol_version,
                    network: version_msg.network,
                    services: version_msg.services,
                    timestamp: DateTime::from_timestamp_millis(version_msg.timestamp),
                    address: version_msg
                        .address
                        .and_then(|a| a.try_into().ok())
                        .map(|(ip, port)| NetAddress { ip: ip.to_canonical(), port }),
                    id: version_msg.id.to_hex(),
                    user_agent: version_msg.user_agent,
                    disable_relay_tx: version_msg.disable_relay_tx,
                    subnetwork_id: version_msg.subnetwork_id.map(|s| s.bytes.to_hex()),
                };
                let json = serde_json::to_string_pretty(&version).unwrap();
                println!("{}", json);
                break;
            }
        }
    }
}

async fn req_ping(receiver: &mut Receiver<KaspadMessage>, router: Arc<Router>, nonce: u64) {
    let _ = router.enqueue(make_message!(Payload::Ping, PingMessage { nonce })).await;

    loop {
        if let Some(msg) = receiver.recv().await {
            if let Some(Payload::Pong(pong_msg)) = msg.payload {
                let json = serde_json::to_string_pretty(&Pong { nonce: pong_msg.nonce }).unwrap();
                println!("{}", json);
                break;
            }
        }
    }
}

async fn req_addresses(receiver: &mut Receiver<KaspadMessage>, router: Arc<Router>) {
    let _ = router
        .enqueue(make_message!(
            Payload::RequestAddresses,
            RequestAddressesMessage { include_all_subnetworks: false, subnetwork_id: None }
        ))
        .await;

    loop {
        if let Some(msg) = receiver.recv().await {
            if let Some(Payload::Addresses(addresses_msg)) = msg.payload {
                let mut addresses = HashSet::new();
                for address in addresses_msg.address_list {
                    if let Ok(result) = address.try_into() {
                        let (ip, port): (IpAddress, u16) = result;
                        addresses.insert(NetAddress { ip: ip.to_canonical(), port });
                    }
                }
                let json = serde_json::to_string_pretty(&addresses).unwrap();
                println!("{}", json);
                break;
            }
        }
    }
}
