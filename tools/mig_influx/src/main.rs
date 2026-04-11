use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};

type DateTime = chrono::DateTime<chrono::Utc>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ColumnMig {
    #[serde(default)]
    new_name: Option<String>,
    #[serde(default)]
    str_values_map: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TableMig {
    cur_name: String,
    #[serde(default)]
    new_name: Option<String>,
    #[serde(default)]
    columns: HashMap<String, ColumnMig>,
    #[serde(default)]
    time_span: Option<u32>,
    #[serde(default)]
    est_start: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    host: String,
    #[serde(default)]
    token: Option<String>,
    database: String,
    tables: Vec<TableMig>,
}

#[tokio::main]
async fn main() {
    // let ex_cfg = Config {
    //     host: "http://domotux.lan:8181".to_string(),
    //     token: None,
    //     database: "domotux".to_string(),
    //     tables: vec![TableMig {
    //         cur_name: "compteurs".to_string(),
    //         new_name: "compteurs_new".to_string(),
    //         columns: HashMap::from([
    //             (
    //                 "active".to_string(),
    //                 ColumnMig {
    //                     new_name: Some("active".to_string()),
    //                     str_values_map: HashMap::from([
    //                         ("HCJB".to_string(), "blancHc".to_string()),
    //                         ("HPJB".to_string(), "blancHp".to_string()),
    //                     ]),
    //                 },
    //             ),
    //             (
    //                 "HCJB".to_string(),
    //                 ColumnMig {
    //                     new_name: Some("blancHc".to_string()),
    //                     str_values_map: HashMap::new(),
    //                 },
    //             ),
    //             (
    //                 "HPJB".to_string(),
    //                 ColumnMig {
    //                     new_name: Some("blancHp".to_string()),
    //                     str_values_map: HashMap::new(),
    //                 },
    //             ),
    //         ]),
    //         time_span: None,
    //         est_start: None,
    //     }],
    // };

    // println!(
    //     "{}",
    //     serde_yml::to_string(&ex_cfg).unwrap()
    // );
    // todo!();

    let cfg = fs::read_to_string("mig_influx.yml").await.unwrap();
    let config: Config = serde_yml::from_str(&cfg).unwrap();

    let client = reqwest::Client::new();

    // retrieve the oldest time
    for table in &config.tables {
        let filename = format!("{}_lines.txt", table.cur_name);
        migrate_table_to_file(&client, &config, table, &filename)
            .await
            .unwrap();
        write_file_to_influxdb(&client, &config, table, &filename)
            .await
            .unwrap();
    }
}

async fn migrate_table_to_file(
    client: &reqwest::Client,
    config: &Config,
    table: &TableMig,
    filename: &str,
) -> anyhow::Result<()> {
    let mut line_file = fs::File::create(filename).await?;

    let time_span = table.time_span.unwrap_or(24 * 3600);
    let mut start = find_oldest_time(&client, &config, table).await.unwrap();
    let mut end = start + chrono::Duration::seconds(time_span as i64);

    while start < chrono::Utc::now() {
        migrate_table_chunk(client, config, table, start, end, &mut line_file).await?;

        start = end;
        end = start + chrono::Duration::seconds(time_span as i64);
    }

    Ok(())
}

async fn write_file_to_influxdb(
    client: &reqwest::Client,
    config: &Config,
    table: &TableMig,
    filename: &str,
) -> anyhow::Result<()> {
    // read chunks of 5000 lines from the file and write to influxdb
    let file = fs::File::open(filename).await?;
    let mut reader = io::BufReader::new(file);
    let mut line_buf = String::new();
    let mut lines = 0;
    let mut tot = 0;

    while reader.read_line(&mut line_buf).await? > 0 {
        lines += 1;
        if lines >= 5000 {
            println!(
                "Writing {} lines to InfluxDB for table {}...",
                lines, table.cur_name
            );
            let mut new_buf = String::new();
            std::mem::swap(&mut line_buf, &mut new_buf);
            write_lines_to_influxdb(client, config, new_buf).await?;
            tot += lines;
            lines = 0;
        }
    }

    if lines > 0 {
        write_lines_to_influxdb(client, config, line_buf).await?;
        tot += lines;
    }

    println!("Total lines migrated for table {}: {}", table.cur_name, tot);

    Ok(())
}

async fn write_lines_to_influxdb(
    client: &reqwest::Client,
    config: &Config,
    lines: String,
) -> anyhow::Result<()> {
    let query = [("db", config.database.as_str()), ("precision", "s")];
    let mut req = client
        .post(&format!("{}/api/v3/write_lp", config.host))
        .query(&query);

    if let Some(token) = &config.token {
        req = req.bearer_auth(token);
    }

    let res = req
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Accept", "application/json")
        .body(lines)
        .send()
        .await?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        anyhow::bail!("InfluxDB write failed with status {}: {}", status, text);
    }

    Ok(())
}

