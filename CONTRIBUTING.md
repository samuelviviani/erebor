# Contribuire

Grazie! Prima di aprire una PR:

1. Leggi [ARCHITECTURE.md](ARCHITECTURE.md) e [DEVELOPMENT.md](DEVELOPMENT.md).
2. Esegui `pnpm -r typecheck && pnpm -r test && cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
3. Aggiungi test per la logica nuova (soprattutto denaro, commissioni, processi, validazione).
4. Aggiorna `PROGRESS.md` / `CHANGELOG.md` se cambia il comportamento.

## Regole non negoziabili

- Nessun mining senza consenso, nessun avvio nascosto, nessuna offuscazione.
- Nessun indirizzo wallet, token o credenziale nel codice.
- Nessun comando shell costruito da input utente.
- Nessun numero inventato: i dati mancanti si mostrano come "non disponibile".
- Nuove dipendenze solo se indispensabili, con licenza compatibile (aggiornare `THIRD_PARTY_NOTICES.md`).

## Nuovi miner

Implementa `MinerAdapter` (`crates/erebor-core/src/miner.rs`), aggiungi la factory in `engine::default_factory`, un manifest fissato e test con processo simulato. Non dichiarare supporto finché non è verificato.

Contribuendo accetti che il tuo codice sia distribuito con licenza MIT.
