use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub token: Option<String>,
    pub database: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "http://localhost:8181".to_string(),
            token: None,
            database: "domotux".to_string(),
        }
    }
}

pub trait AsLine {
    fn as_line(&self) -> String;
}

impl AsLine for &(mqtt::topics::PApp, std::time::SystemTime) {
    fn as_line(&self) -> String {
        format!(
            "papp value={} {}",
            self.0.0,
            self.1.duration_since(UNIX_EPOCH).unwrap().as_secs()
        )
    }
}

impl AsLine for mqtt::topics::Compteurs {
    fn as_line(&self) -> String {
        let mut line = String::new();
        if let Some(active) = &self.active {
            line.push_str(&format!("active=\"{}\"", active));
        }
        for (key, value) in self.compteurs.iter() {
            if !line.is_empty() {
                line.push(',');
            }
            line.push_str(&format!("{}={}u", key, value));
        }
        format!("compteurs {}", line)
    }
}

#[derive(Debug)]
pub struct Client {
    http: reqwest::Client,
    cfg: Config,
}

impl Client {
    pub fn new(cfg: Config) -> Self {
        Client {
            http: reqwest::Client::new(),
            cfg,
        }
    }

    pub async fn write_lines<L>(&self, lines: L) -> anyhow::Result<()>
    where
        L: IntoIterator,
        L::Item: AsLine,
    {
        let query = [("db", self.cfg.database.as_str()), ("precision", "s")];

        let mut req = self
            .http
            .post(&format!("{}/api/v3/write_lp", self.cfg.host))
            .query(&query);

        if let Some(token) = &self.cfg.token {
            req = req.bearer_auth(token);
        }

        let mut lines = lines.into_iter().map(|line| line.as_line());
        let mut body = String::new();
        body.push_str(
            &lines
                .next()
                .ok_or_else(|| anyhow::anyhow!("No lines to write"))?,
        );
        for line in lines {
            body.push('\n');
            body.push_str(&line);
        }

        let res = req
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("InfluxDB write failed with status {}: {}", status, text);
        }

        Ok(())
    }

    pub async fn fetch_json(&self, sql: &str) -> anyhow::Result<Vec<u8>> {
        let query = [
            ("format", "json"),
            ("q", sql),
            ("db", self.cfg.database.as_str()),
        ];

        let mut req = self
            .http
            .get(&format!("{}/api/v3/query_sql", self.cfg.host))
            .query(&query);

        if let Some(token) = &self.cfg.token {
            req = req.bearer_auth(token);
        }

        let res = req.send().await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("InfluxDB query failed with status {}: {}", status, text);
        }

        Ok(res.bytes().await?.to_vec())
    }
}
