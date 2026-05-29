use crate::Cli;
use kaspa_p2p_lib::common::ProtocolError;
use kaspa_p2p_lib::pb::kaspad_message::Payload;
use kaspa_p2p_lib::pb::{KaspadMessage, ReadyMessage, VerackMessage, VersionMessage};
use kaspa_p2p_lib::{ConnectionInitializer, IncomingRoute, KaspadMessagePayloadType, Router, dequeue_with_timeout, make_message};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Sender;
use tonic::async_trait;
use uuid::Uuid;

pub static ROUTER: RwLock<Option<Arc<Router>>> = RwLock::new(None);

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);

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

        let mut version_route = router.subscribe(vec![KaspadMessagePayloadType::Version]);
        let mut verack_route = router.subscribe(vec![KaspadMessagePayloadType::Verack]);
        let mut ready_route = router.subscribe(vec![KaspadMessagePayloadType::Ready]);

        router.start();

        let peer_version: VersionMessage = dequeue_with_timeout!(version_route, Payload::Version, HANDSHAKE_TIMEOUT)?;
        let expected_network = format!("kaspa-{}", self.cli_args.network.to_lowercase());
        if peer_version.network != expected_network {
            return Err(ProtocolError::WrongNetwork(expected_network, peer_version.network));
        }
        router.enqueue(make_message!(Payload::Verack, VerackMessage {})).await?;

        let our_version = build_version_message(self.cli_args.clone(), &peer_version);
        router.enqueue(make_message!(Payload::Version, our_version)).await?;
        let _verack: VerackMessage = dequeue_with_timeout!(verack_route, Payload::Verack, HANDSHAKE_TIMEOUT)?;

        self.sender
            .send(KaspadMessage { request_id: 0, response_id: 0, payload: Some(Payload::Version(peer_version)) })
            .await
            .unwrap();

        let mut incoming_route = subscribe_all(&router);
        let sender = self.sender.clone();
        tokio::spawn(async move {
            while let Some(msg) = incoming_route.recv().await {
                let _ = sender.send(msg).await;
            }
        });

        router.enqueue(make_message!(Payload::Ready, ReadyMessage {})).await?;
        let _ready: ReadyMessage = dequeue_with_timeout!(ready_route, Payload::Ready, HANDSHAKE_TIMEOUT)?;
        Ok(())
    }
}

fn build_version_message(cli_args: Arc<Cli>, peer: &VersionMessage) -> VersionMessage {
    VersionMessage {
        protocol_version: peer.protocol_version,
        services: peer.services,
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64,
        address: None,
        id: Vec::from(Uuid::new_v4().as_bytes()),
        user_agent: format!("/kp2p:{}/", env!("VERGEN_GIT_DESCRIBE")),
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
