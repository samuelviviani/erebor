//! Payout identifiers: Monero addresses (syntax, Monero base58, Keccak checksum,
//! network), Bitcoin mainnet addresses (bech32/bech32m or base58check) and Kryptex
//! account IDs (`krx…`, earnings credited to the Kryptex account).
//! Erebor never asks for seeds or private keys: only public identifiers are handled.

use serde::Serialize;
use tiny_keccak::{Hasher, Keccak};

const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
/// Encoded length (chars) for a block of N bytes, N = index.
const ENCODED_BLOCK_SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet,
    Testnet,
    Stagenet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AddressKind {
    Standard,
    Integrated,
    Subaddress,
    /// Kryptex "Mining Username" (account mining, paid out by Kryptex).
    Kryptex,
    /// Bitcoin mainnet address (paid out in BTC by a converting pool).
    Bitcoin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddressInfo {
    pub network: Network,
    pub kind: AddressKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum WalletError {
    Empty,
    /// Looks like a seed phrase or private key: never accepted.
    LooksLikeSecret,
    InvalidCharacters,
    InvalidLength,
    InvalidEncoding,
    UnknownPrefix,
    BadChecksum,
    WrongNetwork {
        network: Network,
    },
    /// For now Erebor pays out through a Kryptex account only.
    KryptexOnly,
}

fn decode_block(block: &[u8], out_size: usize) -> Option<Vec<u8>> {
    let mut num: u128 = 0;
    for &c in block {
        let digit = ALPHABET.iter().position(|&a| a == c)? as u128;
        num = num.checked_mul(58)?.checked_add(digit)?;
    }
    if out_size < 8 && num >= 1u128 << (8 * out_size) {
        return None;
    }
    if num > u64::MAX as u128 {
        return None;
    }
    let bytes = (num as u64).to_be_bytes();
    Some(bytes[8 - out_size..].to_vec())
}

/// Monero-flavoured base58 (8-byte blocks -> 11 chars).
pub fn base58_decode(s: &str) -> Option<Vec<u8>> {
    let raw = s.as_bytes();
    let full = raw.len() / 11;
    let rem = raw.len() % 11;
    let last_size = ENCODED_BLOCK_SIZES.iter().position(|&n| n == rem)?;
    let mut out = Vec::with_capacity(full * 8 + last_size);
    for i in 0..full {
        out.extend(decode_block(&raw[i * 11..i * 11 + 11], 8)?);
    }
    if rem > 0 {
        out.extend(decode_block(&raw[full * 11..], last_size)?);
    }
    Some(out)
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut k = Keccak::v256();
    k.update(data);
    let mut out = [0u8; 32];
    k.finalize(&mut out);
    out
}

fn prefix_info(prefix: u8) -> Option<(Network, AddressKind)> {
    use AddressKind::*;
    use Network::*;
    Some(match prefix {
        18 => (Mainnet, Standard),
        19 => (Mainnet, Integrated),
        42 => (Mainnet, Subaddress),
        53 => (Testnet, Standard),
        54 => (Testnet, Integrated),
        63 => (Testnet, Subaddress),
        24 => (Stagenet, Standard),
        25 => (Stagenet, Integrated),
        36 => (Stagenet, Subaddress),
        _ => return None,
    })
}

/// Parse any Monero address (any network).
pub fn parse_monero_address(input: &str) -> Result<AddressInfo, WalletError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(WalletError::Empty);
    }
    // Seed phrases contain spaces; 64-hex strings are private keys.
    if s.split_whitespace().count() >= 12
        || (s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(WalletError::LooksLikeSecret);
    }
    if !s.bytes().all(|b| ALPHABET.contains(&b)) {
        return Err(WalletError::InvalidCharacters);
    }
    if s.len() != 95 && s.len() != 106 {
        return Err(WalletError::InvalidLength);
    }
    let data = base58_decode(s).ok_or(WalletError::InvalidEncoding)?;
    let (network, kind) = prefix_info(data[0]).ok_or(WalletError::UnknownPrefix)?;
    let expected_len = match kind {
        AddressKind::Integrated => 1 + 64 + 8 + 4,
        _ => 1 + 64 + 4,
    };
    if data.len() != expected_len {
        return Err(WalletError::InvalidLength);
    }
    let (body, checksum) = data.split_at(data.len() - 4);
    if keccak256(body)[..4] != *checksum {
        return Err(WalletError::BadChecksum);
    }
    Ok(AddressInfo { network, kind })
}

