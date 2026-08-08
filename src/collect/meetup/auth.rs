use std::{env, fs};

use anyhow::{Context, bail};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;

const PRIVATE_KEY_ENV: &str = "MEETUP_PRIVATE_KEY";
const MEMBER_ID_ENV: &str = "MEETUP_AUTHORIZED_MEMBER_ID";
const CLIENT_KEY_ENV: &str = "MEETUP_CLIENT_KEY";
const SIGNING_KEY_ID_ENV: &str = "MEETUP_SIGNING_KEY_ID";
const JWT_LIFETIME_SECONDS: i64 = 120;

pub struct MeetupCredentials {
    member_id: String,
    client_key: String,
    signing_key_id: Option<String>,
    private_key_pem: Vec<u8>,
}

#[derive(Serialize)]
struct Claims<'a> {
    sub: &'a str,
    iss: &'a str,
    aud: &'static str,
    exp: i64,
}

impl MeetupCredentials {
    pub fn from_env() -> anyhow::Result<Self> {
        let private_key_path = required_env(PRIVATE_KEY_ENV)?;
        let private_key_pem = fs::read(&private_key_path).with_context(|| {
            format!("failed to read Meetup private key from path in {PRIVATE_KEY_ENV}")
        })?;

        Ok(Self {
            member_id: required_env(MEMBER_ID_ENV)?,
            client_key: required_env(CLIENT_KEY_ENV)?,
            signing_key_id: optional_env(SIGNING_KEY_ID_ENV),
            private_key_pem,
        })
    }

    pub fn signed_jwt(&self) -> anyhow::Result<String> {
        let key = EncodingKey::from_rsa_pem(&self.private_key_pem)
            .context("failed to parse Meetup RSA private key")?;
        let claims = Claims {
            sub: &self.member_id,
            iss: &self.client_key,
            aud: "api.meetup.com",
            exp: jwt_expiration(Utc::now().timestamp()),
        };
        let header = jwt_header(self.signing_key_id.as_deref());

        encode(&header, &claims, &key).context("failed to sign Meetup authentication JWT")
    }
}

fn jwt_header(signing_key_id: Option<&str>) -> Header {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = signing_key_id.map(str::to_owned);
    header
}

fn jwt_expiration(now: i64) -> i64 {
    now + Duration::seconds(JWT_LIFETIME_SECONDS).num_seconds()
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn required_env(name: &str) -> anyhow::Result<String> {
    let value =
        env::var(name).with_context(|| format!("environment variable {name} is not set"))?;
    if value.trim().is_empty() {
        bail!("environment variable {name} is empty");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_uses_documented_header_and_lifetime() {
        let header = jwt_header(Some("signing-key-id"));

        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.typ.as_deref(), Some("JWT"));
        assert_eq!(header.kid.as_deref(), Some("signing-key-id"));
        assert_eq!(jwt_expiration(1_000), 1_120);
    }

    #[test]
    fn jwt_signing_key_id_is_optional() {
        assert_eq!(jwt_header(None).kid, None);
    }
}
