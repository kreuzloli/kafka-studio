# Kafka Studio

A native, read-only Apache Kafka desktop client being built with **Rust and GPUI**, with macOS as the initial target.

This is an incremental learning project. The goal is to import Offset Explorer connection profiles, connect to existing Kafka clusters, and browse topics, messages, and consumer groups.

> **Work in progress:** currently on Lesson 7 — JAAS parsing and credential lifecycle. Real Kafka connectivity is not implemented yet.

## Current Progress

As of **2026-10-28**:

- **Desktop foundation:** connection selection, folder navigation, and in-memory connection editing with basic validation.
- **Offline XML import:** reads connection metadata and nested folders, validates known structure and duplicate folder IDs, and maps external folder IDs to fresh UUIDs. Empty folders are preserved; connections with missing folder references remain ungrouped.
- **JAAS parsing:** supports a single `ScramLoginModule` with `required`, quoted values, and controlled escapes. Credential field names are case-insensitive; values are preserved. Unknown options are parsed and ignored, while malformed values still fail.
- **Configuration validation:** rejects duplicate or missing credential fields, missing separators or semicolons, and non-whitespace content after the final semicolon.
- **XML/JAAS boundary test:** verifies XML entities are decoded once before JAAS parsing, without changing literal entity text inside credentials.

The XML importer is not yet connected to the UI through a unified import entry point. The XML-to-JAAS flow is currently verified in a test; authentication fields are not yet integrated into the production import flow. Parsing a profile does not establish a Kafka connection.

## Next Steps

- Match security protocols and SASL mechanisms to supported authentication configurations.
- Introduce secret handling, redacted diagnostics, session credentials, and a macOS Keychain adapter.
- Add certificate path mapping, JKS migration guidance, and an import preview.
- Introduce background Kafka operations and verify an explicitly authorized development cluster.

Keychain support and credential lifecycle handling are not complete. Credentials must not be persisted as plaintext or included in logs.

## Planned Read-Only Features

- Multiple local connection profiles and folders.
- Broker, topic, partition, and configuration browsing.
- Bounded message inspection with text, JSON, and hex views.
- Message filtering by partition, offset, time, and content.
- Consumer group, committed offset, and lag observation.
- Local persistence of non-sensitive configuration, with separate credential storage.

The v1 scope excludes message production, topic or ACL changes, and consumer offset commits or resets.

## Tech Stack

Current direct dependencies:

| Technology | Purpose |
| --- | --- |
| Rust, edition 2024 | Application language |
| GPUI and gpui-component | Native desktop UI |
| quick-xml 0.42 | XML attribute and event parsing |
| uuid with v4 support | Internal folder identifiers |

`rdkafka`, an application-managed Tokio runtime, SQLite/SQLx, and a Keychain adapter are planned for later lessons. See `Cargo.toml` and `Cargo.lock` for the current dependency configuration.

## Project Structure

The project currently uses a single crate, organized by responsibility:

```text
src/
├── common/
├── components/
├── enums/
├── imports/       # Offset Explorer XML and JAAS parsing
├── models/        # Connections, folders, import records, credentials
├── utils/
├── views/         # GPUI views and connection tree
├── lib.rs
└── main.rs
tests/fixtures/    # Fictional XML examples
docs/PROJECT_GUIDE.md
```

## Getting Started

Prerequisites: macOS, a Rust stable toolchain supporting edition 2024, and Xcode Command Line Tools. A Kafka cluster is not needed for the current UI and offline parsing tests.

```bash
git clone https://github.com/kreuzloli/kafka-studio.git
cd kafka-studio
cargo build --locked
cargo run --locked
```

## Verification

```bash
cargo fmt --check
cargo test --locked imports::jaas::tests
cargo test --locked imports::offset_explorer::tests
```

Latest JAAS verification on **2026-10-28**: **10 tests passed**, including the XML entity decoding test. These are offline tests, not Kafka authentication or connectivity checks. Unused-code warnings remain while the parsing modules are being integrated.

## Learning Guide

The [project guide](docs/PROJECT_GUIDE.md) describes the 24-lesson course, compatibility rules, architecture goals, and acceptance criteria. Planned capabilities in that document should not be read as already implemented.

Use fictional credentials and addresses in fixtures. Do not commit real connection XML, passwords, or private keys.
