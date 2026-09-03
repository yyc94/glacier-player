// SPDX-License-Identifier: GPL-3.0-only

//! Small, provider-specific HTTP layer for QQMusicApi.
//!
//! The Web service owns QQ Music protocol details. Glacier Player only needs
//! to send ordinary HTTP requests, attach credential cookies, validate the
//! common response envelope, and hand typed payloads to the domain adapter.

use reqwest::{Method, RequestBuilder, Url, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt;

use super::models::{
    QqAlbumDetailData, QqAlbumSongsData, QqFavoriteAlbumsData, QqLyricsData, QqPlaylistDetailData, QqSearchData,
    QqSimilarSingersData, QqSingerAlbumsData, QqSingerInfoData, QqSingerSongsData, QqSongDetailData, QqSongListsData,
};
use super::sidecar::QqMusicSidecar;

const SONG_URL_FALLBACK_CDN: &str = "https://isure.stream.qqmusic.qq.com/";

/// A QQ Music credential returned by the Web service login endpoints.
///
/// The two core fields are required by QQMusicApi. The remaining fields are
/// optional but are retained so refresh and account-specific endpoints keep
/// working across login methods.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct QqCredential {
    #[serde(default, deserialize_with = "string_from_any")]
    pub musicid: String,
    #[serde(default, deserialize_with = "string_from_any")]
    pub musickey: String,
    #[serde(default, deserialize_with = "string_from_any")]
    pub openid: String,
    #[serde(default, deserialize_with = "string_from_any")]
    pub refresh_token: String,
    #[serde(default, deserialize_with = "string_from_any")]
    pub access_token: String,
    #[serde(default, deserialize_with = "u64_from_any")]
    pub expired_at: u64,
    #[serde(default, deserialize_with = "string_from_any")]
    pub unionid: String,
    #[serde(default, deserialize_with = "string_from_any")]
    pub str_musicid: String,
    #[serde(default, deserialize_with = "string_from_any")]
    pub refresh_key: String,
    #[serde(default, alias = "musickeyCreateTime", deserialize_with = "u64_from_any")]
    pub musickey_create_time: u64,
    #[serde(default, alias = "keyExpiresIn", deserialize_with = "u64_from_any")]
    pub key_expires_in: u64,
    #[serde(default, alias = "firstLogin", deserialize_with = "u32_from_any")]
    pub first_login: u32,
    #[serde(default, alias = "bindAccountType", deserialize_with = "u32_from_any")]
    pub bind_account_type: u32,
    #[serde(default, alias = "needRefreshKeyIn", deserialize_with = "u64_from_any")]
    pub need_refresh_key_in: u64,
    #[serde(default, alias = "encryptUin", deserialize_with = "string_from_any")]
    pub encrypt_uin: String,
    #[serde(default, alias = "loginType", deserialize_with = "u32_from_any")]
    pub login_type: u32,
}

impl fmt::Debug for QqCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QqCredential")
            .field("musicid", &self.musicid)
            .field("musickey", &"<redacted>")
            .field("openid", &(!self.openid.is_empty()).then_some("<present>"))
            .field("refresh_token", &(!self.refresh_token.is_empty()).then_some("<present>"))
            .field("access_token", &(!self.access_token.is_empty()).then_some("<present>"))
            .field("expired_at", &self.expired_at)
            .field("unionid", &(!self.unionid.is_empty()).then_some("<present>"))
            .field("str_musicid", &self.str_musicid)
            .field("refresh_key", &(!self.refresh_key.is_empty()).then_some("<present>"))
            .field("encrypt_uin", &(!self.encrypt_uin.is_empty()).then_some("<present>"))
            .field("login_type", &self.login_type)
            .finish()
    }
}

impl QqCredential {
    /// Whether the credential contains the two fields required by QQMusicApi.
    pub fn is_authenticated(&self) -> bool {
        !self.musicid.trim().is_empty() && !self.musickey.trim().is_empty()
    }