/// Kryptex Mining Username: `krx` + Latin letters/digits (case-insensitive).
pub fn is_kryptex_id(input: &str) -> bool {
    let s = input.trim();
    s.len() >= 6
        && s.len() <= 32
        && s[..3].eq_ignore_ascii_case("krx")
        && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// What a user may enter as payout today: a Kryptex ID only. Monero and Bitcoin payouts
/// (`validate_payout`) stay in the engine and come back once they are designed properly.
pub fn validate_user_payout(input: &str) -> Result<AddressInfo, WalletError> {
    if input.trim().is_empty() {
        return Err(WalletError::Empty);
    }
    match validate_payout(input) {
        Ok(i) if i.kind == AddressKind::Kryptex => Ok(i),
        Err(WalletError::LooksLikeSecret) => Err(WalletError::LooksLikeSecret),
        _ => Err(WalletError::KryptexOnly),
    }
}

/// Validate what the user mines to: a Kryptex ID, a Bitcoin or a Monero mainnet address.
pub fn validate_payout(input: &str) -> Result<AddressInfo, WalletError> {
    if is_kryptex_id(input) {
        return Ok(AddressInfo {
            network: Network::Mainnet,
            kind: AddressKind::Kryptex,
        });
    }
    let s = input.trim();
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("bc1")
        || lower.starts_with("tb1")
        || s.starts_with('1')
        || s.starts_with('3')
    {
        return validate_bitcoin_address(s).map(|_| AddressInfo {
            network: Network::Mainnet,
            kind: AddressKind::Bitcoin,
        });
    }
    validate_mainnet_address(input)
}

/// Own wallet address for a specific coin (mined on Kryptex as `address/worker`).
/// Formats from unMineable's published patterns (api.unminable.com/v4/coin/<TICKER>);
/// Monero gets the full checksum check. Seed phrases and private keys are refused.
pub fn validate_coin_address(coin: &str, input: &str) -> Result<(), WalletError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(WalletError::Empty);
    }
    if s.split_whitespace().count() >= 12
        || (s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(WalletError::LooksLikeSecret);
    }
    let alnum = |t: &str| t.bytes().all(|b| b.is_ascii_alphanumeric());
    let ok = match coin {
        "XMR" => return validate_mainnet_address(s).map(|_| ()),
        "PRL" => {
            s.len() == 63
                && s.starts_with("prl1p")
                && s[5..]
                    .bytes()
                    .all(|b| b"023456789acdefghjklmnpqrstuvwxyz".contains(&b))
        }
        "XEL" => {
            s.len() > 4
                && s.starts_with("xel:")
                && s[4..]
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        }
        "ERG" => s.len() == 51 && s.starts_with('9') && alnum(s),
        "RVN" => (34..=35).contains(&s.len()) && s[..1].eq_ignore_ascii_case("r") && alnum(s),
        "ETC" | "ETHW" => {
            s.len() == 42 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(WalletError::UnknownPrefix)
    }
}

/// What kind of payout a (validated) wallet string is.
pub fn payout_kind(input: &str) -> Option<AddressKind> {
    validate_payout(input).ok().map(|i| i.kind)
}

// ---------- Bitcoin ----------

