//! End-to-end checks of `SITE HASH` against a filesystem storage backend.

use libunftp::options::{Reply, ReplyCode, SiteCommandContext, SiteCommandHandler};
use std::path::PathBuf;
use std::sync::Arc;
use unftp_core::auth::DefaultUser;
use unftp_sbe_fs::Filesystem;
use unftp_site_hash::HashCommandHandler;

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const ABC_SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";
const ABC_MD5: &str = "900150983cd24fb0d6963f7d28e17f72";
const ABC_CRC32: &str = "352441c2";

fn fixture_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("unftp-site-hash-tests-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("abc.txt"), b"abc").unwrap();
    root
}

fn context(arguments: &str, user: Option<DefaultUser>) -> SiteCommandContext<Filesystem, DefaultUser> {
    SiteCommandContext {
        command: "HASH".to_string(),
        arguments: arguments.to_string(),
        username: user.as_ref().map(|_| "anonymous".to_string()),
        storage: Arc::new(Filesystem::new(fixture_root()).unwrap()),
        user: Arc::new(user),
        logger: slog::Logger::root(slog::Discard, slog::o!()),
    }
}

async fn run(arguments: &str) -> Reply {
    HashCommandHandler::default().handle(&context(arguments, Some(DefaultUser))).await
}

#[tokio::test]
async fn hashes_with_each_named_algorithm() {
    let cases = [("SHA-256", ABC_SHA256), ("SHA-1", ABC_SHA1), ("MD5", ABC_MD5), ("CRC32", ABC_CRC32)];
    for (name, digest) in cases {
        let reply = run(&format!("{name} /abc.txt")).await;
        let expected = Reply::new_with_string(ReplyCode::FileStatus, format!("{name} {digest} /abc.txt"));
        assert_eq!(reply, expected, "algorithm {name}");
    }
}

#[tokio::test]
async fn uses_default_algorithm_when_none_is_named() {
    let reply = run("/abc.txt").await;
    let expected = Reply::new_with_string(ReplyCode::FileStatus, format!("SHA-256 {ABC_SHA256} /abc.txt"));
    assert_eq!(reply, expected);
}

#[tokio::test]
async fn rejects_missing_arguments() {
    let reply = run("").await;
    assert!(matches!(
        reply,
        Reply::CodeAndMsg {
            code: ReplyCode::ParameterSyntaxError,
            ..
        }
    ));
}

#[tokio::test]
async fn reports_missing_file() {
    let reply = run("SHA-256 /does-not-exist.txt").await;
    assert!(matches!(
        reply,
        Reply::CodeAndMsg {
            code: ReplyCode::FileError,
            ..
        }
    ));
}

#[tokio::test]
async fn requires_authenticated_user() {
    let reply = HashCommandHandler::default().handle(&context("/abc.txt", None)).await;
    assert!(matches!(
        reply,
        Reply::CodeAndMsg {
            code: ReplyCode::NotLoggedIn,
            ..
        }
    ));
}
