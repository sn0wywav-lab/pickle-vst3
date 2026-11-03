use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Discord's epoch (2015-01-01T00:00:00Z) in unix milliseconds.
pub const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// A Discord snowflake id. Stored as a `u64` (8 bytes) instead of the
/// `String` that the API sends, which keeps maps and messages compact.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Snowflake(pub u64);

impl Snowflake {
    pub const NONE: Snowflake = Snowflake(0);

    /// Unix timestamp in milliseconds encoded in the id.
    pub fn timestamp_ms(self) -> u64 {
        (self.0 >> 22) + DISCORD_EPOCH_MS
    }

    /// Generate a snowflake-shaped nonce for "now" (used for optimistic sends).
    pub fn generate_nonce() -> Snowflake {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(DISCORD_EPOCH_MS);
        let rnd: u64 = rand::random::<u64>() & 0x3F_FFFF;
        Snowflake(((now.saturating_sub(DISCORD_EPOCH_MS)) << 22) | rnd)
    }

    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl FromStr for Snowflake {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.trim().parse().map(Snowflake)
    }
}

impl From<u64> for Snowflake {
    fn from(v: u64) -> Self {
        Snowflake(v)
    }
}

impl Serialize for Snowflake {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Snowflake {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Snowflake;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a snowflake as string or integer")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Snowflake, E> {
                Ok(Snowflake(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Snowflake, E> {
                Ok(Snowflake(v as u64))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Snowflake, E> {
                v.parse().map(Snowflake).map_err(E::custom)
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_and_number() {
        let a: Snowflake = serde_json::from_str("\"175928847299117063\"").unwrap();
        let b: Snowflake = serde_json::from_str("175928847299117063").unwrap();
        assert_eq!(a, b);
        assert_eq!(serde_json::to_string(&a).unwrap(), "\"175928847299117063\"");
        // Example from the Discord docs.
        assert_eq!(a.timestamp_ms(), 1_462_015_105_796);
    }

    #[test]
    fn nonce_is_recent() {
        let n = Snowflake::generate_nonce();
        assert!(n.timestamp_ms() > 1_700_000_000_000);
    }
}
