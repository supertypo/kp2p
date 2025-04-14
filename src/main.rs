mod initializer;

use crate::initializer::{Initializer, ROUTER};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use kaspa_addresses::Prefix;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash as KaspaHash;
use kaspa_p2p_lib::pb::kaspad_message::Payload;
use kaspa_p2p_lib::pb::{
    AddressesMessage, Hash, KaspadMessage, PingMessage, PongMessage, RequestAddressesMessage,
    RequestNextPruningPointUtxoSetChunkMessage, RequestPruningPointUtxoSetMessage,
};
use kaspa_p2p_lib::{make_message, Hub, Router};
use kaspa_txscript::extract_script_pub_key_address;
use kaspa_utils::hex::ToHex;
use kaspa_utils::networking::IpAddress;
use serde::Serialize;
use serde_with::{hex::Hex, serde_as};
use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

#[derive(Subcommand, Clone, Debug)]
enum RequestType {
    Version,
    Ping { nonce: u64 },
    Addresses,
    UtxoSet,
}

#[derive(Parser)]
#[command(version = env!("VERGEN_GIT_DESCRIBE"))]
struct Cli {
    #[clap(short = 's', long, default_value = "localhost:16111", help = "The ip:port of a kaspad instance")]
    url: String,
    #[clap(short, long, default_value = "mainnet", help = "The network type and suffix, e.g. 'testnet-11'")]
    pub network: String,
    #[clap(long, help = "Pruning point hash, required for utxo-set")]
    pub pruning_point: Option<String>,
    #[clap(subcommand)]
    pub request: RequestType,
}

#[derive(Eq, PartialEq, Hash, Serialize)]
pub struct Version {
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
pub struct NetAddress {
    ip: IpAddr,
    port: u16,
}

#[serde_as]
#[derive(Eq, PartialEq, Hash, Serialize)]
pub struct Utxo {
    pub transaction_id: KaspaHash,
    pub index: u32,
    pub amount: u64,
    #[serde_as(as = "Hex")]
    pub script_public_key: Vec<u8>,
    pub script_public_key_version: u16,
    pub script_public_key_address: String,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

pub type UtxosetChunk = Vec<(TransactionOutpoint, UtxoEntry)>;

pub const IBD_BATCH_SIZE: usize = 99; // kaspa_p2p_flows::v5::ibd::IBD_BATCH_SIZE is private
pub const TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::main]
async fn main() {
    let cli_args = Arc::new(Cli::parse());

    let (sender, mut receiver) = mpsc::channel(10000);
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
        RequestType::UtxoSet => req_utxoset(&mut receiver, router, cli_args.network.clone(), cli_args.pruning_point.clone()).await,
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

async fn req_utxoset(receiver: &mut Receiver<KaspadMessage>, router: Arc<Router>, network: String, pruning_point: Option<String>) {
    let pruning_point = if let Some(pruning_point) = pruning_point {
        pruning_point
    } else {
        panic!("Pruning point is mandatory for utxo-set retrieval");
    };
    let _ = router
        .enqueue(make_message!(
            Payload::RequestPruningPointUtxoSet,
            RequestPruningPointUtxoSetMessage { pruning_point_hash: Some(Hash { bytes: hex::decode(pruning_point).unwrap() }) }
        ))
        .await;

    let prefix = Prefix::from(NetworkId::from_str(network.as_str()).unwrap());
    let mut i = 0;
    println!("[");
    loop {
        match timeout(TIMEOUT, receiver.recv()).await {
            Ok(op) => match op {
                Some(msg) => match msg.payload {
                    Some(Payload::PruningPointUtxoSetChunk(utxo_set_chunk_msg)) => {
                        let chunk: Vec<Utxo> = utxo_set_chunk_msg
                            .outpoint_and_utxo_entry_pairs
                            .into_iter()
                            .map(|u| {
                                let outpoint = u.outpoint.unwrap();
                                let utxo_entry = u.utxo_entry.unwrap();
                                let script_public_key: ScriptPublicKey = utxo_entry.script_public_key.unwrap().try_into().unwrap();
                                let address = extract_script_pub_key_address(&script_public_key, prefix).unwrap();
                                Utxo {
                                    transaction_id: KaspaHash::from_slice(outpoint.transaction_id.unwrap().bytes.as_slice()),
                                    index: outpoint.index,
                                    amount: utxo_entry.amount,
                                    script_public_key: script_public_key.script().to_vec(),
                                    script_public_key_version: script_public_key.version,
                                    script_public_key_address: address.address_to_string(),
                                    block_daa_score: utxo_entry.block_daa_score,
                                    is_coinbase: utxo_entry.is_coinbase,
                                }
                            })
                            .collect();
                        let json = serde_json::to_string(&chunk).unwrap();
                        println!("{},", json);
                        i += 1;
                        if i % IBD_BATCH_SIZE == 0 {
                            let _ = router
                                .enqueue(make_message!(
                                    Payload::RequestNextPruningPointUtxoSetChunk,
                                    RequestNextPruningPointUtxoSetChunkMessage {}
                                ))
                                .await;
                        }
                    }
                    Some(Payload::DonePruningPointUtxoSetChunks(_)) => {
                        println!("Finished receiving utxos");
                        break;
                    }
                    Some(Payload::RequestAddresses(_)) => {
                        let _ = router.enqueue(make_message!(Payload::Addresses, AddressesMessage { address_list: vec![] })).await;
                    }
                    Some(Payload::Ping(ping_msg)) => {
                        let _ = router.enqueue(make_message!(Payload::Pong, PongMessage { nonce: ping_msg.nonce })).await;
                    }
                    Some(_) => {}
                    None => panic!("Got message with empty payload"),
                },
                None => panic!("Channel unexpectedly closed"),
            },
            Err(_) => panic!("Peer timed out after {} seconds", TIMEOUT.as_secs()),
        }
    }
    println!("[]");
    println!("]");
}
