# unftp-site-hash

[![Crate Version](https://img.shields.io/crates/v/unftp-site-hash.svg)](https://crates.io/crates/unftp-site-hash)
[![API Docs](https://docs.rs/unftp-site-hash/badge.svg)](https://docs.rs/unftp-site-hash)
[![Crate License](https://img.shields.io/crates/l/unftp-site-hash.svg)](https://crates.io/crates/unftp-site-hash)

Adds a `SITE HASH` command to [libunftp](https://github.com/bolcom/libunftp), letting FTP
clients ask the server to compute a checksum of a file using SHA-256, SHA-1, MD5 or CRC32.

## Getting started

Add the crate to your project's dependencies in `Cargo.toml`:

```toml
[dependencies]
libunftp = "0.23"
unftp-sbe-fs = "0.4"
unftp-site-hash = "0.1"
tokio = { version = "1", features = ["full"] }
```

Register a [`HashCommandHandler`](https://docs.rs/unftp-site-hash/latest/unftp_site_hash/struct.HashCommandHandler.html)
with [`ServerBuilder::site_command`](https://docs.rs/libunftp/latest/libunftp/struct.ServerBuilder.html#method.site_command):

```rust
use libunftp::ServerBuilder;
use unftp_sbe_fs::Filesystem;
use unftp_site_hash::HashCommandHandler;

#[tokio::main]
pub async fn main() {
    let ftp_home = std::env::temp_dir();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(ftp_home.clone()).unwrap()))
        .site_command("HASH", HashCommandHandler::default())
        .build()
        .unwrap();

    server.listen("127.0.0.1:2121").await;
}
```

Clients can then issue:

```text
SITE HASH SHA-256 /path/to/file
SITE HASH /path/to/file          (uses the handler's default algorithm, SHA-256)
```

See the [examples](./examples) directory for a runnable server.

## Supported algorithms

- `SHA-256` (default)
- `SHA-1`
- `MD5`
- `CRC32`

## Minimum supported Rust version

Rust 1.89 or later.

## License

You're free to use, modify and distribute this software under the terms of
the [Apache License v2.0](./LICENSE).
