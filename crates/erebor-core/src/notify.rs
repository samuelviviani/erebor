//! Notifications for problems (warnings and errors only): the texts, shared by the
//! desktop app's system notifications, and the Telegram bot the user may set up.
//! Outgoing only: nothing listens.

use crate::config::Notify;
use std::time::Duration;

/// Message in the user's language. Codes without a sentence fall back to `code: detail`.
pub fn text(lang: &str, code: &str, detail: Option<&str>) -> String {
    let it = lang == "it";
    let d = detail.unwrap_or("");
    let s = match code {
        "miner_crashed" if it => format!("Il miner si è chiuso: {d}"),
        "miner_crashed" => format!("The miner exited: {d}"),
        "temperature_high" if it => format!("Temperatura alta: {d}"),
        "temperature_high" => format!("High temperature: {d}"),
        "stopped_overheat" if it => format!("Mining fermato per surriscaldamento ({d})"),
        "stopped_overheat" => format!("Mining stopped: overheating ({d})"),
        "hashrate_dropped" if it => format!("Hashrate crollato ({d}): riavvio il miner"),
        "hashrate_dropped" => format!("Hashrate dropped ({d}): restarting the miner"),
        "hashrate_low" if it => format!("Hashrate ancora basso dopo il riavvio ({d})"),
        "hashrate_low" => format!("Hashrate still low after a restart ({d})"),
        "restart_failed" if it => format!("Riavvio del miner non riuscito: {d}"),
        "restart_failed" => format!("Miner restart failed: {d}"),
        "miner_api_unreachable" if it => "Il miner non risponde".into(),
        "miner_api_unreachable" => "The miner is not responding".into(),
        "pool_failover" if it => {
            format!("Il pool non risponde: continuo sul pool di riserva ({d})")
        }
        "pool_failover" => {
            format!("The pool isn't answering: carrying on with the backup pool ({d})")
        }
        "switch_failed_restarting" if it => "Cambio non riuscito: riavvio il miner".into(),
        "switch_failed_restarting" => "Switch failed: restarting the miner".into(),
        "storage_error" if it => "Non riesco a salvare i dati su questo computer".into(),
        "storage_error" => "Data can't be saved on this computer".into(),
        "schedule_start_failed" if it => "Il mining non è partito all'orario programmato".into(),
        "schedule_start_failed" => "Mining couldn't start at the scheduled time".into(),
        "miner_update_failed" if it => {
            "Aggiornamento del miner non riuscito: riprovo più tardi".into()
        }
        "miner_update_failed" => "Miner update failed: it will be retried later".into(),
        "test" if it => "Notifiche attive ✓".into(),
        "test" => "Notifications are on ✓".into(),
        _ if d.is_empty() => code.into(),
        _ => format!("{code}: {d}"),
    };
    s
}

/// A device's alert (`controller` events) in the user's language. Raw errors in the
/// detail stay out: the Alerts page shows them.
pub fn device_text(lang: &str, code: &str, detail: Option<&str>) -> String {
    let it = lang == "it";
    let base = match code {
        "device_offline" if it => "È andato offline",
        "device_offline" => "Went offline",
        "auth_failed" if it => "Non autorizzato: abbinalo di nuovo",
        "auth_failed" => "Not authorized: pair it again",
        "miner_crashed" if it => "Il miner si è chiuso",
        "miner_crashed" => "The miner crashed",
        "miner_stopped" if it => "Il mining si è fermato",
        "miner_stopped" => "Mining stopped",
        "temperature_high" if it => "Temperatura alta",
        "temperature_high" => "High temperature",
        "hashrate_low" if it => "Hashrate sotto il normale",
        "hashrate_low" => "Hashrate below normal",
        "power_high" if it => "Consumo sopra il normale",
        "power_high" => "Power above normal",
        "rejected_high" if it => "Molte share rifiutate",
        "rejected_high" => "Many rejected shares",
        "pool_unreachable" if it => "Pool non raggiungibile",
        "pool_unreachable" => "Pool not reachable",
        _ => return text(lang, code, detail),
    };
    let measured = matches!(
        code,
        "temperature_high" | "hashrate_low" | "power_high" | "rejected_high"
    );
    match detail {
        Some(d) if measured && !d.is_empty() => format!("{base} ({d})"),
        _ => base.into(),
    }
}

/// Now inside the user's quiet hours (local time)?
pub fn quiet_now(n: &Notify) -> bool {
    use chrono::Timelike;
    let now = chrono::Local::now();
    n.is_quiet((now.hour() * 60 + now.minute()) as u16)
}

/// Telegram is set up.
pub fn has_telegram(n: &Notify) -> bool {
    !n.telegram_token.is_empty() && !n.telegram_chat.is_empty()
}

