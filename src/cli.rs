//! Command-line interface for TheWatcher.

use clap::Parser;
use std::path::PathBuf;

use crate::config::Config;

#[derive(Parser, Debug)]
#[command(name = "thewatcher")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Self-hosted system metrics viewer", long_about = None)]
pub struct Cli {
    /// Bind address; default: 127.0.0.1
    #[arg(long, default_value = "127.0.0.1")]
    pub listen: String,

    /// HTTP port; default: 8080
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Granular collection interval (e.g., 5s, 30s, 1m, 5m)
    #[arg(long, default_value = "30s")]
    pub interval: String,

    /// Directory for MooFiles
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Retain granular samples (e.g., 30d)
    #[arg(long, default_value = "30d")]
    pub granular_retention: String,

    /// Retain hourly summaries (e.g., 365d)
    #[arg(long, default_value = "365d")]
    pub hourly_retention: String,

    /// Retain daily summaries (e.g., 5y)
    #[arg(long, default_value = "5y")]
    pub daily_retention: String,

    /// Retain monthly summaries (e.g., 10y)
    #[arg(long, default_value = "10y")]
    pub monthly_retention: String,

    /// Retain yearly summaries (e.g., 0 for indefinite)
    #[arg(long, default_value = "0")]
    pub yearly_retention: String,

    /// Log level: error, warn, info, debug, trace
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

impl Cli {
    pub fn into_config(self) -> Result<Config, String> {
        let interval_secs = parse_duration_secs(&self.interval)?;
        if interval_secs < 1 {
            return Err("Interval must be at least 1 second".into());
        }

        let granular_retention_days = parse_retention_days(&self.granular_retention)?;
        let hourly_retention_days = parse_retention_days(&self.hourly_retention)?;
        let daily_retention_days = parse_retention_days(&self.daily_retention)?;
        let monthly_retention_days = parse_retention_days(&self.monthly_retention)?;
        let yearly_retention_days = parse_retention_days(&self.yearly_retention)?;

        let data_dir = self.data_dir.unwrap_or_else(|| {
            crate::config::Config::default().data_dir
        });

        Ok(Config {
            listen: self.listen,
            port: self.port,
            interval_secs,
            data_dir,
            granular_retention_days,
            hourly_retention_days,
            daily_retention_days,
            monthly_retention_days,
            yearly_retention_days,
            log_level: self.log_level,
        })
    }
}

/// Parse a human-readable duration like "30s", "5m", "1h", "7d" into seconds.
fn parse_duration_secs(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Empty duration".into());
    }

    let (num_str, unit) = input.split_at(input.len() - 1);
    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("Invalid duration number: {}", num_str))?;

    match unit {
        "s" => Ok(num),
        "m" => Ok(num * 60),
        "h" => Ok(num * 3600),
        "d" => Ok(num * 86400),
        _ => Err(format!("Unknown duration unit: {}. Use s, m, h, or d", unit)),
    }
}

/// Parse a retention string like "30d", "5y", "0" into days.
/// "0" means indefinite (returned as 0).
fn parse_retention_days(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if input == "0" {
        return Ok(0);
    }
    if input.is_empty() {
        return Err("Empty retention".into());
    }

    let last_char = input.chars().last().unwrap();
    if last_char.is_ascii_digit() {
        // plain number of days
        return input.parse::<u64>().map_err(|_| format!("Invalid retention: {}", input));
    }

    let (num_str, unit) = input.split_at(input.len() - 1);
    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("Invalid retention number: {}", num_str))?;

    match unit {
        "d" => Ok(num),
        "w" => Ok(num * 7),
        "m" => Ok(num * 30),
        "y" => Ok(num * 365),
        _ => Err(format!(
            "Unknown retention unit: {}. Use d, w, m, or y",
            unit
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration_secs("30s").unwrap(), 30);
        assert_eq!(parse_duration_secs("5m").unwrap(), 300);
        assert_eq!(parse_duration_secs("1h").unwrap(), 3600);
        assert_eq!(parse_duration_secs("7d").unwrap(), 604800);
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration_secs("").is_err());
        assert!(parse_duration_secs("x").is_err());
        assert!(parse_duration_secs("0x").is_err());
    }

    #[test]
    fn test_parse_retention() {
        assert_eq!(parse_retention_days("30d").unwrap(), 30);
        assert_eq!(parse_retention_days("5y").unwrap(), 1825);
        assert_eq!(parse_retention_days("0").unwrap(), 0);
        assert_eq!(parse_retention_days("365").unwrap(), 365);
    }
}
