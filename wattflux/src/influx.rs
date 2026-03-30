use base::mqtt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    host: String,
    token: Option<String>,
    database: String,
    power: String,
    meters: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "http://localhost:8181".to_string(),
            token: None,
            database: "domotux".to_string(),
            power: "papp".to_string(),
            meters: "compteurs".to_string(),
        }
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

    pub async fn write_power_line(&self, power: mqtt::topics::AppPower) -> anyhow::Result<()>
    {
        let line = format!("{} value={}", self.cfg.power, power.0);
        self.write_line(line).await?;
        Ok(())
    }

    pub async fn write_meters_line(&self, meters: mqtt::topics::Meters) -> anyhow::Result<()>
    {
        let mut line = String::new();
        if let Some(active) = &meters.active {
            line.push_str(&format!("active=\"{}\"", active));

        }
        for (key, value) in meters.meters.iter() {
             if !line.is_empty() {
                line.push(',');
            }
             line.push_str(&format!("{}={}", key, value));
        }
        if line.is_empty() {
            anyhow::bail!("No meter data to write");
        }

         let line = format!("{} {}", self.cfg.meters, line);
         self.write_line(line).await?;
        Ok(())
    }

    async fn write_line(&self, line: String) -> anyhow::Result<()> {
        let query = [("db", self.cfg.database.as_str()), ("precision", "s")];

        let mut req = self
            .http
            .post(&format!("{}/api/v3/write_lp", self.cfg.host))
            .query(&query);

        if let Some(token) = &self.cfg.token {
            req = req.bearer_auth(token);
        }

        let res = req.header("Content-Type", "text/plain; charset=utf-8")
            .header("Accept", "application/json")
            .body(line)
            .send().await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("InfluxDB write failed with status {}: {}", status, text);
        }

        Ok(())
    }
}
