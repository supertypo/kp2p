use kaspa_p2p_lib::{ConnectionInitializer, IncomingRoute, KaspadHandshake, KaspadMessagePayloadType, Router};
use std::sync::Arc;
use std::time::SystemTime;
use kaspa_p2p_lib::common::ProtocolError;
use kaspa_p2p_lib::pb::{KaspadMessage, VersionMessage};
use log::{debug, trace, warn};
use tonic::async_trait;
use uuid::Uuid;

pub struct AddressesFlow {
    receiver: IncomingRoute,
    router: Arc<Router>,
}

impl AddressesFlow {
    pub async fn register(router: Arc<Router>) {
        trace!("Subscribe to addresses");
        let receiver = router.subscribe(vec![
            KaspadMessagePayloadType::InvRelayBlock, // Mandatory for some reason
            KaspadMessagePayloadType::Addresses,
            KaspadMessagePayloadType::RequestAddresses,
        ]);
        let mut echo_flow = AddressesFlow { router, receiver };
        debug!("Start receiving loop");
        tokio::spawn(async move {
            debug!("EchoFlow, start message dispatching loop");
            while let Some(msg) = echo_flow.receiver.recv().await {
                if !echo_flow.call(msg).await {
                    warn!("EchoFlow, receive loop - call failed");
                    break;
                }
            }
            debug!("EchoFlow, exiting message dispatch loop");
        });
    }

    async fn call(&self, msg: KaspadMessage) -> bool {
        debug!("EchoFlow, got message:{:?}", msg);
        self.router.enqueue(msg).await.is_ok()
    }
}

#[derive(Default)]
pub struct AddressesFlowInitializer {}

fn build_dummy_version_message() -> VersionMessage {
    VersionMessage {
        protocol_version: 6,
        services: 0,
        timestamp: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64,
        address: None,
        id: Vec::from(Uuid::new_v4().as_bytes()),
        user_agent: String::new(),
        disable_relay_tx: true,
        subnetwork_id: None,
        network: "kaspa-mainnet".to_string(),
    }
}

impl AddressesFlowInitializer {
    pub fn new() -> Self {
        AddressesFlowInitializer {}
    }
}

#[async_trait]
impl ConnectionInitializer for AddressesFlowInitializer {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        // Build the handshake object and subscribe to handshake messages
        let mut handshake = KaspadHandshake::new(&router);
        // We start the router receive loop only after we registered to handshake routes
        router.start();
        // Build the local version message
        let self_version_message = build_dummy_version_message();
        // Perform the handshake
        let peer_version_message = handshake.handshake(self_version_message).await?;
        debug!("Handshake: {:?}", peer_version_message);
        // Subscribe to messages
        AddressesFlow::register(router.clone()).await;
        // Send a ready signal
        handshake.exchange_ready_messages().await?;
        // Note: at this point receivers for handshake subscriptions
        // are dropped, thus effectively unsubscribing
        Ok(())
    }
}
