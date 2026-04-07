//! Module that provides utilities to handle time in the day,
//! eg. to represent events that occur every day at a given time

use std::{fmt, str};

use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};

/// The time of the day, represented in milliseconds since midnight
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DayTime(u32);

impl DayTime {
    pub fn new(hours: u32, minutes: u32, seconds: u32) -> Option<Self> {
        if hours >= 24 || minutes >= 60 || seconds >= 60 {
            return None;
        }
        Some(DayTime((hours * 3600 + minutes * 60 + seconds) * 1000))
    }

    pub fn hours(&self) -> u32 {
        self.0 / 3600000
    }

    pub fn minutes(&self) -> u32 {
        (self.0 % 3600000) / 60000
    }

    pub fn seconds(&self) -> u32 {
        (self.0 % 60000) / 1000
    }

    pub fn duration_until(&self) -> std::time::Duration {
        let now = DayTime::from(std::time::SystemTime::now());
        let milliseconds_until = if self.0 >= now.0 {
            self.0 - now.0
        } else {
            24 * 3600 * 1000 - (now.0 - self.0)
        };
        std::time::Duration::from_millis(milliseconds_until as u64)
    }
}

impl str::FromStr for DayTime {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 {
            return Err(anyhow::anyhow!("Invalid daytime format: {}", s));
        }

        let hours: u32 = parts[0]
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid hours: {}", parts[0]))?;
        let minutes: u32 = parts[1]
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid minutes: {}", parts[1]))?;
        let seconds = parts.get(2).map_or(Ok(0), |s| {
            s.parse()
                .map_err(|_| anyhow::anyhow!("Invalid seconds: {}", s))
        })?;

        Ok(DayTime::new(hours, minutes, seconds)
            .ok_or_else(|| anyhow::anyhow!("Invalid time: {}:{}:{}", hours, minutes, seconds))?)
    }
}

impl fmt::Display for DayTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hours = self.hours();
        let minutes = self.minutes();
        let seconds = self.seconds();
        write!(f, "{:02}:{:02}:{:02}", hours, minutes, seconds)
    }
}

impl From<std::time::SystemTime> for DayTime {
    fn from(time: std::time::SystemTime) -> Self {
        let datetime = chrono::DateTime::<Local>::from(time);
        let milliseconds_since_midnight = datetime.num_seconds_from_midnight() * 1000
            + datetime.nanosecond() / 1_000_000;
        DayTime(milliseconds_since_midnight)
    }
}

impl Serialize for DayTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DayTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};

    use super::DayTime;

    #[test]
    fn converts_system_time_using_local_timezone() {
        let datetime = Local
            .with_ymd_and_hms(2026, 1, 15, 12, 34, 56)
            .single()
            .unwrap();

        let daytime = DayTime::from(std::time::SystemTime::from(datetime));

        assert_eq!(daytime, DayTime::new(12, 34, 56).unwrap());
    }
}
