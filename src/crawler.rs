use crate::initializer::{Initializer, ROUTER};
use crate::Cli;
use futures::future::join_all;
use kaspa_p2p_lib::pb::kaspad_message::Payload;
use kaspa_p2p_lib::pb::RequestAddressesMessage;
use kaspa_p2p_lib::{make_message, Hub};
use kaspa_utils::hex::ToHex;
use kaspa_utils::networking::IpAddress;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;

#[derive(Eq, PartialEq, Hash, Serialize, Clone)]
struct NetAddress {
    ip: IpAddr,
    port: u16,
}

#[derive(Serialize, Clone)]
struct NodeData {
    ip: String,
    metadata: NodeMetadata,
}

#[derive(Serialize, Clone)]
struct NodeMetadata {
    protocol_version: u32,
    network: String,
    services: u64,
    timestamp: String,
    id: String,
    user_agent: String,
    disable_relay_tx: bool,
}

/// crawl from the node provided in cli_args.url
pub async fn crawl_network(cli_args: Arc<Cli>) {
    let discovered_peers = Arc::new(Mutex::new(HashSet::new()));
    let results = Arc::new(Mutex::new(Vec::new()));
    let queue = Arc::new(Mutex::new(VecDeque::new()));

    println!("Starting crawl from: {}", cli_args.url);
    queue.lock().await.push_back(cli_args.url.clone());

    // continue until there are no more nodes in the queue
    while !queue.lock().await.is_empty() {
        // drain current queue into a batch for concurrent processing
        let batch = {
            let mut queue_lock = queue.lock().await;
            // drain all nodes currently in queue
            queue_lock.drain(..).collect::<Vec<String>>()
        };

        println!("Batch size: {}", batch.len());
        // async task for each node in the batch
        let tasks = batch
            .into_iter()
            .map(|url| {
                let cli_args = cli_args.clone();
                let discovered_peers = Arc::clone(&discovered_peers);
                let results = Arc::clone(&results);
                let queue = Arc::clone(&queue);

                tokio::spawn(async move {
                    println!("Querying: {}", url);
                    match connect_and_query_peer(cli_args, &url).await {
                        Ok((url, peers, Some(metadata))) => {
                            println!("{}: Handshake OK, {} peers", url, peers.len());
                            results.lock().await.push(NodeData { ip: url.clone(), metadata });

                            let mut discovered_lock = discovered_peers.lock().await;
                            let mut queue_lock = queue.lock().await;
                            for peer in peers {
                                // only add new peers to avoid duplicate processing
                                if !discovered_lock.contains(&peer) {
                                    discovered_lock.insert(peer.clone());
                                    let peer_addr = format!("{}:{}", peer.ip, peer.port);
                                    queue_lock.push_back(peer_addr.clone());
                                    println!("New peer: {}", peer_addr);
                                }
                            }
                        }
                        Ok((url, _, None)) => println!("{}: Handshake failed", url),
                        Err(_) => println!("{}: Query failed", url),
                    }
                })
            })
            .collect::<Vec<_>>();

        // If no tasks were spawned, there are no new nodes to query
        if tasks.is_empty() {
            println!("No new peers to query, exiting...");
            break;
        } else {
            println!("Awaiting {} queries", tasks.len());
        }

        // Await all tasks concurrently using join_all
        // This is the batch processing step: all node queries in the current batch run in parallel
        join_all(tasks).await;
        println!("Batch complete, checking queue");
    }

    println!("Finalizing results...");
    // serialize collected results to JSON
    let results_guard = results.lock().await;
    let nodes = (*results_guard).clone();
    drop(results_guard);

    let json_data = serde_json::to_string_pretty(&nodes).expect("Failed to serialize results to JSON");

    if let Err(e) = fs::write(Path::new(&cli_args.output), json_data).await {
        eprintln!("Error writing JSON file: {}", e);
    }
}

/// Query a single node at the given URL and return its discovered addresses and handshake metadata
/// returns error if handshake fails
async fn connect_and_query_peer(cli_args: Arc<Cli>, url: &str) -> Result<(String, Vec<NetAddress>, Option<NodeMetadata>), ()> {
    println!("Connecting to node: {}", url);

    // channel to receive messages from peer
    let (sender, mut receiver) = mpsc::channel(10);
    let initializer = Arc::new(Initializer::new(cli_args.clone(), sender));
    let adaptor = kaspa_p2p_lib::Adaptor::client_only(Hub::new(), initializer, Default::default());

    if adaptor.connect_peer(url.to_string()).await.is_err() {
        println!("Peer {}: connection failed", url);
        adaptor.terminate_all_peers().await;
        return Err(());
    }

    // Retrieve shared router from global state
    let router = {
        let router_guard = ROUTER.read().unwrap();
        router_guard.clone()
    };
    let router = if let Some(router) = router {
        router
    } else {
        println!("Peer {}: router not initialized, skipping", url);
        adaptor.terminate_all_peers().await;
        return Err(());
    };

    // Send a request to get peers list
    let _ = router
        .enqueue(make_message!(
            Payload::RequestAddresses,
            RequestAddressesMessage { include_all_subnetworks: false, subnetwork_id: None }
        ))
        .await;

    let mut addresses = Vec::new();
    let mut metadata: Option<NodeMetadata> = None;

    // Listen for responses with a timeout to avoid hanging indefinitely
    loop {
        match timeout(Duration::from_secs(3), receiver.recv()).await {
            Ok(Some(msg)) => match msg.payload {
                Some(Payload::Addresses(addresses_msg)) => {
                    for address in addresses_msg.address_list {
                        if let Ok(result) = address.try_into() {
                            let (ip, port): (IpAddress, u16) = result;
                            addresses.push(NetAddress { ip: ip.to_canonical(), port });
                        }
                    }
                    println!("Peer {}: received {} addresses", url, addresses.len());
                }
                Some(Payload::Version(version_msg)) => {
                    metadata = Some(NodeMetadata {
                        protocol_version: version_msg.protocol_version,
                        network: version_msg.network,
                        services: version_msg.services,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        id: version_msg.id.to_hex(),
                        user_agent: version_msg.user_agent,
                        disable_relay_tx: version_msg.disable_relay_tx,
                    });
                    println!("Peer {}: received metadata", url);
                }
                _ => {}
            },
            Ok(None) => break,
            Err(_) => {
                println!("Peer {}: timeout waiting for messages", url);
                break;
            }
        }
    }

    println!("Peer {}: disconnected", url);
    adaptor.terminate_all_peers().await;

    // only return nodes that completed handshake successfully
    if metadata.is_some() {
        Ok((url.to_string(), addresses, metadata))
    } else {
        Err(())
    }
}