    /// Build the Cookie header expected by the Web service.
    pub fn cookie_header(&self) -> Option<String> {
        if !self.is_authenticated() {
            return None;
        }

        let mut fields = vec![format!("musicid={}", self.musicid), format!("musickey={}", self.musickey)];
        let optional = [
            ("openid", self.openid.clone()),
            ("refresh_token", self.refresh_token.clone()),
            ("access_token", self.access_token.clone()),
            ("unionid", self.unionid.clone()),
            ("str_musicid", self.str_musicid.clone()),
            ("refresh_key", self.refresh_key.clone()),
            ("encrypt_uin", self.encrypt_uin.clone()),
        ];
        fields.extend(optional.into_iter().filter(|(_, value)| !value.is_empty()).map(|(name, value)| format!("{name}={value}")));
        if self.expired_at != 0 {
            fields.push(format!("expired_at={}", self.expired_at));
        }
        Some(fields.join("; "))
    }
}

/// The response envelope returned by every QQMusicApi Web route.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiEnvelope<T> {
    pub code: i32,
    #[serde(default)]
    pub msg: String,
    pub data: Option<T>,
}

/// One item in the `/song/{mid}/url` response.
#[derive(Debug, Clone, Deserialize)]
pub struct SongUrlItem {
    #[serde(default, alias = "songmid")]
    pub mid: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub purl: String,
    #[serde(default)]
    pub vkey: String,
    #[serde(default, deserialize_with = "i32_from_any")]
    pub result: i32,
}

/// Payload returned by QQMusicApi's song URL routes.
#[derive(Debug, Clone, Deserialize)]
pub struct SongUrlPayload {
    #[serde(default, deserialize_with = "u64_from_any")]
    pub expiration: u64,
    #[serde(default, alias = "midurlinfo")]
    pub data: Vec<SongUrlItem>,
}

impl SongUrlPayload {
    /// Return the first successful stream URL in a form GStreamer can use.
    pub fn first_playable_url(&self) -> Option<String> {
        self.data.iter().find(|item| item.result == 0 && !item.purl.is_empty()).map(|item| {
            if item.purl.starts_with("http://") || item.purl.starts_with("https://") {
                item.purl.clone()
            } else {
                format!("{}{}", SONG_URL_FALLBACK_CDN, item.purl.trim_start_matches('/'))
            }
        })
    }
}

/// QR image returned by `/login/qrcode/{login_type}`.
#[derive(Debug, Clone, Deserialize)]
pub struct QqQrCodeData {
    #[serde(default)]
    pub qr_type: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub mimetype: String,
    #[serde(default)]
    pub data: String,
    #[serde(default)]
    pub img: String,
}

/// QR status returned by `/login/qrcode/{login_type}/status`.
#[derive(Debug, Clone, Deserialize)]
pub struct QqQrCodeStatus {
    #[serde(default, deserialize_with = "i32_list_from_any")]
    pub event: Vec<i32>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub credential: Option<QqCredential>,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    #[serde(deserialize_with = "string_from_any")]
    pub login_type: String,
}

/// Errors raised by the QQMusicApi HTTP boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QqMusicError {
    InvalidBaseUrl(String),
    Http(String),
    Api { code: i32, message: String },
    InvalidResponse(String),
    NotAuthenticated,
    Sidecar(String),
}

impl fmt::Display for QqMusicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaseUrl(message) => write!(f, "invalid QQMusicApi base URL: {message}"),
            Self::Http(message) => write!(f, "QQMusicApi HTTP request failed: {message}"),
            Self::Api { code, message } => write!(f, "QQMusicApi error {code}: {message}"),
            Self::InvalidResponse(message) => write!(f, "invalid QQMusicApi response: {message}"),
            Self::NotAuthenticated => write!(f, "QQ Music authentication is required"),
            Self::Sidecar(message) => write!(f, "QQ Music backend unavailable: {message}"),
        }
    }
}

impl std::error::Error for QqMusicError {}

/// Result alias for QQMusicApi operations.
pub type QqResult<T> = Result<T, QqMusicError>;

/// HTTP client for the QQMusicApi Web service.
#[derive(Clone)]
pub struct QqMusicClient {
    http: reqwest::Client,
    base_url: Url,
    credential: QqCredential,
    sidecar: QqMusicSidecar,
}

