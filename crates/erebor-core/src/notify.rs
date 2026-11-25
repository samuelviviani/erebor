//! Push notifications for problems (warnings and errors only), sent to channels the user
//! set up: a Telegram bot. Outgoing only: nothing listens.

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
        "test" if it => "Notifiche attive ✓".into(),
        "test" => "Notifications are on ✓".into(),
        _ if d.is_empty() => code.into(),
        _ => format!("{code}: {d}"),
    };
    s
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
            "storage_error: disk full"
        );
    }
}
