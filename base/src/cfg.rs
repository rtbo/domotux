use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::fs;

pub fn print_default_config<C>() -> Result<(), anyhow::Error>
where
    C: Default + Serialize,
{
    let config = C::default();
    let yaml = serde_yml::to_string(&config)?;
    println!("{}", yaml);
    Ok(())
}

pub async fn load_config<C>(
    service_name: &str,
    cfg_path: Option<PathBuf>,
) -> Result<C, anyhow::Error>
where
    C: for<'de> Deserialize<'de> + Default,
{
    let config_file = if let Some(p) = cfg_path {
        Some(p)
    } else {
        find_config_file(service_name)
    };

    if let Some(config_file) = config_file {
        let config_contents = fs::read_to_string(&config_file).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to read config file {}: {}",
                config_file.display(),
                e
            )
        })?;
        let config: C = serde_yml::from_str(&config_contents).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse config file {}: {}",
                config_file.display(),
                e
            )
        })?;
        Ok(config)
    } else {
        Ok(C::default())
    }
}

pub async fn save_config<C>(
    service_name: &str,
    config: &C,
    cfg_path: Option<PathBuf>,
) -> Result<(), anyhow::Error>
where
    C: Serialize,
{
    let config_file = if let Some(p) = cfg_path {
        Some(p)
    } else {
        dirs::config_local_dir().map(|dir| dir.join(format!("{}.yml", service_name)))
    };

    if let Some(config_file) = config_file {
        let yaml = serde_yml::to_string(config)?;
        fs::write(&config_file, yaml).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to write config file {}: {}",
                config_file.display(),
                e
            )
        })?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("No config file path specified"))
    }
}

fn find_config_file(service_name: &str) -> Option<PathBuf> {
    let filename = format!("{}.yml", service_name);
    if let Some(file) = check_config_path(dirs::config_local_dir().map(|dir| dir.join(&filename))) {
        return Some(file);
    }
    if let Some(file) = check_config_path(dirs::config_dir().map(|dir| dir.join(&filename))) {
        return Some(file);
    }
    if let Some(file) =
        check_config_path(dirs::home_dir().map(|dir| dir.join(format!(".{}", filename))))
    {
        return Some(file);
    }
    #[cfg(target_os = "linux")]
    if let Some(file) = check_config_path(Some(PathBuf::from("/etc").join(&filename))) {
        return Some(file);
    }
    None
}

fn check_config_path(path: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(p) = path {
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Custom serializer for Duration that serializes as seconds
pub fn serialize_seconds<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let secs = duration.as_secs();
    serializer.serialize_u64(secs)
}

/// Custom deserializer for Duration that deserializes from seconds
pub fn deserialize_seconds<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(secs))
}

/// Serialize a vector of (String, T) as a map
pub fn serialize_vec_to_map<S, T>(vec: &Vec<(String, T)>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: serde::ser::Serialize,
{
    use serde::ser::SerializeMap;

    let mut map = serializer.serialize_map(Some(vec.len()))?;
    for (k, v) in vec {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

/// Deserialize a map as a vector of (String, T)
pub fn deserialize_vec_from_map<'de, D, T>(deserializer: D) -> Result<Vec<(String, T)>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::Deserialize<'de>,
{
    use std::marker::PhantomData;

    use serde::de::{MapAccess, Visitor};

    struct FieldsVisitor<T>(PhantomData<T>);

    impl<'de, T> Visitor<'de> for FieldsVisitor<T>
    where
        T: serde::de::Deserialize<'de>,
    {
        type Value = Vec<(String, T)>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a map of field names to minimum periods")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut vec = Vec::with_capacity(map.size_hint().unwrap_or(0));
            while let Some((k, v)) = map.next_entry()? {
                vec.push((k, v));
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_map(FieldsVisitor::<T>(PhantomData))
}