impl QqMusicClient {
    /// Construct a client from a base URL such as `http://127.0.0.1:8080`.
    pub fn new(base_url: &str) -> QqResult<Self> {
        let mut normalized = base_url.trim().to_string();
        if normalized.is_empty() {
            return Err(QqMusicError::InvalidBaseUrl("URL is empty".to_string()));
        }
        if !normalized.ends_with('/') {
            normalized.push('/');
        }
        let base_url = Url::parse(&normalized).map_err(|error| QqMusicError::InvalidBaseUrl(error.to_string()))?;
        if base_url.scheme() != "http" && base_url.scheme() != "https" {
            return Err(QqMusicError::InvalidBaseUrl("scheme must be http or https".to_string()));
        }
        let http = reqwest::Client::builder()
            .user_agent("glacier-player/qqmusic")
            .build()
            .map_err(|error| QqMusicError::Http(error.to_string()))?;
        let sidecar = QqMusicSidecar::new(&base_url);
        Ok(Self { http, base_url, credential: QqCredential::default(), sidecar })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn credential(&self) -> &QqCredential {
        &self.credential
    }

    pub fn set_credential(&mut self, credential: QqCredential) {
        self.credential = credential;
    }

    pub fn clear_credential(&mut self) {
        self.credential = QqCredential::default();
    }

    /// Execute a search request. The raw payload is kept until the domain
    /// adapter is introduced, because the service exposes several search
    /// result variants behind one endpoint.
    pub async fn search_by_type(&self, keyword: &str, page: u32, num: u32) -> QqResult<QqSearchData> {
        self.get_json(
            "search/search_by_type",
            [
                ("keyword".to_string(), keyword.to_string()),
                ("search_type".to_string(), "0".to_string()),
                ("num".to_string(), num.to_string()),
                ("page".to_string(), page.to_string()),
            ],
        )
        .await
    }

    pub async fn song_detail(&self, value: &str) -> QqResult<QqSongDetailData> {
        self.get_json(&format!("song/{value}/detail"), []).await
    }

    pub async fn lyrics(&self, value: &str) -> QqResult<QqLyricsData> {
        self.get_json(&format!("song/{value}/lyric"), [("trans".to_string(), "true".to_string())]).await
    }

    pub async fn playlist_detail(&self, songlist_id: &str, page: u32, num: u32) -> QqResult<QqPlaylistDetailData> {
        self.get_json(
            &format!("songlist/{songlist_id}/detail"),
            [("page".to_string(), page.to_string()), ("num".to_string(), num.to_string())],
        )
        .await
    }

    pub async fn album_detail(&self, value: &str) -> QqResult<QqAlbumDetailData> {
        self.get_json(&format!("album/{value}/detail"), []).await
    }

    pub async fn album_songs(&self, value: &str, page: u32, num: u32) -> QqResult<QqAlbumSongsData> {
        self.get_json(
            &format!("album/{value}/songs"),
            [("page".to_string(), page.to_string()), ("num".to_string(), num.to_string())],
        )
        .await
    }

    pub async fn singer_info(&self, mid: &str) -> QqResult<QqSingerInfoData> {
        self.get_json(&format!("singer/{mid}/info"), []).await
    }

    pub async fn singer_songs(&self, mid: &str, page: u32, num: u32) -> QqResult<QqSingerSongsData> {
        self.get_json(
            &format!("singer/{mid}/songs"),
            [("page".to_string(), page.to_string()), ("num".to_string(), num.to_string())],
        )
        .await
    }

    pub async fn singer_albums(&self, mid: &str, page: u32, num: u32) -> QqResult<QqSingerAlbumsData> {
        self.get_json(
            &format!("singer/{mid}/albums"),
            [("page".to_string(), page.to_string()), ("num".to_string(), num.to_string())],
        )
        .await
    }

    pub async fn similar_singers(&self, mid: &str, number: u32) -> QqResult<QqSimilarSingersData> {
        self.get_json(&format!("singer/{mid}/similar"), [("number".to_string(), number.to_string())]).await
    }

    pub async fn user_created_songlists(&self, uin: &str) -> QqResult<QqSongListsData> {
        self.require_auth()?;
        self.get_json(&format!("user/{uin}/created_songlists"), []).await
    }

    pub async fn user_favorite_songlists(&self, euin: &str, page: u32, num: u32) -> QqResult<QqSongListsData> {
        self.require_auth()?;
        self.get_json(
            &format!("user/{euin}/fav/songlists"),
            [("page".to_string(), page.to_string()), ("num".to_string(), num.to_string())],
        )
        .await
    }

    pub async fn user_favorite_songs(&self, euin: &str, page: u32, num: u32) -> QqResult<QqPlaylistDetailData> {
        self.require_auth()?;
        self.get_json(
            &format!("user/{euin}/fav/songs"),
            [("page".to_string(), page.to_string()), ("num".to_string(), num.to_string())],
        )
        .await
    }

    pub async fn user_favorite_albums(&self, euin: &str, page: u32, num: u32) -> QqResult<QqFavoriteAlbumsData> {
        self.require_auth()?;
        self.get_json(
            &format!("user/{euin}/fav/albums"),
            [("page".to_string(), page.to_string()), ("num".to_string(), num.to_string())],
        )
        .await
    }

    pub async fn song_url(
        &self,
        mid: &str,
        file_type: u8,
        song_type: Option<i32>,
        media_mid: Option<&str>,
    ) -> QqResult<SongUrlPayload> {
        let mut query = vec![("file_type".to_string(), file_type.to_string())];
        if let Some(song_type) = song_type {
            query.push(("song_type".to_string(), song_type.to_string()));
        }
        if let Some(media_mid) = media_mid.filter(|value| !value.is_empty()) {
            query.push(("media_mid".to_string(), media_mid.to_string()));
        }
        self.get_json(&format!("song/{mid}/url"), query).await
    }

    pub async fn check_login_expired(&self) -> QqResult<bool> {
        let request = self.http.request(Method::GET, self.url("login/check_expired")?);
        let envelope: ApiEnvelope<bool> = self.send_envelope(request).await?;

        // QQMusicApi's Web executor currently represents bool results through
        // the response code: true becomes code 0 with no data, while false
        // becomes code -1. Also accept a regular boolean payload so this stays
        // compatible if the upstream route is corrected later.
        match (envelope.code, envelope.data) {
            (0, Some(expired)) => Ok(expired),
            (0, None) => Ok(true),
            (-1, _) => Ok(false),
            (code, _) => Err(QqMusicError::Api { code, message: envelope.msg }),
        }
    }

    pub async fn request_qrcode(&self, login_type: &str) -> QqResult<QqQrCodeData> {
        self.get_json(&format!("login/qrcode/{login_type}"), []).await
    }

    pub async fn check_qrcode(&self, login_type: &str, identifier: &str) -> QqResult<QqQrCodeStatus> {
        self.get_json(&format!("login/qrcode/{login_type}/status"), [("identifier".to_string(), identifier.to_string())]).await
    }

    /// Refresh the credential stored by QQMusicApi.
    pub async fn refresh_credential(&self) -> QqResult<QqCredential> {
        self.require_auth()?;
        self.get_json("login/refresh_credential", []).await
    }

    async fn get_json<T>(&self, path: &str, query: impl IntoIterator<Item = (String, String)>) -> QqResult<T>
    where
        T: DeserializeOwned,
    {
        let mut url = self.url(path)?;
        let mut query = query.into_iter().peekable();
        if query.peek().is_some() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let request = self.http.request(Method::GET, url);
        self.send(request).await
    }

    async fn send<T>(&self, request: RequestBuilder) -> QqResult<T>
    where
        T: DeserializeOwned,
    {
        let envelope = self.send_envelope(request).await?;
        if envelope.code != 0 {
            return Err(QqMusicError::Api { code: envelope.code, message: envelope.msg });
        }
        envelope.data.ok_or_else(|| QqMusicError::InvalidResponse("successful response has no data".to_string()))
    }

    async fn send_envelope<T>(&self, mut request: RequestBuilder) -> QqResult<ApiEnvelope<T>>
    where
        T: DeserializeOwned,
    {
        if let Some(cookie) = self.credential.cookie_header() {
            request = request.header(header::COOKIE, cookie);
        }
        self.sidecar.ensure_ready(&self.http).await?;
        let retry = request.try_clone();
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) if error.is_connect() && self.sidecar.is_managed() => {
                self.sidecar.mark_unavailable().await;
                self.sidecar.ensure_ready(&self.http).await?;
                retry
                    .ok_or_else(|| QqMusicError::Http(error.to_string()))?
                    .send()
                    .await
                    .map_err(|retry_error| QqMusicError::Http(retry_error.to_string()))?
            }
            Err(error) => return Err(QqMusicError::Http(error.to_string())),
        };
        let status = response.status();
        let body = response.text().await.map_err(|error| QqMusicError::Http(error.to_string()))?;
        if !status.is_success() {
            return Err(QqMusicError::Http(format!("HTTP {status}: {}", truncate(&body))));
        }
        serde_json::from_str(&body).map_err(|error| QqMusicError::InvalidResponse(format!("{error}: {}", truncate(&body))))
    }

    fn require_auth(&self) -> QqResult<()> {
        if self.credential.is_authenticated() { Ok(()) } else { Err(QqMusicError::NotAuthenticated) }
    }

    fn url(&self, path: &str) -> QqResult<Url> {
        self.base_url.join(path.trim_start_matches('/')).map_err(|error| QqMusicError::InvalidBaseUrl(error.to_string()))
    }
}

