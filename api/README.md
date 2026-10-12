# stellar-stream-hive-api

A Rust + Axum REST API that indexes Stellar payment-stream contract activity for
[Stellar Stream Hive](https://github.com/Stellar-Hive). It reads the Stream
contract's events from
[Soroban RPC](https://developers.stellar.org/docs/data/apis/rpc) (`getEvents`),
keeps an in-memory index of every stream, and serves it over HTTP. (Horizon no
longer exposes transaction meta, so contract events cannot be read from it.)

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
startup it spawns a background task that polls Soroban RPC every 10 seconds
(`POLL_INTERVAL_SECS`) for new stream events. The first poll reaches back
`INDEXER_BACKFILL_LEDGERS` ledgers (default 17,280, about a day) so a restart
recovers recent activity; every later poll resumes from the last event cursor, so
nothing that lands between polls is missed. `SOROBAN_RPC_URL` selects the RPC
endpoint (default: public testnet).

By default any browser origin may call the API (it is read-only and uses no
cookies). To restrict it to your own frontend, set `CORS_ALLOWED_ORIGINS` to a
comma-separated list such as `https://app.example,https://staging.example`.
Responses expose `X-Total-Count` to browsers so paginated lists work
cross-origin.

**Important:** `STREAM_CONTRACT_ID` defaults to an obvious placeholder so the
service starts without configuration. Until you set it to a real `C...` address,
the indexer still runs (ledger tracking, `/health`, `/stats` all work) but has
nothing to match contract events against, so `/streams` will stay empty. The
testnet deployment's id is in `.env.example` (and in the
[contracts repo's `DEPLOYMENTS.md`](https://github.com/Stellar-Hive/stellar-stream-hive-contract/blob/main/DEPLOYMENTS.md)):

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
- `src/indexer/` -- the background event-polling task:
  - `rpc.rs` -- minimal Soroban RPC client (`getLatestLedger`, `getEvents`).
  - `events.rs` -- decodes `create` / `withdraw` / `cancel` events (base64 XDR topics
    and value) via the `stellar-xdr` crate.
  - `mod.rs` -- the poll loop that ties the two together and updates `AppState`.
  - `testdata/testnet_events.json` -- real `getEvents` output from the testnet
    deployment, replayed by the tests.

## Why no database

v1 keeps everything in memory. The indexer is the only writer, the chain (via Soroban RPC) is the
only source of truth, and a process restart just re-reads recent events
(see `INDEXER_BACKFILL_LEDGERS`) rather than losing data. This keeps the deployment trivial (single binary,
no migrations) while the platform is still young; a persistent store can be
added later behind the same `AppState` interface without touching route
handlers.
