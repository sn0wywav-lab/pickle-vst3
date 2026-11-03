//! Minimal REST client for the endpoints the client needs.

use crate::model::{Message, User};
use crate::props::ClientProperties;
use crate::snowflake::Snowflake;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use std::time::Duration;

pub const API_BASE: &str = "https://discord.com/api/v9";

#[derive(Debug, thiserror::Error)]
pub enum RestError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Discord returned {status}: {message}")]
    Api { status: u16, message: String },
    #[error("unauthorized (token invalid or expired)")]
    Unauthorized,
    #[error("missing permissions to do that here")]
    Forbidden,
}

/// Cheap to clone (reqwest clients are reference counted).
#[derive(Clone)]
pub struct Rest {
    /// Authenticated client: only ever used for discord.com/api.
    api: reqwest::Client,
    /// Unauthenticated client for CDN / external downloads. The token is
    /// never attached to these requests.
    plain: reqwest::Client,
}

impl Rest {
    pub fn new(token: &str, props: &ClientProperties) -> Self {
        let mut h = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(token) {
            let mut v = v;
            v.set_sensitive(true);
            h.insert(AUTHORIZATION, v);
        }
        if let Ok(v) = HeaderValue::from_str(&props.browser_user_agent) {
            h.insert(USER_AGENT, v);
        }
        if let Ok(v) = HeaderValue::from_str(&props.super_properties_b64()) {
            h.insert("X-Super-Properties", v);
        }
        h.insert("X-Discord-Locale", HeaderValue::from_static("en-US"));
        h.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.9"));
        h.insert("Origin", HeaderValue::from_static("https://discord.com"));

        let api = reqwest::Client::builder()
            .default_headers(h)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_nodelay(true)
            .build()
            .expect("reqwest client");
        Self { api, plain: plain_client(props) }
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T, RestError> {
        let url = format!("{API_BASE}{path}");
        for attempt in 0..3 {
            let mut req = self.api.request(method.clone(), &url);
            if let Some(b) = &body {
                req = req.json(b);
            }
            let resp = req.send().await?;
            let status = resp.status();
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 2 {
                #[derive(serde::Deserialize)]
                struct RateLimit {
                    retry_after: f64,
                }
                let wait = resp
                    .json::<RateLimit>()
                    .await
                    .map(|r| r.retry_after)
                    .unwrap_or(1.0)
                    .clamp(0.05, 30.0);
                tokio::time::sleep(Duration::from_secs_f64(wait)).await;
                continue;
            }
            if status == StatusCode::UNAUTHORIZED {
                return Err(RestError::Unauthorized);
            }
            if status == StatusCode::FORBIDDEN {
                return Err(RestError::Forbidden);
            }
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                let message = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                    .unwrap_or(text);
                return Err(RestError::Api { status: status.as_u16(), message });
            }
            if status == StatusCode::NO_CONTENT {
                return serde_json::from_str("null").map_err(|e| RestError::Api {
                    status: 204,
                    message: e.to_string(),
                });
            }
            return Ok(resp.json().await?);
        }
        Err(RestError::Api { status: 429, message: "rate limited".into() })
    }

    pub async fn me(&self) -> Result<User, RestError> {
        self.request(Method::GET, "/users/@me", None).await
    }

    /// Newest-first page of messages, returned oldest-first.
    pub async fn messages(
        &self,
        channel: Snowflake,
        before: Option<Snowflake>,
        limit: u8,
    ) -> Result<Vec<Message>, RestError> {
        let limit = limit.clamp(1, 100);
        let path = match before {
            Some(b) => format!("/channels/{channel}/messages?limit={limit}&before={b}"),
            None => format!("/channels/{channel}/messages?limit={limit}"),
        };
        let mut msgs: Vec<Message> = self.request(Method::GET, &path, None).await?;
        msgs.reverse();
        Ok(msgs)
    }

    pub async fn send_message(
        &self,
        channel: Snowflake,
        content: &str,
        nonce: Snowflake,
        reply_to: Option<Snowflake>,
    ) -> Result<Message, RestError> {
        let mut body = serde_json::json!({
            "content": content,
            "nonce": nonce.to_string(),
            "tts": false,
            "flags": 0,
        });
        if let Some(r) = reply_to {
            body["message_reference"] = serde_json::json!({ "message_id": r, "channel_id": channel });
        }
        self.request(Method::POST, &format!("/channels/{channel}/messages"), Some(body))
            .await
    }

    pub async fn edit_message(
        &self,
        channel: Snowflake,
        message: Snowflake,
        content: &str,
    ) -> Result<Message, RestError> {
        self.request(
            Method::PATCH,
            &format!("/channels/{channel}/messages/{message}"),
            Some(serde_json::json!({ "content": content })),
        )
        .await
    }

    pub async fn delete_message(&self, channel: Snowflake, message: Snowflake) -> Result<(), RestError> {
        let _: serde_json::Value = self
            .request(Method::DELETE, &format!("/channels/{channel}/messages/{message}"), None)
            .await?;
        Ok(())
    }

    pub async fn typing(&self, channel: Snowflake) -> Result<(), RestError> {
        let _: serde_json::Value = self
            .request(Method::POST, &format!("/channels/{channel}/typing"), None)
            .await?;
        Ok(())
    }

    /// Download bytes without credentials (CDN images, theme imports...).
    pub async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>, RestError> {
        fetch_bytes(&self.plain, url).await
    }

    pub fn plain_client(&self) -> reqwest::Client {
        self.plain.clone()
    }
}

pub fn plain_client(props: &ClientProperties) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(props.browser_user_agent.clone())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("reqwest client")
}

pub async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, RestError> {
    let resp = client.get(url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(RestError::Api { status: status.as_u16(), message: url.to_string() });
    }
    Ok(resp.bytes().await?.to_vec())
}
