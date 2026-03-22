use std::{fmt, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt},
    sync,
};
use tokio_serial::{DataBits, Parity, SerialStream, StopBits};

/// Configuration for the TIC reader
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the serial device (typically /dev/ttyAMA0)
    device_path: String,
    /// Mode to calculate checksum (1 or 2)
    checksum_mode: u32,
    /// Fields to be ignored
    ignore: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_path: "/dev/ttyAMA0".to_string(),
            checksum_mode: 1,
            ignore: vec![
                "MOTDETAT".to_string(),
                // Couleur du lendemain.
                // Arrive à 20H sur le TIC, on peut l'avoir dès 11H avec service web.
                "DEMAIN".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Integer(i32),
    Float(f32),
    String(String),
}

impl Value {
    pub fn from_str(s: &str) -> Self {
        if let Ok(i) = s.parse::<i32>() {
            Value::Integer(i)
        } else if let Ok(f) = s.parse::<f32>() {
            Value::Float(f)
        } else {
            Value::String(s.to_string())
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "{}", s),
        }
    }
}

fn compute_checksum(data: &[u8]) -> u8 {
    let s1 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    (s1 & 0x3F) + 0x20
}

pub async fn read_loop(
    cfg: Config,
    tx: sync::mpsc::Sender<Vec<(String, Value)>>,
) -> Result<(), anyhow::Error> {
    log::debug!("Starting TIC reader with config: {:?}", cfg);

    let dev = SerialStream::open(
        &tokio_serial::new(&cfg.device_path, 1200)
            .data_bits(DataBits::Seven)
            .parity(Parity::Even)
            .stop_bits(StopBits::One)
            .timeout(Duration::from_millis(1000)),
    )?;
    let mut reader = tokio::io::BufReader::new(dev);

    log::info!("Serial device opened successfully: {}", cfg.device_path);

    let checksum_skip_bytes = match cfg.checksum_mode {
        1 => 3,
        2 => 2,
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid checksum mode: {}",
                cfg.checksum_mode
            ));
        }
    };

    let mut buf = Vec::new();
    let mut last_len = 16;

    'frame: loop {
        log::debug!("Waiting for start of frame...");

        const STX: u8 = 2; // STX, Start of Text
        const ETX: u8 = 3; // ETX, End of Text

        const FIELD_START: u8 = b'\n';
        const FIELD_END: u8 = b'\r';

        while reader.read_u8().await? != STX {
            // Wait for data
        }

        log::debug!("Start of frame detected");
        let mut fields = Vec::with_capacity(last_len);

        'field: loop {
            buf.clear();
            let c = reader.read_u8().await?;
            if c == ETX {
                break;
            }
            if c != FIELD_START {
                log::warn!("Ignoring unexpected start of field: {}", c);
                continue 'frame;
            }

            reader.read_until(FIELD_END, &mut buf).await?;

            if buf.len() < 7 {
                log::warn!("Ignoring short field: {:?}", buf);
                continue 'frame;
            }

            let frame = &buf[..];

            // checksum just before carriage return
            let checksum = frame[frame.len() - 2];

            let checked_range = &frame[..frame.len() - checksum_skip_bytes];
            if compute_checksum(checked_range) != checksum {
                log::warn!(
                    "Invalid checksum for frame: {:?}",
                    String::from_utf8_lossy(frame)
                );
                continue 'frame;
            }

            let sep = frame[frame.len() - 3];
            let mut parts = frame.split(|&b| b == sep);

            let Some(label) = parts.next().and_then(|s| str::from_utf8(s).ok()) else {
                log::warn!("Ignoring non-ASCII field label: {:?}", buf);
                continue 'frame;
            };

            if cfg.ignore.iter().any(|f| f == label) {
                continue 'field;
            }

            let Some(value) = parts.next().and_then(|s| str::from_utf8(s).ok()) else {
                log::warn!("Ignoring non-ASCII field value: {:?}", buf);
                continue 'frame;
            };

            if label.is_empty() || value.is_empty() {
                log::warn!("Ignoring empty label or value: {:?}", buf);
                continue 'frame;
            }

            let label = label.to_string();
            let value = Value::from_str(value);

            log::debug!("Parsed field: {} = {:?}", label, value);
            fields.push((label, value));
        }

        last_len = fields.len();
        tx.send(fields)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send TIC field through channel: {}", e))?;
    }
}
