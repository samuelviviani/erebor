//! Settings changes from the frontends, miner selection, CSV export and data deletion.

use super::*;

impl Engine {
    /// Apply settings from the GUI. Protected fields (miner binary, benchmark) can only
    /// change through their dedicated commands.
    pub fn update_config(&mut self, mut new: AppConfig) -> Result<(), String> {
        new.miner = self.config.miner.clone();
        new.srbminer = self.config.srbminer.clone();
        new.lolminer = self.config.lolminer.clone();
        new.cpuminer = self.config.cpuminer.clone();
        new.defender_allowed = self.config.defender_allowed;
        new.benchmark = self.config.benchmark.clone();
        new.version = crate::config::CONFIG_VERSION;
        new.wallet = new.wallet.trim().to_string();
        // A new payout identifier brings its pool along: Kryptex IDs only work on
        // Kryptex. A coin picked by hand that the new payout can't mine falls back to the
        // automatic choice instead of blocking the save.
        if wallet::is_kryptex_id(&new.wallet) && !pools::is_kryptex_host(&new.pool.host) {
            new.pool = crate::config::PoolConfig {
                worker: new.pool.worker.clone(),
                ..Default::default()
            };
        }
        if new.coin != AUTO_COIN
            && new
                .validate()
                .is_err_and(|e| e == "kryptex_or_btc_required" || e == "not_on_unmineable")
        {
            new.coin = AUTO_COIN.into();
        }
        new.validate()?;
        let relaunch = self.is_active()
            && new.demo_mode == self.config.demo_mode
            && (new.wallet != self.config.wallet
                || new.coin != self.config.coin
                || new.pool != self.config.pool
                || new.performance != self.config.performance);
        if new.demo_mode != self.config.demo_mode {
            self.stop("mode_changed");
            self.persist_ledger();
            self.store = open_store(&self.data_dir, new.demo_mode)?;
            self.devfee = DevFeeManager::new(fee_percent(&new), false, load_ledger(&self.store));
        }
        self.config = new;
        self.devfee.set_enabled(fee_enabled(&self.config));
        self.devfee.set_percent(fee_percent(&self.config));
        self.save_config()?;
        if relaunch {
            self.stop("config_changed");
            self.start(false)?;
        }
        Ok(())
    }

    pub fn select_miner(&mut self, path: &str) -> Result<(), String> {
        self.select_miner_kind(MinerKind::Xmrig, path)
    }

    pub fn select_miner_kind(&mut self, kind: MinerKind, path: &str) -> Result<(), String> {
        if self.is_active() {
            return Err("stop_mining_first".into());
        }
        let p = xmrig::validate_binary_path(path, kind.dir_prefix())?;
        let version = match kind {
            MinerKind::Xmrig => Some(xmrig::detect_version(&p)?),
            _ => None,
        };
        let sha = xmrig::sha256_file(&p)?;
        let current = self.config.miner_for(kind);
        // Re-selecting the binary Erebor installed keeps it verified.
        let verified = current.sha256.as_deref() == Some(sha.as_str()) && current.verified;
        *self.config.miner_for_mut(kind) = crate::config::MinerConfig {
            path: Some(p.to_string_lossy().into_owned()),
            version: version.or_else(|| verified.then(|| current.version.clone()).flatten()),
            sha256: Some(sha),
            verified,
        };
        self.save_config()
    }

    /// Samples of the last `range_secs` as CSV.
    pub fn export_csv(&self, range_secs: i64) -> Result<String, String> {
        let to = now_unix() + 1;
        let from = to - range_secs.clamp(60, 366 * 86_400);
        self.store.export_csv(from, to).map_err(|e| e.to_string())
    }

    /// Same CSV written to the user's Downloads folder (data folder as fallback).
    pub fn save_csv(&self, range_secs: i64) -> Result<String, String> {
        let csv = self.export_csv(range_secs)?;
        let home =
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        let dir = home
            .map(|h| h.join("Downloads"))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| self.data_dir.clone());
        let name = format!(
            "erebor-{}.csv",
            chrono::Local::now().format("%Y-%m-%d-%H%M")
        );
        let path = dir.join(name);
        std::fs::write(&path, csv).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn export_config(&self) -> String {
        serde_json::to_string_pretty(&self.config).unwrap_or_default()
    }

    /// Stops mining and removes statistics, configuration and installed miners.
    pub fn delete_all_data(&mut self) -> Result<(), String> {
        self.stop("data_deleted");
        // Re-opening the production store closes the demo database if it was open.
        self.store = open_store(&self.data_dir, false)?;
        self.store.clear().map_err(|e| e.to_string())?;
        for f in [
            "erebor-demo.db",
            "erebor-demo.db-wal",
            "erebor-demo.db-shm",
            "config.json",
        ] {
            let _ = std::fs::remove_file(self.data_dir.join(f));
        }
        let _ = std::fs::remove_dir_all(self.data_dir.join("miners"));
        self.config = AppConfig::default();
        self.devfee = DevFeeManager::new(devfee::DEFAULT_FEE_PERCENT, false, Ledger::default());
        self.alerts.clear();
        Ok(())
    }
}
