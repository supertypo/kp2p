use crate::Cli;
use kaspa_p2p_lib::common::ProtocolError;
use kaspa_p2p_lib::pb::kaspad_message::Payload;
use kaspa_p2p_lib::pb::{KaspadMessage, VersionMessage};
use kaspa_p2p_lib::{ConnectionInitializer, IncomingRoute, KaspadHandshake, KaspadMessagePayloadType, Router};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Sender;
use tonic::async_trait;
use uuid::Uuid;

pub static ROUTER: RwLock<Option<Arc<Router>>> = RwLock::new(None);

pub struct Initializer {
    cli_args: Arc<Cli>,
    sender: Sender<KaspadMessage>,
}

impl Initializer {
    pub fn new(cli_args: Arc<Cli>, sender: Sender<KaspadMessage>) -> Self {
        Initializer { cli_args, sender }
    }
}

#[async_trait]
impl ConnectionInitializer for Initializer {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        ROUTER.write().unwrap().replace(router.clone());
        let mut handshake = KaspadHandshake::new(&router);
        router.start();
        let version_msg = handshake.handshake(build_dummy_version_message(self.cli_args.clone())).await?;
        self.sender.send(KaspadMessage { request_id: 0, response_id: 0, payload: Some(Payload::Version(version_msg)) }).await.unwrap();

        let mut incoming_route = subscribe_all(&router);
        let sender = self.sender.clone();
        tokio::spawn(async move {
            while let Some(msg) = incoming_route.recv().await {
                if sender.send(msg).await.is_err() {
                    break;
                }
            }
        });

        handshake.exchange_ready_messages().await?;
        Ok(())
    }
}

fn build_dummy_version_message(cli_args: Arc<Cli>) -> VersionMessage {
    VersionMessage {
        protocol_version: 6,
        services: 0,
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64,
        address: None,
        id: Vec::from(Uuid::new_v4().as_bytes()),
        user_agent: "kp2p".to_string(),
        disable_relay_tx: true,
        subnetwork_id: None,
        network: format!("kaspa-{}", cli_args.network.to_lowercase()),
    }
}

fn subscribe_all(router: &Arc<Router>) -> IncomingRoute {
    router.subscribe(vec![
        KaspadMessagePayloadType::Addresses,
        KaspadMessagePayloadType::Block,
        KaspadMessagePayloadType::Transaction,
        KaspadMessagePayloadType::BlockLocator,
        KaspadMessagePayloadType::RequestAddresses,
        KaspadMessagePayloadType::RequestRelayBlocks,
        KaspadMessagePayloadType::RequestTransactions,
        KaspadMessagePayloadType::IbdBlock,
        KaspadMessagePayloadType::InvRelayBlock,
        KaspadMessagePayloadType::InvTransactions,
        KaspadMessagePayloadType::Ping,
        KaspadMessagePayloadType::Pong,
        // KaspadMessagePayloadType::Verack,
        // KaspadMessagePayloadType::Version,
        KaspadMessagePayloadType::TransactionNotFound,
        KaspadMessagePayloadType::Reject,
        KaspadMessagePayloadType::PruningPointUtxoSetChunk,
        KaspadMessagePayloadType::RequestIbdBlocks,
        KaspadMessagePayloadType::UnexpectedPruningPoint,
        KaspadMessagePayloadType::IbdBlockLocator,
        KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
        KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk,
        KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
        KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound,
        KaspadMessagePayloadType::BlockWithTrustedData,
        KaspadMessagePayloadType::DoneBlocksWithTrustedData,
        KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
        KaspadMessagePayloadType::BlockHeaders,
        KaspadMessagePayloadType::RequestNextHeaders,
        KaspadMessagePayloadType::DoneHeaders,
        KaspadMessagePayloadType::RequestPruningPointUtxoSet,
        KaspadMessagePayloadType::RequestHeaders,
        KaspadMessagePayloadType::RequestBlockLocator,
        KaspadMessagePayloadType::PruningPoints,
        KaspadMessagePayloadType::RequestPruningPointProof,
        KaspadMessagePayloadType::PruningPointProof,
        // KaspadMessagePayloadType::Ready,
        KaspadMessagePayloadType::BlockWithTrustedDataV4,
        KaspadMessagePayloadType::TrustedData,
        KaspadMessagePayloadType::RequestIbdChainBlockLocator,
        KaspadMessagePayloadType::IbdChainBlockLocator,
        KaspadMessagePayloadType::RequestAntipast,
        KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks,
    ])
}
