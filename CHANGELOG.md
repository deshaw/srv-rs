# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.2.0 - 2020-12-18

### Changed

- Flattened `crate::client` and `crate::record` modules in public API
- Exported `Cache` from `crate::policy`
- Hid empty `crate::resolver::trust_dns` module
- Updated README contributing section and `crates.io` dependency

## 0.1.1 - 2020-12-18

### Changed

- Author email in Cargo.toml

### Fixed

- `docs.rs`: document with `--all-features`

## 0.1.0 - 2020-12-18

### Added

- Abstraction of SRV records
- Abstraction of SRV DNS resolvers
  - `libresolv` and `trust-dns`-based implementations
- Abstraction of SRV target selection policies
- Client for communicating with SRV-located services
