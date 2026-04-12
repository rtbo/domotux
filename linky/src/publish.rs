use std::collections::HashMap;
use tokio::time;
use tokio::time::Duration;

use base::vecmap::VecMap;
use mqtt::topics::{CompteurActif, CouleurTempo};
use mqtt::{QoS, topics::CouleurTempoAujourdhui};
use serde::{Deserialize, Serialize};

use crate::tic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    broker: mqtt::BrokerAddress,
    compteurs: CompteursConfig,
    contrat: ContratConfig,
    tempo: TempoConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompteursConfig {
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    min_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContratConfig {
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    min_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TempoConfig {
    skip_aujourdhui: bool,
    skip_demain: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: "domotux.lan".parse().unwrap(),
            compteurs: CompteursConfig {
                min_interval: Duration::from_secs(60),
            },
            contrat: ContratConfig {
                min_interval: Duration::from_hours(24),
            },
            tempo: TempoConfig {
                skip_aujourdhui: true,
                skip_demain: true,
            },
        }
    }
}

#[derive(Debug)]
pub struct Client {
    client: mqtt::Client<()>,
    config: Config,

    compteurs_map: HashMap<&'static str, &'static str>,
    last_compteurs_pub: Option<time::Instant>,
    ptec_map: HashMap<&'static str, &'static str>,
    last_ptec: Option<String>,

    last_demain: Option<String>,

    last_contrat_pub: Option<time::Instant>,
    last_isousc: Option<u32>,
    last_optarif: Option<String>,
}

impl Client {
    pub fn new(config: Config) -> Self {
        let client = mqtt::Client::new("linky", config.broker.clone());
        let compteurs_map = HashMap::from([
            ("BASE", "base"),
            ("HCHC", "hc"),
            ("HCHP", "hp"),
            ("BBRHCJB", "bleuHc"),
            ("BBRHPJB", "bleuHp"),
            ("BBRHCJW", "blancHc"),
            ("BBRHPJW", "blancHp"),
            ("BBRHCJR", "rougeHc"),
            ("BBRHPJR", "rougeHp"),
        ]);

        let ptec_map = HashMap::from([
            ("TH..", "th"),
            ("HC..", "hc"),
            ("HP..", "hp"),
            ("HCJB", "bleuHc"),
            ("HPJB", "bleuHp"),
            ("HCJW", "blancHc"),
            ("HPJW", "blancHp"),
            ("HCJR", "rougeHc"),
            ("HPJR", "rougeHp"),
        ]);

        Self {
            client,
            config,

            compteurs_map,
            last_compteurs_pub: None,
            ptec_map,
            last_ptec: None,

            last_demain: None,

            last_isousc: None,
            last_optarif: None,
            last_contrat_pub: None,
        }
    }

    pub async fn publish(&mut self, tic_frame: &[(String, tic::Value)]) -> anyhow::Result<()> {
        self.publish_papp(&tic_frame).await?;
        self.publish_compteurs(&tic_frame).await?;
        if !self.config.tempo.skip_demain {
            self.publish_demain(&tic_frame).await?;
        }
        self.publish_contrat(&tic_frame).await?;

        Ok(())
    }

    async fn publish_papp(&self, tic_frame: &[(String, tic::Value)]) -> anyhow::Result<()> {
        let power = tic_frame
            .iter()
            .find(|(field, _)| field == "PAPP")
            .map(|(_, value)| value)
            .and_then(|v| v.as_f32());

        if let Some(power) = power {
            self.client
                .publish(&mqtt::topics::PApp(power), QoS::AtMostOnce, false)
                .await?;
        }
        Ok(())
    }

