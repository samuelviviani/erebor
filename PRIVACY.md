# Privacy

Erebor is local-first and collects no data.

- **No accounts, telemetry or analytics.**
- **Local data** (the app's data folder): configuration (public payout ID, pool, preferences),
  mining statistics, sessions, the fee ledger, installed miners. It can be exported and
  deleted from the settings.
- **External services** (each sees your IP address):
  - the **pool** (Kryptex): receives your payout ID and worker name while mining;
  - **WhatToMine**, **Kryptex pool stats** and **CoinGecko**: public market data (difficulty,
    rewards, prices), fetched about every 10 minutes, with no personal data;
  - **GitHub**: downloads of the pinned miners, the GPU hashrate table and Erebor's own updates;
  - the miners' **developer-fee servers**: XMRig donates 1% of time; SRBMiner-MULTI and lolMiner
    keep their own fee per algorithm.
- Demo mode makes no network requests for mining or market data.
- The website is static, with no cookies or analytics.

Note: this document describes the software's technical behaviour. A GDPR-compliant notice
for public distribution must be reviewed by a professional (see
[docs/LEGAL-CHECKLIST.md](docs/LEGAL-CHECKLIST.md)).
