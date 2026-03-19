use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt},
    sync,
};
use tokio_serial::SerialStream;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum TicValue {
    Integer(i64),
    Float(f64),
    String(String),
}

impl TicValue {
    pub fn from_str(s: &str) -> TicValue {
        if let Ok(i) = s.parse::<i64>() {
            TicValue::Integer(i)
        } else if let Ok(f) = s.parse::<f64>() {
            TicValue::Float(f)
        } else {
            TicValue::String(s.to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TicFrame(pub Vec<(String, TicValue)>);

/// Configuration for the TIC reader
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the serial device (typically /dev/ttyAMA0)
    device_path: String,
    /// Mode to calculate checksum (1 or 2)
    checksum_mode: u32,
    /// Fields to be ignored during reading
    ignore: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_path: "/dev/ttyAMA0".to_string(),
            checksum_mode: 1,
            ignore: vec![
                "ADCO".to_string(),
                "ISOUSC".to_string(),
                "IMAX".to_string(),
                "MOTDETAT".to_string(),
            ],
        }
    }
}

fn compute_checksum(data: &[u8]) -> u8 {
    let s1 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    (s1 & 0x3F) + 0x20
}

pub async fn read_frames(
    cfg: Config,
    tx: sync::mpsc::Sender<TicFrame>,
) -> Result<(), anyhow::Error> {
    log::debug!("Starting TIC reader with config: {:?}", cfg);

    let dev = SerialStream::open(
        &tokio_serial::new(&cfg.device_path, 1200)
            .data_bits(tokio_serial::DataBits::Seven)
            .parity(tokio_serial::Parity::Even)
            .stop_bits(tokio_serial::StopBits::One)
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

        let mut fields = Vec::new();

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

            let label = parts.next().and_then(|s| str::from_utf8(s).ok());
            if let Some(label_str) = label {
                if cfg.ignore.iter().any(|ignore| ignore == label_str) {
                    continue 'field;
                }
            }

            let value = parts.next().and_then(|s| str::from_utf8(s).ok());

            let (label, value) = match (label, value) {
                (Some(l), Some(v)) if !l.is_empty() && !v.is_empty() => {
                    (l.to_string(), TicValue::from_str(v))
                }
                (Some(_), Some(_)) => {
                    log::warn!("Ignoring empty label or value");
                    continue 'frame;
                }
                (_, _) => {
                    log::warn!("Ignoring non-ASCII field: {:?}", buf);
                    continue 'frame;
                }
            };

            log::debug!("Parsed field: {} = {:?}", label, value);
            fields.push((label, value));
        }

        let tic_frame = TicFrame(fields);
        log::debug!("Completed frame: {:?}", tic_frame);
        tx.send(tic_frame.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send TIC frame through channel: {}", e))?;
    }
}