    async fn publish_compteurs(
        &mut self,
        tic_frame: &[(String, tic::Value)],
    ) -> anyhow::Result<()> {
        if let Some(last_compteurs_pub) = self.last_compteurs_pub {
            if last_compteurs_pub.elapsed() < self.config.compteurs.min_interval {
                log::debug!("Skipping compteurs publish because min_interval not reached");
                return Ok(());
            }
        }

        let mut ptec = None;
        let mut compteurs = VecMap::new();

        for (tic_field, value) in tic_frame {
            if tic_field == "PTEC" {
                let tic::Value::String(s) = value.clone() else {
                    log::warn!("PTEC {} is not a string, got {:?}", tic_field, value);
                    continue;
                };
                let ptec_key = self
                    .ptec_map
                    .get(s.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Unknown PTEC value '{}'", s))?;
                ptec = Some(ptec_key.to_string());
                continue;
            }

            if let Some(meter_key) = self.compteurs_map.get(tic_field.as_str()) {
                let tic::Value::Integer(i) = value else {
                    log::warn!(
                        "Meter field {} is not an integer, got {:?}",
                        tic_field,
                        value
                    );
                    continue;
                };
                compteurs.push_no_check(meter_key.to_string(), *i as u32);
            }
        }

        if compteurs.is_empty() {
            log::warn!("No compteur field found in TIC frame, skipping MQTT publish");
            return Ok(());
        }

        let msg = mqtt::topics::Compteurs {
            active: ptec.clone(),
            compteurs,
        };

        self.client.publish(&msg, QoS::AtLeastOnce, true).await?;

        if self.last_ptec != ptec
            && let Some(active) = ptec.as_ref()
        {
            self.client
                .publish(&CompteurActif(active.clone()), QoS::AtLeastOnce, true)
                .await?;

            if !self.config.tempo.skip_aujourdhui {
                let couleur_tempo = match active.as_str() {
                    "bleuHp" | "bleuHc" => Some(CouleurTempo::Bleu),
                    "blancHp" | "blancHc" => Some(CouleurTempo::Blanc),
                    "rougeHp" | "rougeHc" => Some(CouleurTempo::Rouge),
                    _ => None,
                };

                self.client
                    .publish(
                        &CouleurTempoAujourdhui(couleur_tempo),
                        QoS::AtLeastOnce,
                        true,
                    )
                    .await?;
            }
        }

        self.last_compteurs_pub = Some(time::Instant::now());
        self.last_ptec = ptec;

        Ok(())
    }

    pub async fn publish_demain(
        &mut self,
        tic_frame: &[(String, tic::Value)],
    ) -> anyhow::Result<()> {
        let demain = tic_frame
            .iter()
            .find(|(field, _)| field == "DEMAIN")
            .map(|(_, value)| value)
            .map(|v| v.to_string());

        if demain == self.last_demain {
            log::debug!("Skipping to publish couleur demain, because same as before");
            return Ok(());
        }

        if let Some(demain) = demain.as_ref() {
            let couleur_tempo = match demain.as_str() {
                "BLEU" => Some(CouleurTempo::Bleu),
                "BLAN" => Some(CouleurTempo::Blanc),
                "ROUG" => Some(CouleurTempo::Rouge),
                _ => None,
            };
            self.client
                .publish(
                    &mqtt::topics::CouleurTempoDemain(couleur_tempo),
                    QoS::AtLeastOnce,
                    true,
                )
                .await?;
        }
        Ok(())
    }

    async fn publish_contrat(&mut self, tic_frame: &[(String, tic::Value)]) -> anyhow::Result<()> {
        let isousc = tic_frame
            .iter()
            .find(|(field, _)| field == "ISOUSC")
            .and_then(|(_, value)| value.as_u32());

        let optarif = tic_frame
            .iter()
            .find(|(field, _)| field == "OPTARIF")
            .map(|(_, value)| value.to_string());

        let time_elapsed = self
            .last_contrat_pub
            .map(|last_pub| last_pub.elapsed() < self.config.contrat.min_interval)
            .unwrap_or(true);

        // Skipping if value did not change AND if min_interval is not elapsed
        if time_elapsed && isousc == self.last_isousc && optarif == self.last_optarif {
            log::debug!("Skipping contrat publish because min_interval not reached");
            return Ok(());
        }

        let Some(isousc) = isousc else {
            log::warn!("ISOUSC field not found in TIC frame, skipping contract publish");
            return Ok(());
        };

        let Some(optarif) = optarif else {
            log::warn!("OPTARIF field not found in TIC frame, skipping contract publish");
            return Ok(());
        };
        let optarifs = optarif.as_str().trim();

        let option = if optarifs == "BASE" {
            "base"
        } else if optarifs.starts_with("HC") {
            "hchp"
        } else if optarifs.starts_with("BBR") {
            "tempo"
        } else {
            log::warn!(
                "Unknown OPTARIF value '{}', skipping contract publish",
                optarif
            );
            return Ok(());
        };

        let contrat = mqtt::topics::Contrat {
            subsc_power: Some(isousc * 200 / 1000),
            option: Some(option.to_string()),
        };

        self.client
            .publish(&contrat, QoS::AtLeastOnce, true)
            .await?;

        self.last_contrat_pub = Some(time::Instant::now());
        self.last_isousc = Some(isousc);
        self.last_optarif = Some(optarif);

        Ok(())
    }
}
