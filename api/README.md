# stellar-stream-hive-api

A Rust + Axum REST API that indexes Stellar payment-stream contract activity for
[Stellar Stream Hive](https://github.com/Stellar-Hive). It polls
[Horizon](https://developers.stellar.org/docs/data/horizon) for Soroban contract
events emitted by the Stream contract, keeps an in-memory index of every stream,
and serves it over HTTP.

See the [docs site](../docs) for the full write-up -- in particular
[Concepts: Streaming Math](../docs/app/docs/concepts/streaming-math/page.mdx) for
the accrual formula this API re-implements, and
[API Reference: Overview](../docs/app/docs/api/overview/page.mdx) for endpoint
documentation.

## Endpoints

| Method | Path                    | Description                                      |
|--------|-------------------------|---------------------------------------------------|
| GET    | `/streams`              | All known streams, with live-computed accrual      |
| GET    | `/streams/:id`          | One stream's full detail                          |
| GET    | `/senders/:address`     | Streams sent by an address                        |
| GET    | `/recipients/:address`  | Streams received by an address                    |
| GET    | `/stats`                | Aggregate totals across every indexed stream       |
| GET    | `/health`               | Liveness check, always returns `200 ok`            |
| GET    | `/ready`                | Readiness: indexer sync state and stream count     |

List endpoints accept `?status=active|pending|completed|cancelled`, `?limit=` (1-200,
default 50) and `?offset=`, return streams newest-first, and report the unpaginated
match count in an `X-Total-Count` header. A stream's `status` is derived from the
latest observed ledger, so it moves from `pending` to `active` to `completed` on its own.

## Development

```sh
cargo test            # unit + HTTP route tests
cargo clippy --all-targets -- -D warnings
```

## Running locally

```sh
cp .env.example .env   # optional -- every value has a sane default
cargo run
```

The server binds to `0.0.0.0:8080` by default (override with `BIND_ADDR`). On
startup it spawns a background task that polls Horizon every 10 seconds
(`POLL_INTERVAL_SECS`) for new stream activity.

**Important:** `STREAM_CONTRACT_ID` defaults to an obvious placeholder because
the [contracts repo](https://github.com/Stellar-Hive/stellar-stream-hive-contract)
has not deployed the Stream contract yet. Until you set it to a real `C...`
address, the indexer still runs (Horizon polling, `/health`, `/stats` all work)
but has nothing to match contract events against, so `/streams` will stay empty.
Once a testnet deployment exists, set it via the environment or `.env`:

```sh
STREAM_CONTRACT_ID=C... cargo run
```

## Building on Windows (GNU toolchain)

If `cargo build` fails to link with errors like `cannot find -lkernel32` or
`dlltool ... CreateProcess`, your MinGW-w64 install is either missing the
self-contained runtime libraries or missing `as`/`dlltool`. This repo's
`.cargo/config.toml` already sets `-C link-self-contained=yes` so rustup's own
bundled CRT objects and import libraries are used; you additionally need a
complete MinGW-w64 toolchain (providing a working `as` and `dlltool`) ahead of
rustup's own linker shims on `PATH`, e.g.:

```sh
export PATH="/c/mingw64/bin:$PATH"
cargo build
```

## Testing

```sh
cargo test
```

Unit tests cover the streaming-math module exhaustively (before-start, at-cliff,
quarter/half/full progress, post-end capping, zero-duration streams, and the
cancel fund split) -- see `src/streaming_math.rs`.

## Architecture

- `src/main.rs` -- process entry point: tracing setup, spawns the indexer,
  serves the Axum router with graceful shutdown.
- `src/config.rs` -- environment-driven configuration.
- `src/state.rs` -- in-memory `AppState` (`Arc<RwLock<HashMap<...>>>`), no database.
- `src/models/` -- `Stream`, `StreamStatus`, `StreamView`, `Stats`.
- `src/streaming_math.rs` -- the linear-accrual formula, shared by routes and
  (documentation for) the indexer.
- `src/error.rs` -- a single `ApiError` enum mapped to HTTP status codes.
- `src/routes/` -- one file per resource, each a thin Axum handler over `AppState`.
- `src/indexer/` -- the background Horizon-polling task:
  - `horizon.rs` -- minimal Horizon REST client.
  - `events.rs` -- decodes Soroban `ContractEvent`s out of transaction result meta
    XDR via the `stellar-xdr` crate, matching by contract id.
  - `mod.rs` -- the poll loop that ties the two together and updates `AppState`.

## Why no database

v1 keeps everything in memory. The indexer is the only writer, Horizon is the
only source of truth, and a process restart just means a brief re-sync from
"now" rather than data loss. This keeps the deployment trivial (single binary,
no migrations) while the platform is still young; a persistent store can be
added later behind the same `AppState` interface without touching route
handlers.
