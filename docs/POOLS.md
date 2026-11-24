# Pools

Erebor mines on **Kryptex** with the user's Kryptex ID (`krx…`): every coin is credited in BTC
to the user's Kryptex account. Per-coin hosts and ports are in the coin catalog
(`crates/erebor-core/src/coins.rs`, from the pool.kryptex.com coin pages); the CPU pool
catalog is in `crates/erebor-core/src/pools.rs`. **Re-check every entry on the pool's website
before each release.**

| Id | Host | Port | TLS | Fee | API | Notes |
|---|---|---|---|---|---|---|
| kryptex (default) | xmr.kryptex.network (XMR; other coins per catalog) | 7029 | no (TLS port undocumented) | 1% PPS+ for most coins (Pearl 3%) | no public per-account API: the balance is on kryptex.com | Login `krxID/worker`. Pool fees per coin are read from `pool.kryptex.com/<coin>/api/v1/pool/stats` |
| supportxmr | pool.supportxmr.com | 443 | yes | 0.6% | nodejs-pool `https://www.supportxmr.com/api` | Monero address only (min payout 0.3 XMR) |

## nodejs-pool API

- `GET /network/stats` → `difficulty`, `value` (block reward, atomic units), `height`
- `GET /miner/{addr}/stats` → `amtDue`, `amtPaid` (atomic units), `hash`, `validShares`, `invalidShares`
- `GET /miner/{addr}/payments` → array of `ts`, `amount`, `txnHash`

Missing fields are tolerated and shown as "not available".

## Adding a pool

Add a `PoolInfo` with its documented fee, and an API adapter if it isn't nodejs-pool. Custom
pools work without an API: balance and payments are then checked on the pool's website.

## unMineable (not exposed yet)

The engine can pay a user's own Bitcoin address through unMineable (`BTC:<address>.<worker>`,
TCP port 3333, hosts per algorithm in `coins.rs`). The option is hidden in the app and the
CLI until it is designed and tested properly: today only Kryptex IDs are accepted.
