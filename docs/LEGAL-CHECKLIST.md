# Legal and business checks still open

**No legal review has been done.** Before wider or commercial distribution:

- [ ] **Name and trademark "Erebor"**: prior-art search (EUIPO, USPTO, WIPO), risks around
      Tolkien intellectual property (Middle-earth Enterprises / Tolkien Estate) and existing
      crypto/finance marks.
- [ ] **Domain** and app identifier (`org.erebor.desktop` in `tauri.conf.json`).
- [ ] **Privacy policy** compliant with the GDPR (controller, legal bases, transfers outside
      the EU to pools, CoinGecko, WhatToMine, GitHub; data subject rights).
- [ ] **Terms of use** and limitation of liability reviewed.
- [ ] **Dev fee**: tax treatment of the project's revenue (crypto-assets) and any disclosure
      duties; consumer-law disclosures (EU Directive 2011/83/EU) if it counts as a paid service.
- [ ] **MiCA**: confirm that non-custodial software that never handles funds is not a
      regulated service.
- [ ] **Local mining rules** (some countries restrict it) and power-use warnings.
- [ ] **Licences**: generate the full list of transitive dependencies; confirm GPL
      obligations if a GPL miner is ever bundled in the packages.
- [ ] **CoinGecko and WhatToMine terms** for use in publicly distributed software (API key,
      attribution).
- [ ] **Code signing** and SmartScreen/antivirus reputation (miners are often flagged as PUA).

## Closed-source miners

- [ ] Ask the SRBMiner-MULTI author (doktor83) for written confirmation that downloading the
      official release and running it unmodified from Erebor is allowed by the licence
      ("incorporating into another product" is forbidden without permission).
- [ ] Same question to the lolMiner author.
