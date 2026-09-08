//! A filesystem-backed FTP server with `SITE HASH` enabled.
//!
//! Run with `cargo run --example basic`, then e.g.:
//! `curl -s ftp://127.0.0.1:2121/ -Q 'SITE HASH SHA-256 some-file.txt'`

use libunftp::ServerBuilder;
use unftp_sbe_fs::Filesystem;
use unftp_site_hash::HashCommandHandler;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let addr = "127.0.0.1:2121";
    let root = std::env::temp_dir();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(root.clone()).unwrap()))
        .site_command("HASH", HashCommandHandler::default())
        .build()
        .unwrap();

    println!("Starting ftp server on {addr}, serving {}", std::env::temp_dir().display());
    server.listen(addr).await.unwrap();
}
