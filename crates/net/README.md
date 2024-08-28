## Consensus Network Library

Contains a gossipsub driver to run discv5 peer discovery and block gossip.

### Example

```rust,no_run
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use alloy::primitives::address;
use op_net::driver::NetworkDriver;

// Build the network driver.
let signer = address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9099);
let driver = NetworkDriver::builder()
    .with_chain_id(10) // op mainnet chain id
    .with_unsafe_block_signer(signer)
    .with_socket(socket)
    .build()
    .expect("Failed to builder network driver");

// Call `.start()` on the driver.
driver.start().expect("Failed to start network driver");

println!("NetworkDriver started.");
```

### Acknowledgements

Largely based off [magi](https://github.com/a16z/magi)'s [p2p module](https://github.com/a16z/magi/tree/master/src/network).
