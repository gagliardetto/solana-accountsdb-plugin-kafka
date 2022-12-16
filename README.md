# Solana AccountsDB Plugin for Kafka

Kafka publisher for use with Solana's [plugin framework](https://docs.solana.com/developing/plugins/geyser-plugins).

## Installation

### Binary releases

Find binary releases at: https://github.com/Blockdaemon/solana-accountsdb-plugin-kafka/releases

### Building from source

```shell
cargo build --release
```

- Linux: `./target/release/libsolana_accountsdb_plugin_kafka.so`
- macOS: `./target/release/libsolana_accountsdb_plugin_kafka.dylib`

**Important:** Solana's plugin interface requires the build environment of the Solana validator and this plugin to be **identical**.

This includes the Solana version and Rust compiler version.
Loading a plugin targeting wrong versions will result in memory corruption and crashes.

## Config

Config is specified via the plugin's JSON config file.

### Example Config

```json
{
  "libpath": "/solana/target/release/libsolana_accountsdb_plugin_kafka.so",
  "kafka": {
    "bootstrap.servers": "localhost:9092",
    "request.required.acks": "1",
    "message.timeout.ms": "30000",
    "compression.type": "lz4",
    "partitioner": "murmur2_random"
  },
  "shutdown_timeout_ms": 30000,
  "update_account_topic": "solana.testnet.account_updates",
  "slot_status_topic": "solana.testnet.slot_status",
  "publish_all_accounts": false,
  "program_ignores": [
    "Sysvar1111111111111111111111111111111111111",
    "Vote111111111111111111111111111111111111111"
  ],
  "program_allowlist_url": "https://example.com/program_allowlist.txt",
  "program_allowlist_expiry_sec": 5,
  "program_allowlist": [
    "11111111111111111111111111111111"
  ]
}
```

### Reference

- `libpath`: Path to Kafka plugin
- `kafka`: [`librdkafka` config options](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md).
  This plugin overrides the defaults as seen in the example config.
- `shutdown_timeout_ms`: Time the plugin is given to flush out all messages to Kafka upon exit request.
- `update_account_topic`: Topic name of account updates. Omit to disable.
- `slot_status_topic`: Topic name of slot status update. Omit to disable.
- `publish_all_accounts`: Publish all accounts on startup. Omit to disable.
- `program_ignores`: Solana program IDs for which to ignore updates for owned accounts.
- `program_allowlist`: Solana program IDs for which to publish updates for owned accounts.
- `program_allowlist_url`: HTTP URL to fetch the program allowlist from. The file must be json, and with the following schema:
  ```json
  {
    "program_allowlist": [
      "11111111111111111111111111111111",
      "22222222222222222222222222222222"
    ]
  }
  ```
- `program_allowlist_expiry_sec`: Expiry time for the program allowlist cache before fetching it again from the HTTP URL.

## Buffering

The Kafka producer acts strictly non-blocking to allow the Solana validator to sync without much induced lag.
This means incoming events from the Solana validator will get buffered and published asynchronously.

When the publishing buffer is exhausted any additional events will get dropped.
This can happen when Kafka brokers are too slow or the connection to Kafka fails.
Therefor it is crucial to choose a sufficiently large buffer.

The buffer size can be controlled using `librdkafka` config options, including:
- `queue.buffering.max.messages`: Maximum number of messages allowed on the producer queue.
- `queue.buffering.max.kbytes`: Maximum total message size sum allowed on the producer queue.
