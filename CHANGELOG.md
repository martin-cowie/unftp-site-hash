# Changelog

## Unreleased

- Initial release: `HashCommandHandler` implementing `SITE HASH` for libunftp, supporting
  SHA-256, SHA-1, MD5 and CRC32.
- Depends on the crates.io releases of libunftp 0.23 and unftp-core 0.1 rather than git `master`.
- Minimum supported Rust version is 1.89.
- Integration tests exercising the handler against a filesystem storage backend.
