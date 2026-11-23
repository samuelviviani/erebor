# Pool

Catalogo in `crates/erebor-core/src/pools.rs`. Ogni voce va **ricontrollata prima di ogni release** sul sito della pool.

| Id | Host | Porta | TLS | Fee | API | Verificato il |
|---|---|---|---|---|---|---|
| kryptex (predefinita) | xmr.kryptex.network | 7029 | no (porta TLS non documentata) | 1% PPS+ | nessuna API pubblica per account: saldo su kryptex.com | ✅ 2026-11-16: host/porta/fee dalla pagina pool.kryptex.com/xmr; TCP raggiungibile. Accetta ID Kryptex (`krxID/worker`, accredito in BTC) o indirizzo XMR (payout min 0,01 XMR). La 8029 risponde in TLS (cert kryptex.com) ma non è documentata: non usata |
| supportxmr | pool.supportxmr.com | 443 | sì | 0,6% | nodejs-pool `https://www.supportxmr.com/api` | ✅ verificato 2026-11-16: TCP+TLS 1.3 su 443, `/api/config` pplns_fee 0.6, payout minimo 0,3 XMR |

## API nodejs-pool usate

- `GET /network/stats` → `difficulty`, `value` (ricompensa blocco, unità atomiche), `height`
- `GET /miner/{addr}/stats` → `amtDue`, `amtPaid` (atomiche), `hash`, `validShares`, `invalidShares`
- `GET /miner/{addr}/payments` → array con `ts`, `amount`, `txnHash`

Il parser tollera campi mancanti (mostrati come "non disponibile").

## Aggiungere una pool

Aggiungi una `PoolInfo` con fee documentata e un adapter API se diverso da nodejs-pool. Pool personalizzate funzionano senza API: saldo e pagamenti vanno controllati sul sito della pool; la difficoltà di rete per le stime viene presa dalla prima pool del catalogo.

## unMineable (pagamenti in BTC sul wallet dell'utente)

Verificato il 2026-11-16 su unmineable.com/coins/BTC e api.unminable.com/v4/coin/BTC:
- Host per algoritmo: `pearlpow`, `xelishash`, `autolykos`, `etchash`, `ethash`, `rx` `.unmineable.com`; TCP 3333/13333/80, SSL 4444/443. Erebor usa TCP 3333.
- Accesso: `BTC:<indirizzo>.<worker>`, password `x` (formato dal wrapper XMRig di riferimento; nessun codice referral).
- Soglia BTC 0,00075, pagamento giornaliero. Fee ~1% **non confermata da pagina ufficiale**. Ravencoin (KawPow) non elencato.
- Da verificare alla prima prova reale: accettazione del login con SRBMiner su `pearlpow.unmineable.com:3333`.
