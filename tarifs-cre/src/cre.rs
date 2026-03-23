//! Tarifs CRE
//! See https://www.data.gouv.fr/datasets/historique-des-tarifs-reglementes-de-vente-delectricite-pour-les-consommateurs-residentiels

use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use serde::{Deserialize, Deserializer};

use crate::{OptionPrice, tabular};

const BASE_ID: &str = "c13d05e5-9e55-4d03-bf7e-042a2ade7e49";
const HPHC_ID: &str = "f7303b3a-93c7-4242-813d-84919034c416";
const TEMPO_ID: &str = "0c3d1d36-c412-4620-8566-e5cbb4fa2b5a";

#[derive(Debug, Clone)]
pub struct Db {
    base: Vec<BaseRow>,
    hphc: Vec<HphcRow>,
    tempo: Vec<TempoRow>,
}

impl Db {
    pub async fn load() -> anyhow::Result<Self> {
        let base_resource = tabular::Resource::<BaseRow>::new(BASE_ID);
        let hphc_resource = tabular::Resource::<HphcRow>::new(HPHC_ID);
        let tempo_resource = tabular::Resource::<TempoRow>::new(TEMPO_ID);

        let (base, hphc, tempo) = tokio::try_join!(
            base_resource.fetch_all(),
            hphc_resource.fetch_all(),
            tempo_resource.fetch_all(),
        )?;

        Ok(Self { base, hphc, tempo })
    }

    pub fn search(&self, option: &str, psousc: u32, date: DateTime<FixedOffset>) -> anyhow::Result<Option<OptionPrice>> {
        match option {
            "base" => {
                let row = self.search_in_rows(&self.base, psousc, date);
                Ok(row.map(|r| OptionPrice::Base { base: r.price }))
            }
            "hphc" => {
                let row = self.search_in_rows(&self.hphc, psousc, date);
                Ok(row.map(|r| OptionPrice::Hphc { hp: r.price_hp, hc: r.price_hc }))
            }
            "tempo" => {
                let row = self.search_in_rows(&self.tempo, psousc, date);
                Ok(row.map(|r| OptionPrice::Tempo {
                    hp_bleu: r.price_hp_bleu.unwrap_or(0.0),
                    hc_bleu: r.price_hc_bleu.unwrap_or(0.0),
                    hp_blanc: r.price_hp_blanc.unwrap_or(0.0),
                    hc_blanc: r.price_hc_blanc.unwrap_or(0.0),
                    hp_rouge: r.price_hp_rouge.unwrap_or(0.0),
                    hc_rouge: r.price_hc_rouge.unwrap_or(0.0),
                }))
            }
            _ => Err(anyhow::anyhow!("Unknown option")),
        }
    }

    fn search_in_rows<R: Row>(&self, rows: &[R], psousc: u32, date: DateTime<FixedOffset>) -> Option<R> {
        rows.iter().find_map(|row| {
            if row.subsc_power()? == psousc
                && row.date_start()? <= date
                && row.date_end().unwrap_or_else(|| DateTime::<FixedOffset>::MAX_UTC.fixed_offset()) > date
            {
                Some(row.clone())
            } else {
                None
            }
        })
    }
}

trait Row: Clone {
    fn date_start(&self) -> Option<DateTime<FixedOffset>>;
    fn date_end(&self) -> Option<DateTime<FixedOffset>>;
    fn subsc_power(&self) -> Option<u32>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaseRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    #[serde(deserialize_with = "deserialize_paris_date")]
    date_start: DateTime<FixedOffset>,
    #[serde(rename(deserialize = "DATE_FIN"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_end: Option<DateTime<FixedOffset>>,
    #[serde(rename(deserialize = "P_SOUSCRITE"))]
    subsc_power: u32,
    #[serde(rename(deserialize = "PART_VARIABLE_TTC"))]
    price: f32,
}

impl Row for BaseRow {
    fn date_start(&self) -> Option<DateTime<FixedOffset>> {
        Some(self.date_start)
    }

    fn date_end(&self) -> Option<DateTime<FixedOffset>> {
        self.date_end
    }

    fn subsc_power(&self) -> Option<u32> {
        Some(self.subsc_power)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HphcRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    #[serde(deserialize_with = "deserialize_paris_date")]
    date_start: DateTime<FixedOffset>,
    #[serde(rename(deserialize = "DATE_FIN"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_end: Option<DateTime<FixedOffset>>,
    #[serde(rename(deserialize = "P_SOUSCRITE"))]
    subsc_power: u32,
    #[serde(rename(deserialize = "PART_VARIABLE_HP_TTC"))]
    price_hp: f32,
    #[serde(rename(deserialize = "PART_VARIABLE_HC_TTC"))]
    price_hc: f32,
}

impl Row for HphcRow {
    fn date_start(&self) -> Option<DateTime<FixedOffset>> {
        Some(self.date_start)
    }

    fn date_end(&self) -> Option<DateTime<FixedOffset>> {
        self.date_end
    }

    fn subsc_power(&self) -> Option<u32> {
        Some(self.subsc_power)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TempoRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_start: Option<DateTime<FixedOffset>>,
    #[serde(rename(deserialize = "DATE_FIN"))]
    #[serde(deserialize_with = "deserialize_paris_date_opt")]
    date_end: Option<DateTime<FixedOffset>>,
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
    fn date_start(&self) -> Option<DateTime<FixedOffset>> {
        self.date_start
    }

    fn date_end(&self) -> Option<DateTime<FixedOffset>> {
        self.date_end
    }

    fn subsc_power(&self) -> Option<u32> {
        self.subsc_power
    }
}

fn parse_paris_date_str(s: &str) -> Result<DateTime<FixedOffset>, String> {
    let naive_date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| e.to_string())?;

    let paris_offset = FixedOffset::east_opt(3600)
        .ok_or_else(|| "Invalid offset for Paris timezone".to_string())?;

    paris_offset
        .from_local_datetime(&naive_date.and_hms_opt(0, 0, 0).ok_or("Invalid time")?)
        .single()
        .ok_or_else(|| "Ambiguous or invalid local datetime".to_string())
}

fn deserialize_paris_date<'de, D>(deserializer: D) -> Result<DateTime<FixedOffset>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_paris_date_str(&s).map_err(serde::de::Error::custom)
}

fn deserialize_paris_date_opt<'de, D>(
    deserializer: D,
) -> Result<Option<DateTime<FixedOffset>>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    s.map(|v| parse_paris_date_str(&v).map_err(serde::de::Error::custom))
        .transpose()
}
