//! Tarifs CRE
//! See https://www.data.gouv.fr/datasets/historique-des-tarifs-reglementes-de-vente-delectricite-pour-les-consommateurs-residentiels

use base::vecmap::VecMap;
use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone};
use mqtt::topics::{Contrat, PrixKwh};
use serde::Deserialize;
use serde::de::DeserializeOwned;

mod tabular;

#[derive(Debug, Clone)]
pub struct PricePeriod {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub price: VecMap<f32>,
}

const BASE_ID: &str = "c13d05e5-9e55-4d03-bf7e-042a2ade7e49";
const HPHC_ID: &str = "f7303b3a-93c7-4242-813d-84919034c416";
const TEMPO_ID: &str = "0c3d1d36-c412-4620-8566-e5cbb4fa2b5a";

pub async fn fetch_kwh_price(
    contrat: &Contrat,
    date: Option<NaiveDate>,
) -> anyhow::Result<Option<PrixKwh>> {
    let date = date.unwrap_or_else(|| chrono::Local::now().date_naive());
    match contrat.option.as_deref() {
        Some("base") => fetch_kwh_price_for::<BaseRow>(BASE_ID, contrat.subsc_power, date).await,
        Some("hphc") => fetch_kwh_price_for::<HphcRow>(HPHC_ID, contrat.subsc_power, date).await,
        Some("tempo") => fetch_kwh_price_for::<TempoRow>(TEMPO_ID, contrat.subsc_power, date).await,
        _ => anyhow::bail!("Unknown contract option"),
    }
}

async fn fetch_kwh_price_for<R>(
    id: &str,
    subsc_power: Option<u32>,
    date: NaiveDate,
) -> anyhow::Result<Option<PrixKwh>>
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
            return Ok(Some(PrixKwh(row.to_vecmap())));
        }
    }
    Ok(None)
}

pub async fn fetch_price_periods(contrat: &Contrat) -> anyhow::Result<Vec<PricePeriod>> {
    match contrat.option.as_deref() {
        Some("base") => fetch_price_periods_for::<BaseRow>(BASE_ID, contrat).await,
        Some("hphc") => fetch_price_periods_for::<HphcRow>(HPHC_ID, contrat).await,
        Some("tempo") => fetch_price_periods_for::<TempoRow>(TEMPO_ID, contrat).await,
        _ => anyhow::bail!("Unknown contract option"),
    }
}

async fn fetch_price_periods_for<R>(id: &str, contrat: &Contrat) -> anyhow::Result<Vec<PricePeriod>>
where
    R: Row + DeserializeOwned,
{
    let rows = tabular::Resource::<R>::new(id).fetch_all().await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| match (contrat.subsc_power, row.subsc_power()) {
            (Some(sp), Some(rsp)) if sp != rsp => None,
            (Some(_), None) => None,
            _ => Some(row.to_price_period()),
        })
        .collect())
}

trait Row: Clone {
    fn date_start(&self) -> Option<NaiveDate>;
    fn date_end(&self) -> Option<NaiveDate>;
    fn subsc_power(&self) -> Option<u32>;
    fn to_vecmap(&self) -> VecMap<f32>;

    fn to_price_period(&self) -> PricePeriod {
        let time = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let very_old_date = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap();
        let future_date = NaiveDate::from_ymd_opt(2500, 1, 1).unwrap();

        let date_start = self.date_start().unwrap_or(very_old_date);
        let date_end = self.date_end().unwrap_or(future_date);

        PricePeriod {
            start: paris_date_to_local(date_start, time),
            end: paris_date_to_local(date_end, time) + chrono::Duration::days(1),
            price: self.to_vecmap(),
        }
    }
}

fn paris_date_to_local(date: NaiveDate, time: NaiveTime) -> DateTime<Local> {
    let local_dt = date.and_time(time);
    chrono_tz::Europe::Paris
        .from_local_datetime(&local_dt)
        .single()
        .unwrap()
        .with_timezone(&Local)
}

#[derive(Debug, Clone, Deserialize)]
struct BaseRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    date_start: String,
    #[serde(rename(deserialize = "DATE_FIN"))]
    date_end: Option<String>,
    #[serde(rename(deserialize = "P_SOUSCRITE"))]
    subsc_power: u32,
    #[serde(rename(deserialize = "PART_VARIABLE_TTC"))]
    price: f32,
}

impl Row for BaseRow {
    fn date_start(&self) -> Option<NaiveDate> {
        if self.date_start.starts_with("2012") {
            NaiveDate::parse_from_str(&self.date_start, "%Y-%m-%d").ok()
        } else {
            NaiveDate::parse_from_str(&self.date_start, "%Y-%d-%m").ok()
        }
    }

    fn date_end(&self) -> Option<NaiveDate> {
        self.date_end
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
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
struct HphcRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    date_start: String,
    #[serde(rename(deserialize = "DATE_FIN"))]
    date_end: Option<String>,
    #[serde(rename(deserialize = "P_SOUSCRITE"))]
    subsc_power: u32,
    #[serde(rename(deserialize = "PART_VARIABLE_HP_TTC"))]
    price_hp: f32,
    #[serde(rename(deserialize = "PART_VARIABLE_HC_TTC"))]
    price_hc: f32,
}

impl Row for HphcRow {
    fn date_start(&self) -> Option<NaiveDate> {
        if self.date_start.starts_with("2012") {
            NaiveDate::parse_from_str(&self.date_start, "%Y-%m-%d").ok()
        } else {
            NaiveDate::parse_from_str(&self.date_start, "%Y-%d-%m").ok()
        }
    }

    fn date_end(&self) -> Option<NaiveDate> {
        self.date_end
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
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
struct TempoRow {
    #[serde(rename(deserialize = "DATE_DEBUT"))]
    date_start: Option<String>,
    #[serde(rename(deserialize = "DATE_FIN"))]
    date_end: Option<String>,
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
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%d-%m").ok())
    }

    fn date_end(&self) -> Option<NaiveDate> {
        self.date_end
            .as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
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