/// The previous calendar month (local time): label `YYYY-MM`, its name in `lang`, and
/// its start and end as unix seconds.
pub fn last_month(lang: &str) -> (String, String, i64, i64) {
    use chrono::{Datelike, Local, TimeZone};
    let now = Local::now();
    let (y, m) = if now.month() == 1 {
        (now.year() - 1, 12)
    } else {
        (now.year(), now.month() - 1)
    };
    let start = |y: i32, m: u32| {
        Local
            .with_ymd_and_hms(y, m, 1, 0, 0, 0)
            .earliest()
            .map_or(0, |d| d.timestamp())
    };
    let name = month_name(lang, m as usize - 1);
    (
        format!("{y}-{m:02}"),
        format!("{name} {y}"),
        start(y, m),
        start(now.year(), now.month()),
    )
}

fn month_name(lang: &str, m: usize) -> &'static str {
    const IT: [&str; 12] = [
        "Gennaio",
        "Febbraio",
        "Marzo",
        "Aprile",
        "Maggio",
        "Giugno",
        "Luglio",
        "Agosto",
        "Settembre",
        "Ottobre",
        "Novembre",
        "Dicembre",
    ];
    const EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    if lang == "it" {
        IT[m]
    } else {
        EN[m]
    }
}

/// One line about a month of mining from `get_stats` (`earnings`), or `None` when nothing
/// was mined. Amounts are the engine's estimates; the pool balance is the real figure.
pub fn month_summary(
    lang: &str,
    month: &str,
    earnings: &serde_json::Value,
    currency: &str,
) -> Option<String> {
    let e = earnings;
    if e["miningSecs"].as_f64().unwrap_or(0.0) <= 0.0 {
        return None;
    }
    let sym = if currency == "USD" { "$" } else { "€" };
    let money = |k: &str| e[k].as_f64().map(|v| format!("{v:.2} {sym}"));
    let sats = (e["netBtc"].as_f64().unwrap_or(0.0) * 1e8).round() as i64;
    let it = lang == "it";
    let mut s = if it {
        format!("{month}: minati {sats} sat")
    } else {
        format!("{month}: mined {sats} sat")
    };
    if let Some(v) = money("netValue") {
        s += &format!(" ({v})");
    }
    if let Some(c) = money("energyCost") {
        s += &if it {
            format!(", corrente {c}")
        } else {
            format!(", electricity {c}")
        };
    }
    if let Some(n) = money("netProfit") {
        s += &if it {
            format!(", netto {n}")
        } else {
            format!(", net {n}")
        };
    }
    Some(s)
}

/// Sends to every configured channel; the first error is returned.
pub fn send(n: &Notify, title: &str, body: &str) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let mut result = Ok(());
    let mut keep = |r: Result<ureq::Response, ureq::Error>| {
        if let (Err(e), Ok(())) = (r, &result) {
            result = Err(e.to_string());
        }
    };
    if !n.telegram_token.is_empty() && !n.telegram_chat.is_empty() {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            n.telegram_token
        );
        keep(agent.post(&url).send_json(serde_json::json!({
            "chat_id": n.telegram_chat,
            "text": format!("{title}\n{body}"),
        })));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_follow_the_language() {
        assert_eq!(
            text("it", "hashrate_dropped", Some("40% of normal")),
            "Hashrate crollato (40% of normal): riavvio il miner"
        );
        assert_eq!(
            text("en", "miner_api_unreachable", None),
            "The miner is not responding"
        );
        assert_eq!(
            text("en", "storage_error", Some("disk full")),
            "Data can't be saved on this computer"
        );
        assert_eq!(text("en", "new_code", Some("x")), "new_code: x");
        let e = serde_json::json!({"miningSecs": 3600.0, "netBtc": 0.00012345, "netValue": 7.5,
            "energyCost": 2.25, "netProfit": 5.25});
        assert_eq!(
            month_summary("it", "Ottobre 2026", &e, "EUR").unwrap(),
            "Ottobre 2026: minati 12345 sat (7.50 €), corrente 2.25 €, netto 5.25 €"
        );
        assert_eq!(
            month_summary("en", "May", &serde_json::json!({"miningSecs": 0}), "EUR"),
            None
        );
        let q = Notify {
            quiet: true,
            quiet_start: 23 * 60,
            quiet_end: 7 * 60,
            ..Default::default()
        };
        assert!(
            q.is_quiet(23 * 60)
                && q.is_quiet(3 * 60)
                && !q.is_quiet(7 * 60)
                && !q.is_quiet(12 * 60)
        );
        assert_eq!(
            device_text("it", "temperature_high", Some("92 °C")),
            "Temperatura alta (92 °C)"
        );
        assert_eq!(
            device_text("en", "device_offline", Some("connection refused")),
            "Went offline"
        );
    }
}
