//! End-to-end checks of `SITE HASH` against a filesystem storage backend.

use libunftp::options::{Reply, ReplyCode, SiteCommandContext, SiteCommandHandler};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use unftp_core::auth::DefaultUser;
use unftp_sbe_fs::Filesystem;
use unftp_site_hash::HashCommandHandler;

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const ABC_SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";
const ABC_MD5: &str = "900150983cd24fb0d6963f7d28e17f72";
const ABC_CRC32: &str = "352441c2";

fn fixture_root() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let result = std::env::temp_dir().join(format!("unftp-site-hash-tests-{}", std::process::id()));
        std::fs::create_dir_all(&result).unwrap();
        std::fs::write(result.join("abc.txt"), b"abc").unwrap();
        std::fs::write(result.join("with space.txt"), b"abc").unwrap();
        result
    })
    .clone()
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

fn lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| line.to_string()).collect()
}

#[tokio::test]
async fn hashes_with_each_named_algorithm_in_any_case() {
    let cases = [
        ("SHA-256", "SHA-256", ABC_SHA256),
        ("sha-256", "SHA-256", ABC_SHA256),
        ("SHA-1", "SHA-1", ABC_SHA1),
        ("sha1", "SHA-1", ABC_SHA1),
        ("MD5", "MD5", ABC_MD5),
        ("md5", "MD5", ABC_MD5),
        ("CRC32", "CRC32", ABC_CRC32),
        ("crc32", "CRC32", ABC_CRC32),
    ];
    for (name, canonical, digest) in cases {
        let reply = run(&format!("-a {name} /abc.txt")).await;
        let expected = Reply::new_multiline(ReplyCode::FileStatus, lines(&[&format!("{canonical} {digest} /abc.txt")]));
        assert_eq!(reply, expected, "algorithm {name}");
    }
}

#[tokio::test]
async fn uses_default_algorithm_when_none_is_named() {
    let reply = run("/abc.txt").await;
    let expected = Reply::new_multiline(ReplyCode::FileStatus, lines(&[&format!("SHA-256 {ABC_SHA256} /abc.txt")]));
    assert_eq!(reply, expected);
}

#[tokio::test]
async fn hashes_each_path_in_order_including_quoted_names() {
    let reply = run(r#"-a md5 /abc.txt "/with space.txt""#).await;
    let expected = Reply::new_multiline(
        ReplyCode::FileStatus,
        lines(&[&format!("MD5 {ABC_MD5} /abc.txt"), &format!("MD5 {ABC_MD5} /with space.txt")]),
    );
    assert_eq!(reply, expected);
}

#[tokio::test]
async fn reports_each_failed_path_on_its_own_line() {
    let reply = run("/abc.txt /does-not-exist.txt").await;
    match reply {
        Reply::MultiLine { code, lines } => {
            assert_eq!(code, ReplyCode::FileError);
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0], format!("SHA-256 {ABC_SHA256} /abc.txt"));
            assert!(lines[1].starts_with("/does-not-exist.txt: "), "unexpected line: {}", lines[1]);
        }
        other => panic!("expected a multi-line reply, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_missing_paths() {
    let reply = run("-a md5").await;
    assert!(matches!(
        reply,
        Reply::CodeAndMsg {
            code: ReplyCode::ParameterSyntaxError,
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
