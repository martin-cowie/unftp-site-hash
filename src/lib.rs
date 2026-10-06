//! Adds a `SITE HASH` command to [libunftp], letting clients ask the server to compute checksums
//! of one or more files using SHA-256, SHA-1, MD5 or CRC32.
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
//! Clients then issue, for example:
//!
//! ```text
//! SITE HASH file.txt
//! SITE HASH -a MD5 file.txt "file with spaces.txt"
//! SITE HASH -a sha-1 -- -leading-dash.txt
//! ```
//!
//! Without `-a`, the handler's configured default algorithm is used. Algorithm names are
//! case-insensitive, and lower-case names are as normal as upper-case ones: `-a md5` and
//! `-a MD5` are equivalent. Arguments are split the way a POSIX shell splits them, so quotes keep
//! names containing spaces together, and `--` ends the options so that a name starting with `-`
//! can be given as a path.
//!
//! The reply has one line per path: `<algorithm> <digest> <path>`. A path that cannot be read
//! gets `<path>: <reason>` instead, and the reply code is 550 if any path failed.
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

const MAX_PATHS: usize = 100;
const USAGE: &str = "Usage: SITE HASH [-a SHA-256|SHA-1|MD5|CRC32] [--] <path>...";

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
    /// Writes the algorithm's name as used in `SITE HASH` replies, such as `SHA-256`.
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

    /// Parses an algorithm name, ignoring case and any `-` or `_` separators, so `sha-256`
    /// and `SHA256` both parse.
    ///
    /// # Errors
    ///
    /// Returns `Err(())` if `s` does not name a supported algorithm.
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
/// under the name `"HASH"`. Algorithm names given by clients are case-insensitive.
#[derive(Debug, Clone, Copy)]
pub struct HashCommandHandler {
    default_algorithm: HashAlgorithm,
}

impl HashCommandHandler {
    /// Creates a handler.
    ///
    /// `default_algorithm` is used when the client names no algorithm. Returns the new handler.
    pub fn new(default_algorithm: HashAlgorithm) -> Self {
        HashCommandHandler { default_algorithm }
    }

    fn parse_arguments(&self, arguments: &str) -> Result<Request, String> {
        let words = shell_words::split(arguments).map_err(|err| format!("Could not parse arguments: {err}"))?;
        let mut words = words.into_iter();
        let mut algorithm = self.default_algorithm;
        let mut paths = Vec::new();
        while let Some(word) = words.next() {
            match word.as_str() {
                "--" => {
                    paths.extend(words.by_ref());
                    break;
                }
                "-a" => {
                    let name = words.next().ok_or("-a needs an algorithm name")?;
                    algorithm = name.parse().map_err(|()| format!("Unknown algorithm {name}"))?;
                }
                option if option.starts_with('-') => return Err(format!("Unknown option {option}")),
                _ => paths.push(word),
            }
        }
        if paths.is_empty() {
            return Err("No paths given".to_string());
        }
        if paths.len() > MAX_PATHS {
            return Err(format!("At most {MAX_PATHS} paths may be given"));
        }
        Ok(Request { algorithm, paths })
    }
}

impl Default for HashCommandHandler {
    /// Returns a handler that uses SHA-256 when the client names no algorithm.
    fn default() -> Self {
        HashCommandHandler::new(HashAlgorithm::default())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Request {
    algorithm: HashAlgorithm,
    paths: Vec<String>,
}

fn to_hex(bytes: &[u8]) -> String {
    use fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(result, "{byte:02x}").expect("writing to a String cannot fail");
    }
    result
}

async fn read_chunks(mut reader: Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>, mut consume: impl FnMut(&[u8]) + Send) -> std::io::Result<()> {
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }
        consume(&buffer[..n]);
    }
}

async fn digest<D: Digest + Send>(reader: Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>) -> std::io::Result<String> {
    let mut hasher = D::new();
    read_chunks(reader, |chunk| hasher.update(chunk)).await?;
    Ok(to_hex(&hasher.finalize()))
}

async fn compute_hash(algorithm: HashAlgorithm, reader: Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>) -> std::io::Result<String> {
    let result = match algorithm {
        HashAlgorithm::Sha256 => digest::<Sha256>(reader).await?,
        HashAlgorithm::Sha1 => digest::<Sha1>(reader).await?,
        HashAlgorithm::Md5 => digest::<Md5>(reader).await?,
        HashAlgorithm::Crc32 => {
            let mut hasher = crc32fast::Hasher::new();
            read_chunks(reader, |chunk| hasher.update(chunk)).await?;
            format!("{:08x}", hasher.finalize())
        }
    };
    Ok(result)
}

