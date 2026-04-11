use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use axum::{Json, extract::State, http::StatusCode};
use base::vecmap::VecMap;
use chrono::{DateTime, Datelike, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    WeekStart,
    service::{INTERNAL_SERVER_ERROR, tarifs_db},
};

use super::{AppState, JwtAuth};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Compteur {
    kwh: f32,
    cost: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsoPeriod {
    start: DateTime<Local>,
    end: DateTime<Local>,

    total_kwh: f32,
    total_cost: f32,
    compteurs: VecMap<Compteur>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoStats {
    data_start: DateTime<Local>,

    today: ConsoPeriod,
    yesterday: Option<ConsoPeriod>,

    this_week: ConsoPeriod,
    last_week: Option<ConsoPeriod>,

    this_month: ConsoPeriod,
    last_month: Option<ConsoPeriod>,

    this_year: ConsoPeriod,
    last_year: Option<ConsoPeriod>,
}

pub async fn get_conso_stats(
    State(state): State<Arc<AppState>>,
    _: JwtAuth,
) -> Result<Json<ConsoStats>, (StatusCode, &'static str)> {
    let week_start = match state.week_start {
        WeekStart::Monday => chrono::Weekday::Mon,
        WeekStart::Sunday => chrono::Weekday::Sun,
    };
    let mqtt_state = state.mqtt.lock().await;
    let Some(contrat) = mqtt_state.contrat.clone() else {
        return Err((StatusCode::BAD_REQUEST, "No contrat configured"));
    };

    let tarifs_db = tokio::spawn(async move {
        tarifs_db::TarifsDb::fetch_for_contrat(&contrat)
            .await
            .map_err(|_| {
                log::error!("Error fetching tarifs for contrat");
                INTERNAL_SERVER_ERROR
            })
    });

    let now = Local::now();
    let today_start = {
        let mut today_start = state.day_start.with_datetime(now.clone());
        if today_start > now {
            today_start = today_start - chrono::Duration::days(1);
        }
        today_start
    };
    log::debug!("Today starts at {}", today_start);
    let week_start = today_start
        - chrono::Duration::days(
            (7 + today_start.weekday().num_days_from_monday() as i64
                - week_start.num_days_from_monday() as i64)
                % 7,
        );
    let month_start = today_start.with_day(1).unwrap();
    let year_start = today_start.with_month(1).unwrap().with_day(1).unwrap();

    let yesterday_start = today_start - chrono::Duration::days(1);
    let last_week_start = week_start - chrono::Duration::days(7);
    let last_month = {
        let m = month_start.month();
        if m == 1 { 12 } else { m - 1 }
    };
    let last_month_start = month_start.with_month(last_month).unwrap();
    let last_year_start = year_start.with_year(year_start.year() - 1).unwrap();

    let oldest = get_compteurs_oldest_time(&state.influx)
        .await
        .map_err(|_| {
            log::error!("Error fetching compteurs oldest");
            INTERNAL_SERVER_ERROR
        })?;

    let time_span = (oldest, now);
    log::debug!("Time span of compteurs data: {} to {}", oldest, now);

    let tarifs_db = tarifs_db.await.map_err(|_| {
        log::error!("Error joining tarifs fetching task");
        INTERNAL_SERVER_ERROR
    })??;

    let today = get_conso_period(&state.influx, today_start, now, time_span, &tarifs_db)
        .await
        .map_err(|e| {
            log::error!("Error fetching today's conso period: {}", e);
            INTERNAL_SERVER_ERROR
        })?
        .ok_or((StatusCode::BAD_REQUEST, "No data for today"))?;
    let this_week = get_conso_period(&state.influx, week_start, now, time_span, &tarifs_db)
        .await
        .map_err(|e| {
            log::error!("Error fetching this week's conso period: {}", e);
            INTERNAL_SERVER_ERROR
        })?
        .ok_or((StatusCode::BAD_REQUEST, "No data for this week"))?;
    let this_month = get_conso_period(&state.influx, month_start, now, time_span, &tarifs_db)
        .await
        .map_err(|e| {
            log::error!("Error fetching this month's conso period: {}", e);
            INTERNAL_SERVER_ERROR
        })?
        .ok_or((StatusCode::BAD_REQUEST, "No data for this month"))?;
    let this_year = get_conso_period(&state.influx, year_start, now, time_span, &tarifs_db)
        .await
        .map_err(|e| {
            log::error!("Error fetching this year's conso period: {}", e);
            INTERNAL_SERVER_ERROR
        })?
        .ok_or((StatusCode::BAD_REQUEST, "No data for this year"))?;

    let yesterday = get_conso_period(
        &state.influx,
        yesterday_start,
        today_start,
        time_span,
        &tarifs_db,
    )
    .await
    .map_err(|e| {
        log::error!("Error fetching yesterday's conso period: {}", e);
        INTERNAL_SERVER_ERROR
    })?;
    let last_week = get_conso_period(
        &state.influx,
        last_week_start,
        week_start,
        time_span,
        &tarifs_db,
    )
    .await
    .map_err(|e| {
        log::error!("Error fetching last week's conso period: {}", e);
        INTERNAL_SERVER_ERROR
    })?;
    let last_month = get_conso_period(
        &state.influx,
        last_month_start,
        month_start,
        time_span,
        &tarifs_db,
    )
    .await
    .map_err(|e| {
        log::error!("Error fetching last month's conso period: {}", e);
        INTERNAL_SERVER_ERROR
    })?;
    let last_year = get_conso_period(
        &state.influx,
        last_year_start,
        year_start,
        time_span,
        &tarifs_db,
    )
    .await
    .map_err(|e| {
        log::error!("Error fetching last year's conso period: {}", e);
        INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ConsoStats {
        data_start: time_span.0,

        today,
        yesterday,

        this_week,
        last_week,

        this_month,
        last_month,

        this_year,
        last_year,
    }))
}

async fn get_conso_period(
    client: &influx::Client,
    start: chrono::DateTime<Local>,
    end: chrono::DateTime<Local>,
    time_span: (chrono::DateTime<Local>, chrono::DateTime<Local>),
    tarifs_db: &tarifs_db::TarifsDb,
) -> anyhow::Result<Option<ConsoPeriod>> {
    if start > time_span.1 || end < time_span.0 {
        // No data for this period
        log::debug!("No data for period from {} to {}", start, end);
        return Ok(None);
    }
    let start = start.max(time_span.0);
    let end = end.min(time_span.1);

    log::debug!("Getting compteurs data from {} to {}", start, end);

    let compteurs_start = get_compteurs_near(client, start)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No compteurs found near start time"))?;
    let compteurs_end = get_compteurs_near(client, end)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No compteurs found near end time"))?;

    let mut compteurs: VecMap<Compteur> = VecMap::new();
    for (key, start_wh) in compteurs_start.compteurs.iter() {
        let end_wh = compteurs_end
            .compteurs
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("Compteur '{}' not found in end compteurs", key))?;

        compteurs.push_no_check(
            key.to_string(),
            Compteur {
                kwh: (*end_wh - *start_wh) as f32 / 1000.0,
                cost: 0.0, // Will be filled later
            },
        );
    }

    let price_periods = tarifs_db.get_price_periods_for_time_span(start, end);
    if price_periods.is_empty() {
        // No tarifs for this period, can't compute cost
        return Err(anyhow::anyhow!("No tarifs found for the given time span"));
    }
    let mut compteur_map: HashMap<DateTime<Local>, mqtt::topics::Compteurs> =
        HashMap::from([(start, compteurs_start), (end, compteurs_end)]);

    for period in price_periods {
        let start = start.max(period.start);
        let end = end.min(period.end);
        log::debug!("Processing price period from {} to {}", start, end);
        let compteur_start: mqtt::topics::Compteurs = match compteur_map.entry(start) {
            Entry::Occupied(e) => e.get().clone(),
            Entry::Vacant(e) => {
                let compteurs = get_compteurs_near(client, start)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No compteurs found near time {}", start))?;
                e.insert(compteurs).clone()
            }
        };
        let compteur_end: &mqtt::topics::Compteurs = match compteur_map.entry(end) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let compteurs = get_compteurs_near(client, end)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No compteurs found near time {}", end))?;
                e.insert(compteurs)
            }
        };
        for (key, compteur) in compteurs.iter_mut() {
            let start_wh = compteur_start.compteurs.get(key).ok_or_else(|| {
                anyhow::anyhow!("Compteur '{}' not found in start compteurs", key)
            })?;
            let end_wh = compteur_end
                .compteurs
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("Compteur '{}' not found in end compteurs", key))?;
            let kwh = (*end_wh - *start_wh) as f32 / 1000.0;
            let price = period.price.get(key).cloned().unwrap_or(0.0);
            log::debug!("Compteur '{}': {} kWh at price {} €/kWh", key, kwh, price,);
            compteur.cost += kwh * price;
        }
    }

    let total_kwh: f32 = compteurs.iter().map(|(_, c)| c.kwh).sum();
    let total_cost: f32 = compteurs.iter().map(|(_, c)| c.cost).sum();

    Ok(Some(ConsoPeriod {
        start: start.into(),
        end: end.into(),
        total_kwh,
        total_cost,
        compteurs,
    }))
}

