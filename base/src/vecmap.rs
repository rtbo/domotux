//! A simple container for small maps that need to be serialized as vectors of (String, T) pairs
//! It is implemented as a Vec<(String, T)> and provides custom serialization and deserialization functions for serde.
//! Search is linear, so it is not suitable for large maps, but will be efficient for small maps (less than 10-15 entries)

#[derive(Debug, Clone)]
pub struct VecMap<T>(Vec<(String, T)>);

impl<T> From<Vec<(String, T)>> for VecMap<T> {
    fn from(vec: Vec<(String, T)>) -> Self {
        Self(vec)
    }
}

impl<T> VecMap<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn insert(&mut self, key: String, value: T) {
        for (k, v) in &mut self.0 {
            if *k == key {
                *v = value;
                return;
            }
        }
        self.0.push((key, value));
    }

    /// Insert a key-value pair without checking for duplicates.
    /// It is not unsafe but might lead to duplicate keys, which will hide the latter value
    pub fn push_no_check(&mut self, key: String, value: T) {
        self.0.push((key, value));
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        for (k, v) in &self.0 {
            if k == key {
                return Some(v);
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, T)> {
        self.0.iter()
    }
}


impl<T> serde::ser::Serialize for VecMap<T>
where
    T: serde::ser::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        crate::cfg::serialize_vec_to_map(&self.0, serializer)
    }
}

impl<'de, T> serde::de::Deserialize<'de> for VecMap<T>
where
    T: serde::de::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let vec = crate::cfg::deserialize_vec_from_map(deserializer)?;
        Ok(Self(vec))
    }
}
