# Stellar Stream Hive API + Docs

A Rust/Axum REST API that indexes payment-stream activity on Stellar, plus the
documentation site for the whole Stellar Stream Hive platform (contracts + API
+ web app).

Stellar Stream Hive is a payment-streaming platform built on Stellar and
Soroban: a sender locks funds once and a recipient's claim on them vests
linearly over time, ledger by ledger, rather than arriving as a single
lump-sum transfer. See
[**Getting Started**](docs/app/docs/getting-started/page.mdx) for what that
means in practice, and
[**Streaming Math**](docs/app/docs/concepts/streaming-math/page.mdx) for the
exact accrual formula, derived from first principles with a worked example.

## Repositories

Stellar Stream Hive is split across three repositories:

| Repo | What it is |
|---|---|
| [`stellar-stream-hive-api`](https://github.com/Stellar-Hive/stellar-stream-hive-api) (this repo) | The indexer API (`api/`) and documentation site (`docs/`). |
| [`stellar-stream-hive-contract`](https://github.com/Stellar-Hive/stellar-stream-hive-contract) | The Soroban smart contracts: Stream, Vault, Registry. |
| [`stellar-stream-hive-frontend`](https://github.com/Stellar-Hive/stellar-stream-hive-frontend) | The web app (Freighter wallet integration, stream creation/management UI). |

## This repository's layout

```
api/    Rust + Axum REST API + Horizon-polling indexer  (maintainer-only, see CONTRIBUTING.md)
docs/   Next.js 14 + TypeScript + Tailwind + MDX documentation site (open to contributions)
```

### `api/`

Polls [Horizon](https://developers.stellar.org/docs/data/horizon) every 10
seconds, decodes Soroban contract events emitted by the Stream contract, and
serves the resulting index over a small REST API (`GET /streams`,
`/streams/:id`, `/senders/:address`, `/recipients/:address`, `/stats`). No
database -- everything lives in memory for v1. See
[`api/README.md`](api/README.md) for how to run it, environment variables,
and the full architecture breakdown.

```bash
cd api
cargo run
```

### `docs/`

This documentation site itself -- covers the contracts, the API, the web app,
and how to integrate with the platform. Built with Next.js 14 (app router),
TypeScript, Tailwind CSS, and MDX.

```bash
cd docs
npm install
npm run dev
```

## Contributing

**Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.**
In short: `docs/` is open to contributions, `api/` is maintainer-only, and PRs
touching `api/` will be closed without review.

## Status

The contracts (`stellar-stream-hive-contract`) have not been deployed to any
network yet. The API and docs in this repository are built and ready, but
`STREAM_CONTRACT_ID` is an explicit placeholder until a real deployment
exists -- see
[Getting Started: deployed contract addresses](docs/app/docs/getting-started/page.mdx#deployed-contract-addresses).
Nothing in this repository fabricates a real contract address anywhere.

## License

Apache-2.0.