fn bech32_polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for v in values {
        let b = chk >> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ *v as u32;
        for (i, g) in GEN.iter().enumerate() {
            if (b >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

/// Segwit address (BIP173 bech32 for v0, BIP350 bech32m for v1+), mainnet only.
fn validate_segwit(s: &str) -> Result<(), WalletError> {
    const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    if s.chars().any(|c| c.is_ascii_lowercase()) && s.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(WalletError::InvalidCharacters);
    }
    let s = s.to_ascii_lowercase();
    let (hrp, data) = s.rsplit_once('1').ok_or(WalletError::InvalidEncoding)?;
    if hrp != "bc" {
        return Err(WalletError::WrongNetwork {
            network: Network::Testnet,
        });
    }
    if !(14..=74).contains(&s.len()) || data.len() < 7 {
        return Err(WalletError::InvalidLength);
    }
    let values: Vec<u8> = data
        .bytes()
        .map(|c| CHARSET.iter().position(|x| *x == c).map(|p| p as u8))
        .collect::<Option<_>>()
        .ok_or(WalletError::InvalidCharacters)?;
    let mut expanded: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    expanded.push(0);
    expanded.extend(hrp.bytes().map(|b| b & 31));
    expanded.extend(&values);
    let version = values[0];
    let constant = match bech32_polymod(&expanded) {
        1 => 1,
        0x2bc830a3 => 0x2bc830a3,
        _ => return Err(WalletError::BadChecksum),
    };
    // v0 must use bech32, v1+ bech32m.
    if (version == 0) != (constant == 1) || version > 16 {
        return Err(WalletError::BadChecksum);
    }
    // Witness program: convert 5-bit groups (without version and checksum) to bytes.
    let (mut acc, mut bits, mut prog) = (0u32, 0u32, Vec::new());
    for v in &values[1..values.len() - 6] {
        acc = (acc << 5) | *v as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            prog.push((acc >> bits) as u8);
        }
    }
    if bits >= 5 || (acc & ((1 << bits) - 1)) != 0 {
        return Err(WalletError::InvalidEncoding);
    }
    let ok = match version {
        0 => prog.len() == 20 || prog.len() == 32,
        _ => (2..=40).contains(&prog.len()),
    };
    if ok {
        Ok(())
    } else {
        Err(WalletError::InvalidLength)
    }
}

/// Standard (big-integer) base58 decode, as used by Bitcoin.
fn base58_decode_btc(s: &str) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    for c in s.bytes() {
        let mut carry = ALPHABET.iter().position(|x| *x == c)? as u32;
        for b in out.iter_mut().rev() {
            carry += (*b as u32) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            out.insert(0, (carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let zeros = s.bytes().take_while(|c| *c == b'1').count();
    let mut v = vec![0u8; zeros];
    v.extend(out);
    Some(v)
}

/// Bitcoin mainnet address: bech32/bech32m (`bc1…`) or base58check P2PKH/P2SH (`1…`/`3…`).
pub fn validate_bitcoin_address(input: &str) -> Result<(), WalletError> {
    use sha2::{Digest, Sha256};
    let s = input.trim();
    if s.is_empty() {
        return Err(WalletError::Empty);
    }
    if s.to_ascii_lowercase().starts_with("bc1") || s.to_ascii_lowercase().starts_with("tb1") {
        return validate_segwit(s);
    }
    if !s.bytes().all(|b| ALPHABET.contains(&b)) {
        return Err(WalletError::InvalidCharacters);
    }
    if !(26..=35).contains(&s.len()) {
        return Err(WalletError::InvalidLength);
    }
    let data = base58_decode_btc(s).ok_or(WalletError::InvalidEncoding)?;
    if data.len() != 25 {
        return Err(WalletError::InvalidLength);
    }
    let (body, checksum) = data.split_at(21);
    if Sha256::digest(Sha256::digest(body))[..4] != *checksum {
        return Err(WalletError::BadChecksum);
    }
    match body[0] {
        0x00 | 0x05 => Ok(()),
        _ => Err(WalletError::WrongNetwork {
            network: Network::Testnet,
        }),
    }
}

/// Validate an address for mining on Monero mainnet.
pub fn validate_mainnet_address(input: &str) -> Result<AddressInfo, WalletError> {
    let info = parse_monero_address(input)?;
    if info.network != Network::Mainnet {
        return Err(WalletError::WrongNetwork {
            network: info.network,
        });
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Public donation address published in the XMRig README. Used only as a
    // validation test vector, never as a mining destination.
    const XMRIG_DONATION: &str = "48edfHu7V9Z84YzzMa6fUueoELZ9ZRXq9VetWzYGzKt52XU5xvqgzYnDK9URnRoJMk1j8nLwEVsaSWJ4fhdUyZijBGUicoD";

    #[test]
    fn accepts_valid_mainnet_standard_address() {
        let info = validate_mainnet_address(XMRIG_DONATION).unwrap();
        assert_eq!(info.kind, AddressKind::Standard);
    }

    #[test]
    fn trims_whitespace() {
        assert!(validate_mainnet_address(&format!("  {XMRIG_DONATION}\n")).is_ok());
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut s = XMRIG_DONATION.to_string();
        // Change the last char to another valid base58 char.
        let last = s.pop().unwrap();
        s.push(if last == 'D' { 'E' } else { 'D' });
        assert!(matches!(
            validate_mainnet_address(&s),
            Err(WalletError::BadChecksum) | Err(WalletError::InvalidEncoding)
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(validate_mainnet_address(""), Err(WalletError::Empty));
        assert_eq!(
            validate_mainnet_address("0OIl"),
            Err(WalletError::InvalidCharacters)
        );
        assert_eq!(
            validate_mainnet_address("48edf"),
            Err(WalletError::InvalidLength)
        );
    }

    #[test]
    fn rejects_secrets() {
        let seed = "abbey ability able about above absent absorb abstract absurd abuse access accident account";
        assert_eq!(
            validate_mainnet_address(seed),
            Err(WalletError::LooksLikeSecret)
        );
        let key = "a".repeat(64);
        assert_eq!(
            validate_mainnet_address(&key),
            Err(WalletError::LooksLikeSecret)
        );
    }

    #[test]
    fn base58_block_roundtrip_sizes() {
        // "11" decodes to a single zero byte (2 chars -> 1 byte).
        assert_eq!(base58_decode("11"), Some(vec![0]));
        // Invalid trailing length (1 char) is rejected.
        assert_eq!(base58_decode("1"), None);
    }

    #[test]
    fn kryptex_ids() {
        assert_eq!(
            validate_payout("krx1a2b3c").unwrap().kind,
            AddressKind::Kryptex
        );
        assert!(is_kryptex_id("KRXABCDEF"));
        assert!(!is_kryptex_id("krx12"));
        assert!(!is_kryptex_id("krx12/worker"));
        assert!(!is_kryptex_id("krx12 34567"));
        assert!(validate_payout("krx").is_err());
    }

    #[test]
    fn bitcoin_addresses() {
        // Synthetic vectors built with the BIP173/BIP350 reference encoder.
        let v0 = "bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345";
        let v1 = "bc1pqqqsyqcyq5rqwzqfpg9scrgwpugpzysnzs23v9ccrydpk8qarc0sg5tmnz";
        assert_eq!(validate_payout(v0).unwrap().kind, AddressKind::Bitcoin);
        assert_eq!(
            validate_payout(&v0.to_uppercase()).unwrap().kind,
            AddressKind::Bitcoin
        );
        assert!(validate_bitcoin_address(v1).is_ok());
        // Genesis block P2PKH address (base58check, version 0x00).
        assert!(validate_bitcoin_address("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").is_ok());
        // One character changed: checksum fails.
        assert_eq!(
            validate_bitcoin_address("bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0346"),
            Err(WalletError::BadChecksum)
        );
        assert_eq!(
            validate_bitcoin_address("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb"),
            Err(WalletError::BadChecksum)
        );
        // Testnet and mixed case are rejected.
        assert!(matches!(
            validate_payout("tb1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysnl25zw8"),
            Err(WalletError::WrongNetwork { .. })
        ));
        assert!(validate_bitcoin_address("bc1Qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345").is_err());
    }

    #[test]
    fn coin_addresses() {
        let prl = format!("prl1p{}", "q".repeat(58));
        assert!(validate_coin_address("PRL", &prl).is_ok());
        assert!(validate_coin_address("PRL", "prl1pbad").is_err());
        assert!(validate_coin_address("ETC", &format!("0x{}", "a1".repeat(20))).is_ok());
        assert!(validate_coin_address("ETC", "0x123").is_err());
        assert!(validate_coin_address("XEL", "xel:abc123").is_ok());
        assert!(validate_coin_address("XEL", "XEL:ABC").is_err());
        assert!(validate_coin_address("ERG", &format!("9{}", "f".repeat(50))).is_ok());
        assert!(validate_coin_address("RVN", &format!("R{}", "a".repeat(33))).is_ok());
        assert_eq!(
            validate_coin_address("PRL", "abbey ability able about above absent absorb abstract absurd abuse access accident"),
            Err(WalletError::LooksLikeSecret)
        );
        assert!(validate_coin_address("BTC", "anything").is_err());
    }
}
