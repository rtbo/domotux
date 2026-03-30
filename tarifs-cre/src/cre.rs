//! Tarifs CRE
//! See https://www.data.gouv.fr/datasets/historique-des-tarifs-reglementes-de-vente-delectricite-pour-les-consommateurs-residentiels

use base::{
    mqtt::topics::{Contract, KwhPrice},
    vecmap::VecMap,
};
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};

use crate::tabular;

const BASE_ID: &str = "c13d05e5-9e55-4d03-bf7e-042a2ade7e49";
const HPHC_ID: &str = "f7303b3a-93c7-4242-813d-84919034c416";
const TEMPO_ID: &str = "0c3d1d36-c412-4620-8566-e5cbb4fa2b5a";

pub async fn fetch_kwh_price(
    contract: &Contract,
    date: Option<NaiveDate>,
) -> anyhow::Result<Option<KwhPrice>> {
    let date = date.unwrap_or_else(|| chrono::Local::now().date_naive());
    match contract.option.as_deref() {
        Some("base") => fetch_kwh_price_for::<BaseRow>(BASE_ID, contract.subsc_power, date).await,
        Some("hphc") => fetch_kwh_price_for::<HphcRow>(HPHC_ID, contract.subsc_power, date).await,
        Some("tempo") => {
            fetch_kwh_price_for::<TempoRow>(TEMPO_ID, contract.subsc_power, date).await
        }
        _ => anyhow::bail!("Unknown contract option"),
    }
}

async fn fetch_kwh_price_for<R>(
    id: &str,
    subsc_power: Option<u32>,
    date: NaiveDate,
) -> anyhow::Result<Option<KwhPrice>>
where
    R: Row + DeserializeOwned,
{
    let rows = tabular::Resource::<R>::new(id).fetch_all().await?;
    for row in rows {
        match (subsc_power, row.subsc_power()) {
            (Some(sp), Some(rsp)) if sp != rsp => continue,
            (Some(_), None) => continue,
            _ => {}
        }
        let ds = row.date_start().unwrap_or(NaiveDate::MIN);
        let de = row.date_end().unwrap_or(NaiveDate::MAX);
        if ds <= date && de > date {
            return Ok(Some(KwhPrice(row.to_vecmap())));
        }
    }
    Ok(None)
}

trait Row: Clone {
    fn date_start(&self) -> Option<NaiveDate>;
    fn date_end(&self) -> Option<NaiveDate>;
    fn subsc_power(&self) -> Option<u32>;
    fn to_vecmap(&self) -> VecMap<f32>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaseRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    #[serde(deserialize_with = "deserialize_paris_date")]
    date_start: NaiveDate,
    #[serde(rename(deserialize = "DATE_FIN"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_end: Option<NaiveDate>,
    #[serde(rename(deserialize = "P_SOUSCRITE"))]
    subsc_power: u32,
    #[serde(rename(deserialize = "PART_VARIABLE_TTC"))]
    price: f32,
}

impl Row for BaseRow {
    fn date_start(&self) -> Option<NaiveDate> {
        Some(self.date_start)
    }

    fn date_end(&self) -> Option<NaiveDate> {
        self.date_end
    }

    fn subsc_power(&self) -> Option<u32> {
        Some(self.subsc_power)
    }

    fn to_vecmap(&self) -> VecMap<f32> {
        let mut map = VecMap::new();
        map.push_no_check("base".into(), self.price);
        map
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HphcRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    #[serde(deserialize_with = "deserialize_paris_date")]
    date_start: NaiveDate,
    #[serde(rename(deserialize = "DATE_FIN"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_end: Option<NaiveDate>,
    #[serde(rename(deserialize = "P_SOUSCRITE"))]
    subsc_power: u32,
    #[serde(rename(deserialize = "PART_VARIABLE_HP_TTC"))]
    price_hp: f32,
    #[serde(rename(deserialize = "PART_VARIABLE_HC_TTC"))]
    price_hc: f32,
}

impl Row for HphcRow {
    fn date_start(&self) -> Option<NaiveDate> {
        Some(self.date_start)
    }

    fn date_end(&self) -> Option<NaiveDate> {
        self.date_end
    }

    fn subsc_power(&self) -> Option<u32> {
        Some(self.subsc_power)
    }

    fn to_vecmap(&self) -> VecMap<f32> {
        let mut map = VecMap::new();
        map.push_no_check("hp".into(), self.price_hp);
        map.push_no_check("hc".into(), self.price_hc);
        map
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TempoRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_start: Option<NaiveDate>,
    #[serde(rename(deserialize = "DATE_FIN"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_end: Option<NaiveDate>,
    #[serde(rename(deserialize = "P_SOUSCRITE"))]
    subsc_power: Option<u32>,
    #[serde(rename(deserialize = "PART_VARIABLE_HPBleu_TTC"))]
    price_hp_bleu: Option<f32>,
    #[serde(rename(deserialize = "PART_VARIABLE_HCBleu_TTC"))]
    price_hc_bleu: Option<f32>,
    #[serde(rename(deserialize = "PART_VARIABLE_HPBlanc_TTC"))]
    price_hp_blanc: Option<f32>,
    #[serde(rename(deserialize = "PART_VARIABLE_HCBlanc_TTC"))]
    price_hc_blanc: Option<f32>,
    #[serde(rename(deserialize = "PART_VARIABLE_HPRouge_TTC"))]
    price_hp_rouge: Option<f32>,
    #[serde(rename(deserialize = "PART_VARIABLE_HCRouge_TTC"))]
    price_hc_rouge: Option<f32>,
}

impl Row for TempoRow {
    fn date_start(&self) -> Option<NaiveDate> {
        self.date_start
    }

    fn date_end(&self) -> Option<NaiveDate> {
        self.date_end
    }

    fn subsc_power(&self) -> Option<u32> {
        self.subsc_power
    }

    fn to_vecmap(&self) -> VecMap<f32> {
        let mut map = VecMap::new();
        if let Some(price) = self.price_hp_bleu {
            map.push_no_check("bleuHp".into(), price);
        }
        if let Some(price) = self.price_hc_bleu {
            map.push_no_check("bleuHc".into(), price);
        }
        if let Some(price) = self.price_hp_blanc {
            map.push_no_check("blancHp".into(), price);
        }
        if let Some(price) = self.price_hc_blanc {
            map.push_no_check("blancHc".into(), price);
        }
        if let Some(price) = self.price_hp_rouge {
            map.push_no_check("rougeHp".into(), price);
        }
        if let Some(price) = self.price_hc_rouge {
            map.push_no_check("rougeHc".into(), price);
        }
        map
    }
}

fn parse_paris_date_str(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| e.to_string())
}

fn deserialize_paris_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_paris_date_str(&s).map_err(serde::de::Error::custom)
}

fn deserialize_paris_date_opt<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    s.map(|v| parse_paris_date_str(&v).map_err(serde::de::Error::custom))
        .transpose()
}
