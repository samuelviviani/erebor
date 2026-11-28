# Security Policy

Erebor is proprietary, source-available mining and infrastructure management software. Its source code is publicly accessible to support transparency, independent security review and contributions to the official project under the [Erebor Source-Available License (ESAL v1.0)](./LICENSE.md).

## Supported versions

| Version | Security support |
| --- | --- |
| Latest stable release | Supported |
| Previous releases | Not supported |
| Development builds | Best effort; not recommended for production |

Please update to the latest stable release before testing where practical. Reports affecting older versions are welcome, particularly if the issue may also affect the current release.

## Reporting a vulnerability

Please do not open public issues or pull requests containing undisclosed vulnerability details.

1. Submit a private report through [GitHub private vulnerability reporting](https://github.com/samuelviviani/erebor/security/advisories/new).
2. If private reporting is unavailable, contact [contact@samuelviviani.com](mailto:contact@samuelviviani.com) with the subject `Erebor Security Report`. Request a secure communication channel before sending sensitive material.
3. Include the affected version or commit, operating system, affected component, prerequisites, reproduction steps, expected and actual behavior, potential impact, and any suggested mitigation.
4. Remove wallet secrets, authentication tokens, private keys, pool passwords and personal information from logs and attachments.

The maintainer aims to acknowledge reports within **7 calendar days**. This is a target, not a guaranteed response or remediation deadline. Remediation depends on severity, reproducibility, available resources and third-party dependencies.

## Coordinated disclosure and security research

Good-faith security research on Erebor is welcome. Consistent with ESAL v1.0, the maintainer authorizes inspection of the source code and local testing, including modifications strictly necessary to develop and validate proposed security fixes for the official project.

Research must be performed on systems you own or have explicit permission to test. Use isolated environments, test credentials and synthetic data where possible. Do not access other users' devices, accounts, wallets or infrastructure without authorization; disrupt services; obtain unauthorized financial benefits; or retain data belonging to third parties.

If sensitive third-party data is encountered, stop the relevant test, minimize further access and report the incident privately.

Please coordinate publication of vulnerability details with the maintainer to allow time for remediation. This request does not override legally protected disclosure or research rights. For research conducted in good faith within this policy, the maintainer does not intend to pursue claims based solely on the authorized activities. This assurance applies only to rights controlled by the maintainer and does not bind third parties.

Security fixes should initially be shared through the private advisory process. Public forks and pull requests containing security fixes should be coordinated with the maintainer when premature disclosure could put users at risk. Contributions remain subject to ESAL v1.0.

Attribution may be included in security advisories with the reporter's consent. No financial reward is promised by this policy.

## Security model

The following describes the project's documented implementation and intended security boundaries. It is not a guarantee of immunity from vulnerabilities or an independent audit certification.

### Wallet and payout data

- Erebor is non-custodial and does not require cryptocurrency wallet seed phrases or wallet private keys.
- Public payout identifiers are stored. Inputs recognized as seed phrases or wallet private keys are refused.
- Remote management uses separate authentication credentials. ASIC pool credentials may also be handled where required. These are distinct from wallet secrets.
- Public payout addresses can still reveal financial activity and should be treated as potentially privacy-sensitive.

### Miner integrity and execution

- Supported miners (XMRig, SRBMiner-MULTI, lolMiner and cpuminer-opt) are installed by default using manifests containing pinned SHA-256 hashes and download URLs restricted to the respective upstream projects' GitHub releases.
- Manually selected binaries are marked as unverified. Miner hashes are checked again at startup.
- Hash verification establishes integrity relative to the expected hash. Authenticity depends on the trustworthiness of the manifest and upstream release process.
- Miner processes are launched without shell interpretation, using validated configuration fields and structured arguments.
- Third-party miners execute native code and remain a separate trust boundary. Hash verification does not establish that a binary is safe or sandboxed.

### Local interfaces and permissions

- Miner APIs bind to `127.0.0.1`, using random per-session tokens where supported by the miner.
- On Unix-like systems, sensitive configuration files and local agent sockets use `0600` permissions; the documented local agent design checks peer UIDs on both sides.
- Remote network access requires explicit enablement.
- The desktop application uses restricted Tauri permissions and a restrictive Content Security Policy (CSP). Documented capabilities include core defaults, file selection, updating, restarting and opening an allowlist of payout websites.
- Miner API tokens are intended to be excluded from displayed errors and logs. Review diagnostic files before sharing them.

### Persistence and updates

- Start at login, mining on launch and always-on agent services are opt-in.
- Mining stops when the application exits normally unless the agent is intentionally running as a service. Orphaned miner cleanup is performed on the next startup.
- The application and CLI verify update signatures against the project minisign public key before installing updates.
- Private release-signing keys must remain outside the repository. Update signatures are distinct from operating-system code signing and notarization.

## Remote management

Remote management is disabled until explicitly configured using `erebor-agent init`.

### Transport security and pairing

- Remote connections use mutual TLS 1.3 with self-signed certificates and SHA-256 certificate fingerprint pinning established during pairing.
- The agent maintains trusted controller identities; controllers pin the agent identity. No project-operated central account or certificate authority is required for this design.
- Pairing uses a one-time code with a documented 60-bit entropy target, a 10-minute validity period and a five-attempt limit. HMAC-based proof binds pairing to both certificates.
- Controllers have individually revocable credentials. `erebor-agent revoke` is intended to revoke access on subsequent requests and active event streams. `erebor-agent rotate-key` rotates the agent key and requires controllers to pair again.
- TLS protects normal encrypted connection records. Application requests additionally carry timestamps checked within a ±10-minute window. Timestamp validation alone does not provide complete application-level duplicate-request protection.
- Documented rate limits include a maximum of 32 connections and a 10-minute address lockout following 10 failed pairings or unauthorized requests.

### Command authorization

- Remote operations are restricted to an explicit command list. Arbitrary shell commands, executable selection and arbitrary file paths are not exposed as supported remote operations.
- Remote configuration passes through engine validation. Miner paths and local-only settings cannot be changed through the documented remote interface. Remote profiles are limited to mining-related keys.
- The ASIC gateway is restricted to user-configured devices and private addresses. Network discovery is user-initiated and limited to a private `/22`. Operations are restricted to supported device capabilities.
- Pairing, refusals, revocations and configuration changes are recorded in `remote/audit.log`. Local audit logs are not claimed to be tamper-proof.

### Credential storage and deployment

- On Unix-like systems, TLS private keys are stored in `0600` files inside `0700` directories. System services use a dedicated account according to the documented deployment design.
- OS keychain integration is not implemented. Equivalent restrictive Windows ACLs should be verified before production deployment.
- The controller database is intended to exclude secret key material. Exports omit keys and ASIC pool passwords by default; explicitly including them creates sensitive files that must be protected.
- Remote management requires network reachability between the controller and agent. Port forwarding is not required on an appropriately configured LAN or private VPN, but may be necessary in other network configurations.
- Bind the agent only to the intended interface, restrict inbound access with a firewall and prefer a trusted private network or VPN. Do not expose the agent or ASIC management interfaces directly to the public internet.

## Known limitations and outstanding verification

- Operating systems may display warnings for releases without Authenticode signing or Apple signing/notarization.
- XMRig's `SHA256SUMS` GPG signature is not automatically verified by the manifest script. The documented workflow relies on manual maintainer verification in addition to hash checks.
- SRBMiner-MULTI and lolMiner are closed-source third-party miners. Erebor cannot independently establish their internal behavior through source review.
- Application-level replay prevention using unique request identifiers and duplicate detection has not been established by the current documentation. TLS 0-RTT handling should also be verified before relying on replay-sensitive remote operations.
- OS keychain integration is not currently implemented. Windows credential-file ACL enforcement requires verification.
- The agent's systemd unit and Windows scheduled task have not yet been verified on real machines, according to the current project documentation.
- WhatsMiner encrypted write commands are not implemented. Physical ASIC devices have not yet been tested.
- Local administrators, compromised operating systems and compromised upstream dependencies remain outside the guarantees of the documented security model.

## Security advisories

Confirmed vulnerabilities may be documented through [GitHub Security Advisories](https://github.com/samuelviviani/erebor/security/advisories). Advisories should identify affected versions, impact, available fixes and mitigations where appropriate.

## License and contact

Security research and contributions are governed by this policy, applicable law and the [Erebor Source-Available License](./LICENSE.md). Nothing in this policy limits mandatory statutory rights.

- Maintainer: Samuel Viviani
- Contact: [contact@samuelviviani.com](mailto:contact@samuelviviani.com)
- Official repository: <https://github.com/samuelviviani/erebor>
