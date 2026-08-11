//! Configuration management for TheWatcher.

use std::path::PathBuf;

/// Application configuration, parsed from CLI arguments.
#[derive(Debug, Clone)]
pub struct Config {
    pub listen: String,
    pub port: u16,
    pub interval_secs: u64,
    pub data_dir: PathBuf,
    pub granular_retention_days: u64,
    pub hourly_retention_days: u64,
    pub daily_retention_days: u64,
    pub monthly_retention_days: u64,
    pub yearly_retention_days: u64,
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1".to_string(),
            port: 8080,
            interval_secs: 30,
            data_dir: default_data_dir(),
            granular_retention_days: 30,
            hourly_retention_days: 365,
            daily_retention_days: 1825,    // 5 years
            monthly_retention_days: 3650,  // 10 years
            yearly_retention_days: 0,      // indefinite
            log_level: "info".to_string(),
        }
    }
}

impl Config {
    pub fn listener_addr(&self) -> String {
        format!("{}:{}", self.listen, self.port)
    }

    pub fn is_loopback(&self) -> bool {
        self.listen == "127.0.0.1" || self.listen == "::1" || self.listen == "localhost"
    }

    pub fn is_all_interfaces(&self) -> bool {
        self.listen == "0.0.0.0" || self.listen == "::"
    }
}

fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".local/share/thewatcher")
    }
    #[cfg(target_os = "windows")]
    {
        let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\".to_string());
        PathBuf::from(localappdata).join("TheWatcher")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".local/share/thewatcher")
    }
}
