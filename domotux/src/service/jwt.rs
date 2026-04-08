
use base64::prelude::*;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;


#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    alg: String,
    typ: String,
}

fn to_json_base64<T: Serialize>(value: &T) -> anyhow::Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(BASE64_URL_SAFE_NO_PAD.encode(json.as_bytes()))
}

fn from_json_base64<T: for<'de> Deserialize<'de>>(s: &str) -> anyhow::Result<T> {
    let bytes = BASE64_URL_SAFE_NO_PAD.decode(s)?;
    let value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

fn sign(unsigned_token: &str, secret_key: &str) -> anyhow::Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())?;
    mac.update(unsigned_token.as_bytes());
    let signature = mac.finalize().into_bytes();
    Ok(BASE64_URL_SAFE_NO_PAD.encode(&signature))
}

pub fn generate<C>(claims: C, secret_key: &str) -> anyhow::Result<String>
where
    C: Serialize,
{
    let header = Header {
        alg: "HS256".to_string(),
        typ: "JWT".to_string(),
    };
    let header = to_json_base64(&header)?;
    let claims = to_json_base64(&claims)?;
    let unsigned_token = format!("{}.{}", header, claims);
    let sig = sign(&unsigned_token, secret_key)?;
    let token = format!("{}.{}", unsigned_token, sig);
    Ok(token)
}

pub fn verify<C>(token: &str, secret_key: &str) -> anyhow::Result<C>
where
    C: for<'de> Deserialize<'de>,
{
    let mut parts = token.split('.');
    let header = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid token format"))?;
    let claims = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid token format"))?;
    let sig = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid token format"))?;
    if parts.next().is_some() {
        anyhow::bail!("Invalid token format");
    }
    let unsigned_token = &token[..header.len() + 1 + claims.len()];

    let header: Header = from_json_base64(header)?;
    if header.alg != "HS256" {
        anyhow::bail!("Unsupported algorithm");
    }

    if sig != sign(unsigned_token, secret_key)? {
        anyhow::bail!("Invalid token signature");
    }

    let claims: C = from_json_base64(claims)?;
    Ok(claims)
}
