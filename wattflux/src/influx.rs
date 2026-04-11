use std::time::UNIX_EPOCH;

use mqtt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    token: Option<String>,
    database: String,
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

#[derive(Debug)]
pub struct Client {
    http: reqwest::Client,
    cfg: Config,
}

pub trait AsLine {
    fn as_line(&self) -> String;
}

impl AsLine for &(mqtt::topics::PApp, std::time::SystemTime) {
    fn as_line(&self) -> String {
        format!("papp value={} {}", self.0.0, self.1.duration_since(UNIX_EPOCH).unwrap().as_secs())
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
        body.push_str(&lines.next().ok_or_else(|| anyhow::anyhow!("No lines to write"))?);
        let mut cnt = 1;
        for line in lines {
            body.push('\n');
            body.push_str(&line);
            cnt += 1;
        }
        log::debug!("Writing {} lines to InfluxDB", cnt);

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
}
