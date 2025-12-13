use anyhow::{anyhow, Result};
use serde::{Deserialize, Deserializer, Serializer};
use std::time::Duration;

// 重新导出serde_with
pub use serde_with::{serde_as, DeserializeAs, SerializeAs};

/// Duration的人性化格式化器
///
/// 支持格式: "3s", "100ms", "2m", "1h", "1h30m45s", "2d"
pub struct HumanDur;

impl SerializeAs<Duration> for HumanDur {
    fn serialize_as<S>(source: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format_duration(*source))
    }
}

impl<'de> DeserializeAs<'de, Duration> for HumanDur {
    fn deserialize_as<D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_duration(&s).map_err(serde::de::Error::custom)
    }
}

/// 解析时间字符串: "1h30m45s" -> Duration
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();
    let mut total = Duration::new(0, 0);
    let mut i = 0;
    let chars: Vec<char> = s.chars().collect();

    while i < chars.len() {
        // 解析数字
        let mut num_str = String::new();
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            num_str.push(chars[i]);
            i += 1;
        }

        if num_str.is_empty() {
            return Err(anyhow!("期望数字"));
        }

        let value: f64 = num_str
            .parse()
            .map_err(|_| anyhow!("无效数字: {}", num_str))?;

        // 解析单位
        let mut unit_str = String::new();
        while i < chars.len() && chars[i].is_ascii_alphabetic() {
            unit_str.push(chars[i]);
            i += 1;
        }

        if unit_str.is_empty() {
            return Err(anyhow!("缺少时间单位"));
        }

        let duration = match unit_str.as_str() {
            "ns" => Duration::from_nanos(value as u64),
            "us" | "μs" => Duration::from_nanos((value * 1000.0) as u64),
            "ms" => Duration::from_nanos((value * 1_000_000.0) as u64),
            "s" => Duration::from_secs_f64(value),
            "m" => Duration::from_secs((value * 60.0) as u64),
            "h" => Duration::from_secs((value * 3600.0) as u64),
            "d" => Duration::from_secs((value * 86400.0) as u64),
            _ => return Err(anyhow!("不支持的时间单位: {}", unit_str)),
        };

        total += duration;
    }

    Ok(total)
}

/// Duration格式化为字符串: Duration -> "1h30m45s"
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let nanos = duration.subsec_nanos();

    if total_secs == 0 {
        if nanos == 0 {
            return "0s".to_string();
        } else if nanos % 1_000_000 == 0 {
            return format!("{}ms", nanos / 1_000_000);
        } else if nanos % 1_000 == 0 {
            return format!("{}us", nanos / 1_000);
        } else {
            return format!("{}ns", nanos);
        }
    }

    let mut parts = Vec::new();
    let mut remaining = total_secs;

    if remaining >= 86400 {
        parts.push(format!("{}d", remaining / 86400));
        remaining %= 86400;
    }

    if remaining >= 3600 {
        parts.push(format!("{}h", remaining / 3600));
        remaining %= 3600;
    }

    if remaining >= 60 {
        parts.push(format!("{}m", remaining / 60));
        remaining %= 60;
    }

    if remaining > 0 || nanos > 0 {
        if nanos == 0 {
            parts.push(format!("{}s", remaining));
        } else {
            let total_ms = remaining * 1000 + (nanos as u64) / 1_000_000;
            parts.push(format!("{}ms", total_ms));
        }
    }

    parts.join("")
}
