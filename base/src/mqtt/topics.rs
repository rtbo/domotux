use serde::{Deserialize, Serialize};

use crate::vecmap::VecMap;

pub trait Topic {
    fn topic() -> String;
}

/// Apparent power of the installation, as seen by the meter. Should be <= subscribed power.
/// A negative value means that the installation produces power (ex: solar panels).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PApp(pub f32);

impl Topic for PApp {
    fn topic() -> String {
        "domotux/papp".to_string()
    }
}

/// List of electricity meters, with their summed consumption.
/// The active field is the one currently used by the meter, and should be one of the keys of the meters map.
/// For example, with the french "Tempo" option, the available meters are "bleuHp", "bleuHc", "blancHp", "blancHc", "rougeHp" and "rougeHc".
/// The active one is the one corresponding to the current day type (ex: "bleu" for a blue day).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compteurs {
    pub active: Option<String>,
    #[serde(flatten)]
    pub compteurs: VecMap<u32>,
}

impl Topic for Compteurs {
    fn topic() -> String {
        "domotux/compteurs".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompteurActif(pub String);

impl Topic for CompteurActif {
    fn topic() -> String {
        "domotux/compteurs/actif".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contrat {
    /// Subscribed power in KVA
    pub subsc_power: Option<u32>,
    /// Type of contract (in France: "base", "tempo", "hphc")
    pub option: Option<String>,
}

impl Topic for Contrat {
    fn topic() -> String {
        "domotux/contrat".to_string()
    }
}


/// The price per kWh for a selected option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrixKwh(pub VecMap<f32>);

impl Topic for PrixKwh {
    fn topic() -> String {
        "domotux/prix_kwh".to_string()
    }
}

/// The price per kWh for the currently active compteur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrixKwhActif(pub f32);

impl Topic for PrixKwhActif {
    fn topic() -> String {
        "domotux/prix_kwh/actif".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_power_serialization() {
        let papp = PApp(830.0);
        let payload = serde_json::to_string(&papp).unwrap();
        assert_eq!(payload, "830.0");
    }

    #[test]
    fn test_compteurs_serialization() {
        let mut meters = VecMap::new();
        meters.push_no_check("bleuHp".to_string(), 100);
        meters.push_no_check("bleuHc".to_string(), 200);
        let m = Compteurs {
            active: Some("bleu".to_string()),
            compteurs: meters,
        };
        let payload = serde_json::to_string(&m).unwrap();
        assert_eq!(payload, r#"{"active":"bleu","meters":{"bleuHp":100,"bleuHc":200}}"#);
    }

    #[test]
    fn test_contrat_serialization() {
        let c = Contrat {
            subsc_power: Some(6),
            option: Some("tempo".to_string()),
        };
        let payload = serde_json::to_string(&c).unwrap();
        assert_eq!(payload, r#"{"subsc_power":6,"option":"tempo"}"#);
    }

    #[test]
    fn test_prix_kwh_serialization() {
        let mut prices = VecMap::new();
        prices.push_no_check("bleuHc".to_string(), 0.15);
        prices.push_no_check("bleuHp".to_string(), 0.25);
        let kwh_price = PrixKwh(prices);
        let payload = serde_json::to_string(&kwh_price).unwrap();
        assert_eq!(payload, r#"{"bleuHc":0.15,"bleuHp":0.25}"#);
    }
}
