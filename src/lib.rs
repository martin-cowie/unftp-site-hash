//! Adds a `SITE HASH` command to [libunftp], letting clients ask the server to compute a
//! checksum of a file using SHA-256, SHA-1, MD5 or CRC32.
//!
//! Register [`HashCommandHandler`] with [`ServerBuilder::site_command`]:
//!
//! ```no_run
//! use libunftp::ServerBuilder;
//! use unftp_sbe_fs::Filesystem;
//! use unftp_site_hash::HashCommandHandler;
//!
//! let server = ServerBuilder::new(Box::new(|| Filesystem::new("/srv/ftp").unwrap()))
//!     .site_command("HASH", HashCommandHandler::default())
//!     .build();
//! ```
//!
//! Clients then issue e.g. `SITE HASH SHA-256 /path/to/file`, or `SITE HASH /path/to/file` to
//! use the handler's configured default algorithm.
//!
//! [libunftp]: https://docs.rs/libunftp
//! [`ServerBuilder::site_command`]: libunftp::ServerBuilder::site_command

use async_trait::async_trait;
use libunftp::options::{Reply, ReplyCode, SiteCommandContext, SiteCommandHandler};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;
use tokio::io::AsyncReadExt;
use unftp_core::auth::UserDetail;
use unftp_core::storage::{Metadata, StorageBackend};

/// The hash algorithms that [`HashCommandHandler`] can compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgorithm {
    /// SHA-256, the default algorithm.
    #[default]
    Sha256,
    /// SHA-1.
    Sha1,
    /// MD5.
    Md5,
    /// CRC-32.
    Crc32,
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            HashAlgorithm::Sha256 => "SHA-256",
            HashAlgorithm::Sha1 => "SHA-1",
            HashAlgorithm::Md5 => "MD5",
            HashAlgorithm::Crc32 => "CRC32",
        };
        f.write_str(name)
    }
}

impl FromStr for HashAlgorithm {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().replace(['-', '_'], "").as_str() {
            "SHA256" => Ok(HashAlgorithm::Sha256),
            "SHA1" => Ok(HashAlgorithm::Sha1),
            "MD5" => Ok(HashAlgorithm::Md5),
            "CRC32" => Ok(HashAlgorithm::Crc32),
            _ => Err(()),
        }
    }
}

/// A [`SiteCommandHandler`] that implements `SITE HASH`.
///
/// Register an instance with [`ServerBuilder::site_command`](libunftp::ServerBuilder::site_command)
/// under the name `"HASH"`.
#[derive(Debug, Clone, Copy)]
pub struct HashCommandHandler {
    default_algorithm: HashAlgorithm,
}

impl HashCommandHandler {
    /// Creates a handler that falls back to `default_algorithm` when the client doesn't name one.
    pub fn new(default_algorithm: HashAlgorithm) -> Self {
        HashCommandHandler { default_algorithm }
    }

    fn parse_arguments<'a>(&self, arguments: &'a str) -> Option<(HashAlgorithm, &'a str)> {
        let arguments = arguments.trim();
        if arguments.is_empty() {
            return None;
        }
        match arguments.split_once(char::is_whitespace) {
            Some((first, rest)) => match HashAlgorithm::from_str(first) {
                Ok(algorithm) => {
                    let path = rest.trim();
                    if path.is_empty() { None } else { Some((algorithm, path)) }
                }
                Err(()) => Some((self.default_algorithm, arguments)),
            },
            None => Some((self.default_algorithm, arguments)),
        }
    }
}

impl Default for HashCommandHandler {
    fn default() -> Self {
        HashCommandHandler::new(HashAlgorithm::default())
    }
}

fn to_hex(bytes: &[u8]) -> String {
    use fmt::Write;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

async fn compute_hash(algorithm: HashAlgorithm, mut reader: Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>) -> std::io::Result<String> {
    let mut buffer = [0u8; 8192];
    let digest = match algorithm {
        HashAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
            loop {
                let n = reader.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            to_hex(&hasher.finalize())
        }
        HashAlgorithm::Sha1 => {
            let mut hasher = Sha1::new();
            loop {
                let n = reader.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            to_hex(&hasher.finalize())
        }
        HashAlgorithm::Md5 => {
            let mut hasher = Md5::new();
            loop {
                let n = reader.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            to_hex(&hasher.finalize())
        }
        HashAlgorithm::Crc32 => {
            let mut hasher = crc32fast::Hasher::new();
            loop {
                let n = reader.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            format!("{:08x}", hasher.finalize())
        }
    };
    Ok(digest)
}

#[async_trait]
impl<Storage, User> SiteCommandHandler<Storage, User> for HashCommandHandler
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    async fn handle(&self, context: &SiteCommandContext<Storage, User>) -> Reply {
        let Some((algorithm, path)) = self.parse_arguments(&context.arguments) else {
            return Reply::new(ReplyCode::ParameterSyntaxError, "Usage: SITE HASH [algorithm] <path>");
        };

        let Some(user) = context.user.as_ref() else {
            return Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate");
        };

        let reader = match context.storage.get(user, path, 0).await {
            Ok(reader) => reader,
            Err(err) => return Reply::new_with_string(ReplyCode::FileError, err.to_string()),
        };

        match compute_hash(algorithm, reader).await {
            Ok(digest) => Reply::new_with_string(ReplyCode::FileStatus, format!("{} {} {}", algorithm, digest, path)),
            Err(err) => Reply::new_with_string(ReplyCode::FileError, format!("Failed to read {}: {}", path, err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_algorithm_and_path() {
        let handler = HashCommandHandler::default();
        assert_eq!(handler.parse_arguments("SHA-1 /foo/bar.txt"), Some((HashAlgorithm::Sha1, "/foo/bar.txt")));
        assert_eq!(handler.parse_arguments("md5 /foo/bar.txt"), Some((HashAlgorithm::Md5, "/foo/bar.txt")));
    }

    #[test]
    fn falls_back_to_default_algorithm_when_only_a_path_is_given() {
        let handler = HashCommandHandler::default();
        assert_eq!(handler.parse_arguments("/foo/bar.txt"), Some((HashAlgorithm::Sha256, "/foo/bar.txt")));
    }

    #[test]
    fn rejects_empty_arguments() {
        let handler = HashCommandHandler::default();
        assert_eq!(handler.parse_arguments(""), None);
        assert_eq!(handler.parse_arguments("   "), None);
    }

    #[test]
    fn algorithm_display_matches_ftp_hash_draft_naming() {
        assert_eq!(HashAlgorithm::Sha256.to_string(), "SHA-256");
        assert_eq!(HashAlgorithm::Crc32.to_string(), "CRC32");
    }
}