async fn migrate_table_chunk(
    client: &reqwest::Client,
    config: &Config,
    table: &TableMig,
    start: DateTime,
    end: DateTime,
    line_file: &mut fs::File,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!("{}/api/v3/query_sql", config.host))
        .query(&[
            ("db", config.database.as_str()),
            (
                "q",
                format!(
                    "SELECT * FROM {} WHERE time >= '{}' AND time < '{}' ORDER BY time ASC",
                    table.cur_name,
                    start.to_rfc3339(),
                    end.to_rfc3339()
                )
                .as_str(),
            ),
        ])
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;
    let json = json
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Expected an array of results"))?;

    let table_name = table.new_name.as_ref().unwrap_or(&table.cur_name);

    for item in json {
        // Process each item and write to the new table
        // This is where you would apply any transformations based on the column mappings
        let obj = item
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Expected an object"))?;
        let mut line = format!("{} ", table_name);

        let mut time = None;

        for (col_name, value) in obj.iter() {
            if col_name == "time" {
                time = Some(parse_datetime(
                    value.as_str().expect("Expected a string for time field"),
                )?);
                continue;
            }
            let col_mig = table.columns.get(col_name);
            let new_col_name = col_mig
                .and_then(|col| col.new_name.as_ref())
                .unwrap_or(col_name);
            line.push_str(new_col_name);
            line.push('=');

            let mut done = false;
            if let Some(col_mig) = col_mig {
                if let Some(value_str) = value.as_str() {
                    line.push('"');
                    if let Some(mapped_value) = col_mig.str_values_map.get(value_str) {
                        line.push_str(mapped_value);
                    } else {
                        line.push_str(value_str);
                    }
                    line.push('"');
                    done = true;
                }
            }
            if !done {
                if let Some(val) = value.as_i64() {
                    line.push_str(val.to_string().as_str());
                    line.push('i');
                } else if let Some(val) = value.as_f64() {
                    line.push_str(val.to_string().as_str());
                } else if let Some(val) = value.as_bool() {
                    line.push_str(val.to_string().as_str());
                } else if value.is_null() {
                    line.push_str("null");
                } else {
                    line.push('"');
                    line.push_str(value.to_string().as_str());
                    line.push('"');
                }
            }
            line.push(',');
        }

        line.pop();
        line.push_str(format!(" {}", time.unwrap().timestamp()).as_str());

        line_file.write_all(line.as_bytes()).await?;
        line_file.write_all(b"\n").await?;
    }

    Ok(())
}

// InfluxDB doesn't allow to query the oldest time directly,
// so we need to check chunk by chunk until we find the oldest time.
async fn find_oldest_time(
    client: &reqwest::Client,
    config: &Config,
    table: &TableMig,
) -> anyhow::Result<DateTime> {
    let time_span = table.time_span.unwrap_or(24 * 3600).max(3600);
    let mut start = table
        .est_start
        .as_ref()
        .map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&chrono::Utc)))
        .transpose()?
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::seconds(time_span as i64));
    let mut end = start + chrono::Duration::seconds(time_span as i64);

    let mut span_time =
        get_oldest_time_of_span(client, config, &table.cur_name, start, end).await?;
    let mut oldest_time = span_time;

    while span_time.is_some() {
        oldest_time = span_time;
        end = start;
        start = end - chrono::Duration::seconds(time_span as i64);
        span_time = get_oldest_time_of_span(client, config, &table.cur_name, start, end).await?;
    }
    while oldest_time.is_none() {
        start = end;
        end = start + chrono::Duration::seconds(time_span as i64);
        if start > chrono::Utc::now() {
            anyhow::bail!("No data found for table {} after {}", table.cur_name, start);
        }
        oldest_time = get_oldest_time_of_span(client, config, &table.cur_name, start, end).await?;
    }

    Ok(oldest_time.unwrap())
}

async fn get_oldest_time_of_span(
    client: &reqwest::Client,
    config: &Config,
    table: &str,
    start: DateTime,
    end: DateTime,
) -> anyhow::Result<Option<DateTime>> {
    let sql = format!(
        "SELECT MIN(time) AS oldest_time FROM {} WHERE time >= '{}' AND time < '{}'",
        table,
        start.to_rfc3339(),
        end.to_rfc3339()
    );
    let resp = client
        .get(format!("{}/api/v3/query_sql", config.host))
        .query(&[("db", config.database.as_str()), ("q", sql.as_str())])
        .send()
        .await?;

    #[derive(Debug, Deserialize)]
    struct OldestTime {
        #[serde(default)]
        #[serde(deserialize_with = "deserialize_datetime")]
        oldest_time: Option<DateTime>,
    }

    let res: Vec<OldestTime> = resp.json().await?;

    Ok(res[0].oldest_time)
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<Option<DateTime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    if let Some(s) = s {
        Ok(Some(parse_datetime(&s).map_err(serde::de::Error::custom)?))
    } else {
        Ok(None)
    }
}

fn parse_datetime(s: &str) -> anyhow::Result<DateTime> {
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => Ok(dt.with_timezone(&chrono::Utc)),
        Err(_) => {
            let mut s = s.to_string();
            if !s.ends_with('Z') {
                s.push('Z');
            }
            match chrono::DateTime::parse_from_rfc3339(&s) {
                Ok(dt) => Ok(dt.with_timezone(&chrono::Utc)),
                Err(e) => Err(anyhow::anyhow!("Failed to parse datetime: {}", e)),
            }
        }
    }
}
