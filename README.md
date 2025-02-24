# kp2p

Simple tool for querying Kaspa nodes on the p2p port.

## Docker images
https://hub.docker.com/r/supertypo/kp2p

## Build and run
Protobuf is required, see 'Installation' here: https://github.com/kaspanet/rusty-kaspa/

```bash
cargo run -- -s 192.168.1.113:16111 version
```
```text
{
  "protocol_version": 6,
  "network": "kaspa-mainnet",
  "services": 0,
  "timestamp": "2025-02-24T23:33:26.460Z",
  "address": {
    "ip": "192.168.1.113",
    "port": 16111
  },
  "id": "xxx",
  "user_agent": "/kaspad:0.16.1/kaspad:0.16.1/",
  "disable_relay_tx": false,
  "subnetwork_id": null
}
```

Help
```text
Usage: kp2p [OPTIONS] <COMMAND>

Commands:
  version    
  ping       
  addresses  
  help       Print this message or the help of the given subcommand(s)

Options:
  -s, --url <URL>          The ip:port of a kaspad instance [default: localhost:16111]
  -n, --network <NETWORK>  The network type and suffix, e.g. 'testnet-11' [default: mainnet]
  -h, --help               Print help
  -V, --version            Print version
```