async fn get_compteurs_oldest_time(
    client: &influx::Client,
) -> anyhow::Result<chrono::DateTime<Local>> {
    let sql =
        "SELECT MIN(min_time) as fst_ns FROM system.parquet_files WHERE table_name='compteurs'";
    let json = match client.fetch_json(sql).await {
        Ok(json) => json,
        Err(e) => {
            log::error!("Error fetching compteurs time span: {}", e);
            return Err(anyhow::anyhow!("Error fetching compteurs time span"));
        }
    };
    #[derive(Debug, Deserialize)]
    struct ParquetEntry {
        fst_ns: i64,
    }
    let entries: Vec<ParquetEntry> = serde_json::from_slice(&json)?;
    let Some(entry) = entries.into_iter().next() else {
        return Err(anyhow::anyhow!(
            "No parquet entries found for table 'compteurs'"
        ));
    };

    let oldest = chrono::DateTime::<Utc>::from_timestamp_nanos(entry.fst_ns).with_timezone(&Local);

    Ok(oldest)
}

async fn get_compteurs_near(
    client: &influx::Client,
    timestamp: chrono::DateTime<Local>,
) -> anyhow::Result<Option<mqtt::topics::Compteurs>> {
    #[derive(Debug, Clone, Deserialize)]
    struct Entry {
        #[serde(flatten)]
        compteurs: mqtt::topics::Compteurs,
        #[allow(dead_code)]
        time: String,
        time_utc: DateTime<Utc>,
    }
    let timestamp = timestamp.with_timezone(&Utc);
    let start = timestamp - chrono::Duration::minutes(5);
    let end = timestamp + chrono::Duration::minutes(5);
    let sql = format!(
        "SELECT *, time AT TIME ZONE 'UTC' as time_utc FROM compteurs WHERE time >= '{}' AND time <= '{}' ORDER BY time ASC",
        start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    );

    log::debug!("Querying compteurs near {}: {}", timestamp, sql);

    let json = client.fetch_json(&sql).await?;
    let compteurs_entries: Vec<Entry> = serde_json::from_slice(&json)?;

    let mut dur = chrono::Duration::MAX;
    let mut compteurs = None;
    let mut time = None;
    for entry in compteurs_entries {
        let d = (entry.time_utc - timestamp).num_seconds().abs();
        if d < dur.num_seconds() {
            dur = chrono::Duration::seconds(d);
            time = Some(entry.time_utc);
            compteurs = Some(entry);
        }
    }
    log::debug!(
        "Closest compteurs to {} is at {} ({} seconds away)",
        timestamp,
        time.unwrap_or_else(|| timestamp),
        dur.num_seconds()
    );

    Ok(compteurs.map(|c| c.compteurs))
}
