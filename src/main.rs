mod address_flow;

use crate::address_flow::AddressesFlowInitializer;
use clap::Parser;
use kaspa_p2p_lib::Hub;
use std::env;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
struct Cli {
    #[clap(short = 's', long, help = "The url to a kaspad instance, e.g 'localhost:16111'")]
    url: String,
    #[clap(short, long)]
    verbose: Option<bool>,
    #[clap(short, long)]
    timeout: Option<u64>,
    #[clap(short, long, default_value = "info", help = "error, warn, info, debug, trace, off")]
    pub log_level: String,
    #[clap(long, help = "Disable colored output")]
    pub log_no_color: bool,
}

#[tokio::main]
async fn main() {
    let cli_args = Cli::parse();

    env::set_var("RUST_LOG", &cli_args.log_level);
    env::set_var("RUST_LOG_STYLE", if cli_args.log_no_color { "never" } else { "always" });
    env_logger::builder().target(env_logger::Target::Stdout).format_target(false).format_timestamp_millis().init();

    let initializer = Arc::new(AddressesFlowInitializer::new());
    let adaptor = kaspa_p2p_lib::Adaptor::client_only(Hub::new(), initializer, Default::default());
    let _peer_key = adaptor.connect_peer_with_retries(cli_args.url, 16, Duration::from_secs(1)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    adaptor.terminate_all_peers().await;
}
