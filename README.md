# Real Estate Tokenization Program (Solana + Anchor)

This repository contains a Solana smart contract and integration test suite for tokenized real estate fundraising, escrow management, rental income distribution, and redemption workflows.

## Overview

The on-chain program models each property listing as a tokenized offering where:

- A sponsor (authority) creates a listing with fundraising parameters.
- Investors purchase property tokens with USDC.
- Raised USDC is held in escrow until funding conditions are met.
- Once active, the sponsor can fund a rental income vault.
- Investors can claim pro-rata rental income.
- Investors can request redemption (subject to thresholds), and sponsor can approve burn-based redemption.

The codebase is built using Anchor and includes TypeScript integration tests that validate primary and edge-case flows.

## Repository Structure

- `programs/real-estate/src/lib.rs`: Program entrypoint and instruction routing.
- `programs/real-estate/src/admin_instructions.rs`: Sponsor/admin instruction handlers.
- `programs/real-estate/src/investor_instructions.rs`: Investor instruction handlers.
- `programs/real-estate/src/contexts.rs`: Anchor account context structs.
- `programs/real-estate/src/state.rs`: Persistent account state definitions.
- `programs/real-estate/src/events.rs`: Program event payloads.
- `programs/real-estate/src/errors.rs`: Custom error codes.
- `programs/real-estate/src/constants.rs`: Shared constants.
- `tests/real-estate.ts`: End-to-end integration tests (local validator).
- `migrations/deploy.ts`: Deployment helper script.
- `Anchor.toml`: Anchor workspace and script configuration.
- `target/idl/real_estate.json`: Generated IDL.
- `target/types/real_estate.ts`: Generated TypeScript types.

### Instruction Surface by Role

Admin/sponsor-facing instructions:

- `create_listing`
- `release_escrow`
- `fund_rental_vault`
- `approve_redemption`
- `cancel_listing`
- `update_metadata_uri`

Investor-facing instructions:

- `invest`
- `claim_refund`
- `claim_rental_income`
- `request_redemption`

## Feature Set

### Listing Lifecycle

- Create listing with strict validation:
  - token supply, price, and raise target consistency
  - fundraising deadline checks
  - URI format and length checks
  - minimum investment and redemption threshold boundaries
- Listing statuses:
  - `Draft`
  - `Fundraising`
  - `Funded`
  - `Active`
  - `Completed`
  - `Cancelled`

### Investment and Escrow

- Investors buy listing tokens using USDC.
- Funds are moved into an escrow vault owned by the listing PDA.
- Listing transitions to `Funded` once raise target is reached.
- Sponsor releases escrow to sponsor USDC account to transition listing to `Active`.

### Rental Income Distribution

- Sponsor funds a rental vault (USDC).
- Program tracks reward accumulation with fixed-point precision (`PRECISION`).
- Investors claim accrued rental income proportionally to tokens held.
- Pending accrual is tracked per investor position.

### Refunds and Redemption

- Refunds are allowed when listing is:
  - `Cancelled`, or
  - still `Fundraising` after deadline expiry
- Investor tokens are burned on refund settlement.
- Investors can request redemption in active/completed listings.
- Sponsor can approve redemption when threshold conditions are met.

### Metadata

- Metadata creation and URI update hooks are integrated through Metaplex CPI when the metadata program account is executable.
- Local testing remains possible when metadata CPI is skipped.

## Program Accounts

### `PropertyListing`

Stores listing configuration and lifecycle state:

- authority and listing identity
- metadata fields (name, symbol, URI)
- mint and vault addresses
- fundraising economics
- visibility/status fields
- reward accumulator and timestamps

### `InvestorPosition`

Tracks each investor per listing:

- token holdings and USDC invested
- rental reward accounting fields
- redemption request/approval flags

## Security Model

The contract applies layered controls:

- PDA seed derivation for deterministic, authority-bound accounts.
- Anchor `has_one`, seed, and ownership constraints for account integrity.
- Signer requirements on privileged paths.
- Checked arithmetic (`checked_*`) and explicit overflow/underflow errors.
- Status-gated transitions to enforce valid business workflows.
- Mint and token-account constraints to prevent asset mismatch.
- URI, deadline, and amount validation to reduce malformed state entries.

## Events and Observability

The program emits events for key lifecycle transitions, including:

- listing creation
- investment
- escrow release
- refund claim
- rental vault funding
- rental income claim
- redemption request/approval
- listing cancellation
- metadata updates

These events can be indexed by off-chain services for analytics and UI state updates.

## Prerequisites

- Rust toolchain and Cargo
- Solana CLI
- Anchor CLI
- Node.js + Yarn

Suggested checks:

- `solana --version`
- `anchor --version`
- `rustc --version`
- `node --version`
- `yarn --version`

## Build and Test

From repository root:

1. Install JavaScript dependencies:

```bash
yarn install
```

2. Build the Anchor program:

```bash
anchor build
```

3. Run full integration suite (recommended):

```bash
anchor test
```

Current status: all integration tests pass (`9 passing`).

### Running Tests Directly with `ts-mocha`

If running tests directly, ensure `ANCHOR_WALLET` points to a funded local keypair:

```bash
export ANCHOR_WALLET=~/.config/solana/id.json
yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/**/*.ts
```

The test harness also includes a fallback to `~/.config/solana/id.json` when `ANCHOR_WALLET` is unset.

For deterministic listing deadlines in local testing, the suite uses a far-future UNIX timestamp baseline to avoid local-validator block-time drift.

## Common Development Commands

- Lint check:

```bash
yarn lint
```

- Lint auto-fix:

```bash
yarn lint:fix
```

- Full cycle:

```bash
anchor build && anchor test
```

## Known Warnings and Notes

- If you see a native bigint binding warning, tests still run using the JavaScript fallback.
- If module-type warnings appear in Node for TypeScript tests, they are non-fatal unless your local runtime policy enforces strict module type rules.

## Refactoring Notes (Current State)

Recent cleanup focuses include:

- split instruction handlers by role (`admin_instructions.rs` and `investor_instructions.rs`) for clearer frontend integration boundaries
- reduced duplication in tests through helper methods for invest/release flow
- improved test environment robustness by setting provider fallback wallet path
- removal of unused program imports
- extraction of reusable checked arithmetic helpers for clearer logic

## Production Considerations

Before mainnet deployment, evaluate and harden:

- oracle-backed pricing instead of static listing price assumptions
- governance and admin key management (multisig or timelock)
- upgrade authority policy and verifiable build process
- external audit for fund-flow and redemption invariants
- monitoring and alerting on event streams and account anomalies
- formalized migration and backward-compatibility strategy

## Troubleshooting

### `ANCHOR_WALLET` not set

- Set the variable explicitly:

```bash
export ANCHOR_WALLET=~/.config/solana/id.json
```

- Or run via `anchor test`, which provisions the expected environment.

### Failing integration tests after stale local validator state

- Stop running validators and clean local ledgers if needed.
- Re-run:

```bash
anchor build
anchor test
```

## License

License is currently defined in `package.json` as `ISC`.