fn truncate(value: &str) -> String {
    const MAX: usize = 240;
    let mut chars = value.chars();
    let result: String = chars.by_ref().take(MAX).collect();
    if chars.next().is_some() { format!("{result}…") } else { result }
}

fn string_from_any<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        _ => String::new(),
    })
}

fn u64_from_any<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Number(value) => value.as_u64().unwrap_or(0),
        serde_json::Value::String(value) => value.parse().unwrap_or(0),
        _ => 0,
    })
}

fn u32_from_any<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(u64_from_any(deserializer)? as u32)
}

fn i32_from_any<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Number(value) => value.as_i64().unwrap_or(0) as i32,
        serde_json::Value::String(value) => value.parse().unwrap_or(0),
        _ => 0,
    })
}

fn i32_list_from_any<'de, D>(deserializer: D) -> Result<Vec<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let values = match value {
        serde_json::Value::Array(values) => values,
        value => vec![value],
    };
    Ok(values
        .into_iter()
        .filter_map(|value| match value {
            serde_json::Value::Number(value) => value.as_i64().and_then(|value| i32::try_from(value).ok()),
            serde_json::Value::String(value) => value.parse().ok(),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;
    use crate::qqmusic::models::QqSong;

    #[test]
    fn credentials_build_cookie_header_without_secrets_in_debug() {
        let credential =
            QqCredential { musicid: "123".to_string(), musickey: "secret".to_string(), expired_at: 42, ..Default::default() };
        assert_eq!(credential.cookie_header().as_deref(), Some("musicid=123; musickey=secret; expired_at=42"));
        assert!(!format!("{credential:?}").contains("secret"));
    }

    #[test]
    fn unauthenticated_credentials_do_not_emit_cookies() {
        assert_eq!(QqCredential::default().cookie_header(), None);
    }

    #[test]
    fn client_normalizes_base_url() {
        let client = QqMusicClient::new("http://localhost:8080").unwrap();
        assert_eq!(client.base_url().as_str(), "http://localhost:8080/");
        assert_eq!(client.url("song/abc/detail").unwrap().as_str(), "http://localhost:8080/song/abc/detail");
    }

    #[test]
    fn client_rejects_non_http_schemes() {
        let error = match QqMusicClient::new("file:///tmp/qq") {
            Ok(_) => panic!("file URLs must be rejected"),
            Err(error) => error,
        };
        assert!(matches!(error, QqMusicError::InvalidBaseUrl(_)));
    }

    #[test]
    fn song_url_payload_accepts_midurlinfo_alias() {
        let payload: SongUrlPayload = serde_json::from_value(serde_json::json!({
            "expiration": 60,
            "midurlinfo": [{"songmid": "abc", "purl": "song.mp3", "result": 0}]
        }))
        .unwrap();
        assert_eq!(payload.expiration, 60);
        assert_eq!(payload.data[0].mid, "abc");
        assert_eq!(payload.data[0].purl, "song.mp3");
    }

    #[test]
    fn credentials_accept_numeric_json_fields() {
        let credential: QqCredential = serde_json::from_value(serde_json::json!({
            "musicid": 123,
            "musickey": "key",
            "expired_at": "42",
            "loginType": "1",
            "encryptUin": "encrypted",
            "musickeyCreateTime": 100,
            "keyExpiresIn": 200
        }))
        .expect("numeric credential fields should be accepted");
        assert_eq!(credential.musicid, "123");
        assert_eq!(credential.expired_at, 42);
        assert_eq!(credential.login_type, 1);
        assert_eq!(credential.encrypt_uin, "encrypted");
        assert_eq!(credential.musickey_create_time, 100);
        assert_eq!(credential.key_expires_in, 200);
    }

    #[test]
    fn qr_status_accepts_qqmusicapi_event_code_pairs() {
        let status: QqQrCodeStatus = serde_json::from_value(serde_json::json!({
            "event": [66, 408],
            "done": false,
            "login_type": "qq"
        }))
        .expect("QQMusicApi event pairs should be accepted");
        assert_eq!(status.event, vec![66, 408]);
    }

    #[test]
    fn qr_status_accepts_scalar_event_codes() {
        let status: QqQrCodeStatus = serde_json::from_value(serde_json::json!({
            "event": "67",
            "done": false,
            "login_type": "qq"
        }))
        .expect("normalized scalar event codes should be accepted");
        assert_eq!(status.event, vec![67]);
    }

    #[tokio::test]
    async fn search_uses_qqmusicapi_http_envelope() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let length = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..length]);
            assert!(request.starts_with("GET /search/search_by_type?"));
            assert!(request.contains("keyword=hello"));
            assert!(request.contains("search_type=0"));
            assert!(request.contains("num=10"));
            assert!(request.contains("page=1"));

            let body = r#"{"code":0,"msg":"ok","data":{"song":[{"mid":"mid","name":"Title"}]}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = QqMusicClient::new(&format!("http://{address}")).unwrap();
        let data = client.search_by_type("hello", 1, 10).await.unwrap();
        assert_eq!(data.songs().first().map(QqSong::stable_id).as_deref(), Some("mid"));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn refresh_credential_uses_web_api_get_route() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let length = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..length]);
            assert!(request.starts_with("GET /login/refresh_credential HTTP/1.1"));
            assert!(request.to_ascii_lowercase().contains("cookie: musicid=123; musickey=secret"));

            let body = r#"{"code":0,"msg":"ok","data":{"musicid":123,"musickey":"refreshed"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let mut client = QqMusicClient::new(&format!("http://{address}")).unwrap();
        client.set_credential(QqCredential { musicid: "123".into(), musickey: "secret".into(), ..Default::default() });
        let credential = client.refresh_credential().await.unwrap();
        assert_eq!(credential.musickey, "refreshed");
        server.join().unwrap();
    }

    #[tokio::test]
    async fn check_login_expired_decodes_qqmusicapi_boolean_contract() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let bodies = [
                r#"{"code":-1,"msg":"operation failed","data":null}"#,
                r#"{"code":0,"msg":"ok","data":null}"#,
                r#"{"code":0,"msg":"ok","data":false}"#,
            ];
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 4096];
                let length = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                assert!(request.starts_with("GET /login/check_expired HTTP/1.1"));
                assert!(request.to_ascii_lowercase().contains("cookie: musicid=123; musickey=secret"));

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let mut client = QqMusicClient::new(&format!("http://{address}")).unwrap();
        client.set_credential(QqCredential { musicid: "123".into(), musickey: "secret".into(), ..Default::default() });
        assert!(!client.check_login_expired().await.unwrap());
        assert!(client.check_login_expired().await.unwrap());
        assert!(!client.check_login_expired().await.unwrap());
        server.join().unwrap();
    }

    #[tokio::test]
    #[ignore = "requires GLACIER_QQMUSIC_API_BIN pointing to a built sidecar"]
    async fn bundled_sidecar_starts_serves_qr_code_and_stops() {
        assert!(std::env::var_os("GLACIER_QQMUSIC_API_BIN").is_some());

        let http = reqwest::Client::new();
        assert!(
            http.get("http://127.0.0.1:8080/").send().await.is_err(),
            "port 8080 must be free so the test starts its own sidecar"
        );
        let client = QqMusicClient::new("http://127.0.0.1:8080").unwrap();
        for login_type in ["qq", "wx"] {
            let qr = client.request_qrcode(login_type).await.unwrap();
            assert_eq!(qr.qr_type, login_type);
            assert!(!qr.identifier.is_empty());
            assert!(!qr.data.is_empty() || !qr.img.is_empty());
        }

        drop(client);
        for _ in 0..40 {
            if http.get("http://127.0.0.1:8080/").send().await.is_err() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("bundled sidecar was still running after the client was dropped");
    }
}