async fn hash_file<Storage, User>(context: &SiteCommandContext<Storage, User>, user: &User, algorithm: HashAlgorithm, path: &str) -> Result<String, String>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    let reader = context.storage.get(user, path, 0).await.map_err(|err| format!("{path}: {err}"))?;
    let digest = compute_hash(algorithm, reader).await.map_err(|err| format!("{path}: Failed to read: {err}"))?;
    Ok(format!("{algorithm} {digest} {path}"))
}

#[async_trait]
impl<Storage, User> SiteCommandHandler<Storage, User> for HashCommandHandler
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    /// Replies to `SITE HASH` with one line per path named in the arguments.
    ///
    /// The reply code is 213 when every path was hashed, and 550 when any path failed; each
    /// failed path still gets its own line. Malformed arguments give a syntax error, and an
    /// unauthenticated session gives `NotLoggedIn`.
    async fn handle(&self, context: &SiteCommandContext<Storage, User>) -> Reply {
        let request = match self.parse_arguments(&context.arguments) {
            Ok(request) => request,
            Err(problem) => return Reply::new_with_string(ReplyCode::ParameterSyntaxError, format!("{problem}. {USAGE}")),
        };

        let Some(user) = context.user.as_ref() else {
            return Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate");
        };

        let mut results = Vec::with_capacity(request.paths.len());
        for path in &request.paths {
            results.push(hash_file(context, user, request.algorithm, path).await);
        }

        let code = if results.iter().all(Result::is_ok) {
            ReplyCode::FileStatus
        } else {
            ReplyCode::FileError
        };
        let lines = results.into_iter().map(|result| result.unwrap_or_else(|line| line));
        Reply::new_multiline(code, lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &str) -> Result<Request, String> {
        HashCommandHandler::default().parse_arguments(arguments)
    }

    fn request(algorithm: HashAlgorithm, paths: &[&str]) -> Request {
        Request {
            algorithm,
            paths: paths.iter().map(|path| path.to_string()).collect(),
        }
    }

    #[test]
    fn uses_default_algorithm_without_option() {
        assert_eq!(parse("/foo/bar.txt"), Ok(request(HashAlgorithm::Sha256, &["/foo/bar.txt"])));
    }

    #[test]
    fn accepts_algorithm_option_in_any_case() {
        assert_eq!(parse("-a SHA-1 /foo"), Ok(request(HashAlgorithm::Sha1, &["/foo"])));
        assert_eq!(parse("-a md5 /foo"), Ok(request(HashAlgorithm::Md5, &["/foo"])));
    }

    #[test]
    fn accepts_many_paths_including_quoted_names() {
        assert_eq!(
            parse(r#"-a crc32 a.txt "b c.txt" 'd e.txt'"#),
            Ok(request(HashAlgorithm::Crc32, &["a.txt", "b c.txt", "d e.txt"]))
        );
    }

    #[test]
    fn treats_everything_after_double_dash_as_paths() {
        assert_eq!(parse("-- -a"), Ok(request(HashAlgorithm::Sha256, &["-a"])));
        assert_eq!(parse("-a md5 -- MD5"), Ok(request(HashAlgorithm::Md5, &["MD5"])));
    }

    #[test]
    fn rejects_missing_paths() {
        assert!(parse("").is_err());
        assert!(parse("-a md5").is_err());
    }

    #[test]
    fn rejects_unknown_algorithm_unknown_option_and_unbalanced_quotes() {
        assert!(parse("-a sha999 /foo").is_err());
        assert!(parse("-a").is_err());
        assert!(parse("-x /foo").is_err());
        assert!(parse("\"unterminated").is_err());
    }

    #[test]
    fn rejects_more_than_the_maximum_number_of_paths() {
        let at_limit = vec!["/f"; MAX_PATHS].join(" ");
        let over_limit = vec!["/f"; MAX_PATHS + 1].join(" ");
        assert!(parse(&at_limit).is_ok());
        assert!(parse(&over_limit).is_err());
    }

    #[test]
    fn algorithm_display_matches_ftp_hash_draft_naming() {
        assert_eq!(HashAlgorithm::Sha256.to_string(), "SHA-256");
        assert_eq!(HashAlgorithm::Crc32.to_string(), "CRC32");
    }
}
