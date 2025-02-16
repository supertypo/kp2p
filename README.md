# kp2p

Simple tool for querying Kaspa nodes on the p2p port.

## Docker images
https://hub.docker.com/r/supertypo/kp2p

## Build and run
Protobuf is required, see 'Installation' here: https://github.com/kaspanet/rusty-kaspa/
```bash
cargo run -- -s 192.168.1.113:16111 version
```

Help
```text
Usage: kp2p [OPTIONS] <REQUEST>

Arguments:
  <REQUEST>  Request type [possible values:
    * version: Retrieves the version of the Kaspa node
    * addresses: Retrieves a list of addresses from the Kaspa node
    * crawl: Initiates a network crawl starting from the specified node]

Options:
  -s, --url <URL>          The ip:port of a kaspad instance [default: localhost:16111]
  -n, --network <NETWORK>  The network type and suffix, e.g. 'testnet-11' [default: mainnet]
  -o, --output <OUTPUT>    Output JSON file for crawl mode [default: nodes.json]
```
