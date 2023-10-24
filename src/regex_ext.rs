use std::hash::Hash;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;


#[derive(Clone, Debug)]
pub struct SerializableRegex(pub Regex);
impl PartialEq for SerializableRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}
impl Eq for SerializableRegex {}
impl PartialOrd for SerializableRegex {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.as_str().partial_cmp(other.0.as_str())
    }
}
impl Ord for SerializableRegex {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl Hash for SerializableRegex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}
impl Serialize for SerializableRegex {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_str().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for SerializableRegex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let regex = Regex::new(&s)
            .map_err(|e| D::Error::custom(e))?;
        Ok(Self(regex))
    }
}
