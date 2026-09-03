// SPDX-License-Identifier: GPL-3.0-only

//! Application-facing QQ Music client.
//!
//! This adapter translates QQMusicApi DTOs into provider-neutral music models
//! and exposes the operations used by the queue and navigation handlers.

use base64::{Engine, engine::general_purpose};

use crate::auth::{AuthState, QrLoginProvider, QrLoginRequest, UserProfile};
use crate::config::AudioQuality;
use crate::music::models::{
    Album, Artist, ExplorePage, FeedActivity, Mix, Playlist, SearchResults, Track, TrackCredits, TrackLyrics, parse_lrc,
};

use super::client::{QqCredential, QqMusicClient, QqMusicError, QqQrCodeStatus};

pub type MusicError = QqMusicError;
pub type MusicResult<T> = Result<T, MusicError>;

const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";
const CREDENTIAL_SERVICE: &str = "glacier-player";
const CREDENTIAL_ACCOUNT: &str = "qqmusic-credential";

/// A resolved QQ Music stream URL.
#[derive(Debug, Clone)]
pub enum PlaybackUrl {
    Direct(String, Option<f32>, Option<crate::music::models::StreamQuality>),
}

impl PlaybackUrl {
    pub fn as_url(&self) -> String {
        match self {
            Self::Direct(url, _, _) => url.clone(),
        }
    }

    pub fn replay_gain_db(&self) -> Option<f32> {
        match self {
            Self::Direct(_, gain, _) => *gain,
        }
    }

    pub fn stream_quality(&self) -> Option<crate::music::models::StreamQuality> {
        match self {
            Self::Direct(_, _, quality) => quality.clone(),
        }
    }
}

impl std::fmt::Display for PlaybackUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(url, _, _) => write!(f, "Direct({})", url.split('?').next().unwrap_or(url)),
        }
    }
}

/// Result of one QR polling operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QqLoginState {
    /// The code has not been scanned yet.
    Waiting,
    /// The code was scanned and is awaiting confirmation in QQ Music.
    Confirming,
    /// The QR code is no longer valid.
    Expired,
    /// The user refused the login.
    Refused,
    /// The service returned an unrecognized terminal state.
    Failed,
    /// Login completed and the credential was stored.
    Done,
}

/// High-level QQ Music client used by the application model.
pub struct QqMusicAppClient {
    api: QqMusicClient,
    auth_state: AuthState,
    qr_login_provider: Option<QrLoginProvider>,
    qr_identifier: Option<String>,
    audio_quality: AudioQuality,
}

impl QqMusicAppClient {
    /// Build an app client. Invalid configuration is deferred to the first
    /// request so startup can still render the settings view and show a useful
    /// error instead of panicking.
    pub fn new(base_url: &str) -> Self {
        let api = QqMusicClient::new(base_url).or_else(|_| QqMusicClient::new(DEFAULT_API_URL));
        let api = match api {
            Ok(api) => api,
            Err(error) => {
                tracing::error!("failed to construct QQMusicApi client: {error}");
                // Both constants are valid URLs. This branch is only here for
                // a future reqwest construction failure and cannot panic.
                QqMusicClient::new(DEFAULT_API_URL).unwrap_or_else(|_| unreachable!("default QQMusicApi URL is valid"))
            }
        };
        Self {
            api,
            auth_state: AuthState::NotAuthenticated,
            qr_login_provider: None,
            qr_identifier: None,
            audio_quality: AudioQuality::default(),
        }
    }

    pub fn auth_state(&self) -> &AuthState {
        &self.auth_state
    }

    /// Replace the QQMusicApi endpoint while preserving the current cookie.
    pub fn set_base_url(&mut self, base_url: &str) -> Result<(), QqMusicError> {
        let mut api = QqMusicClient::new(base_url)?;
        api.set_credential(self.api.credential().clone());
        self.api = api;
        Ok(())
    }

    pub fn base_url(&self) -> &reqwest::Url {
        self.api.base_url()
    }

    pub async fn set_audio_quality(&mut self, quality: AudioQuality) {
        self.audio_quality = quality;
    }

    /// Restore the QQ credential from the desktop keyring, if present.
    pub async fn try_restore_session(&mut self) -> MusicResult<bool> {
        let entry =
            keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT).map_err(|error| QqMusicError::Http(error.to_string()))?;
        let json = match entry.get_password() {
            Ok(json) => json,
            Err(keyring::Error::NoEntry) => return Ok(false),
            Err(error) => return Err(QqMusicError::Http(error.to_string())),
        };
        let credential: QqCredential =
            serde_json::from_str(&json).map_err(|error| QqMusicError::InvalidResponse(error.to_string()))?;
        if !credential.is_authenticated() {
            return Ok(false);
        }
        self.api.set_credential(credential.clone());
        let credential = if self.api.check_login_expired().await? {
            let refreshed = self.api.refresh_credential().await?;
            self.api.set_credential(refreshed.clone());
            self.persist_credential(&refreshed)?;
            refreshed
        } else {
            credential
        };
        self.set_authenticated_profile(&credential);
        Ok(true)
    }

    /// Request a QQ login QR code. The image and identifier stay in this
    /// client while the request result transitions the view into QR polling.
    pub async fn start_login(&mut self, provider: QrLoginProvider) -> MusicResult<QrLoginRequest> {
        let qr = self.api.request_qrcode(provider.api_value()).await?;
        self.qr_login_provider = Some(provider);
        self.qr_identifier = Some(qr.identifier.clone());
        Ok(QrLoginRequest { provider, image_data_url: normalize_qr_image(&qr.img, &qr.mimetype, &qr.data) })
    }

    pub fn qr_identifier(&self) -> Option<&str> {
        self.qr_identifier.as_deref()
    }

    /// Poll a QR login and update the app authentication state on success.
    pub async fn poll_qr_login(&mut self) -> MusicResult<QqLoginState> {
        let provider = self.qr_login_provider.ok_or(QqMusicError::InvalidResponse("QR login has not started".into()))?;
        let identifier = self.qr_identifier.clone().ok_or(QqMusicError::InvalidResponse("QR identifier is missing".into()))?;
        let status = self.api.check_qrcode(provider.api_value(), &identifier).await?;
        let state = qr_state(&status);
        if state == QqLoginState::Done {
            if let Some(credential) = status.credential {
                self.api.set_credential(credential.clone());
                self.persist_credential(&credential)?;
                self.set_authenticated_profile(&credential);
            } else {
                return Err(QqMusicError::InvalidResponse("QR login completed without credential".into()));
            }
            self.qr_login_provider = None;
            self.qr_identifier = None;
        } else if matches!(state, QqLoginState::Expired | QqLoginState::Refused | QqLoginState::Failed) {
            self.qr_login_provider = None;
            self.qr_identifier = None;
        }
        Ok(state)
    }

    /// Forget an in-progress QR login.
    pub fn cancel_qr_login(&mut self) {
        self.qr_login_provider = None;
        self.qr_identifier = None;
    }

    pub async fn logout(&mut self) {
        self.api.clear_credential();
        self.auth_state = AuthState::NotAuthenticated;
        if let Ok(entry) = keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT) {
            let _ = entry.delete_credential();
        }
    }

    pub async fn search(&self, query: &str, limit: u32) -> MusicResult<SearchResults> {
        let data = self.api.search_by_type(query, 1, limit).await?;
        Ok(SearchResults {
            tracks: data.songs().into_iter().take(limit as usize).map(track_from_qq).collect(),
            ..Default::default()
        })
    }

    pub async fn get_user_playlists(&self, limit: Option<u32>, offset: Option<u32>) -> MusicResult<Vec<Playlist>> {
        self.require_auth()?;
        let lists = self.api.user_created_songlists(&self.api.credential().musicid).await?;
        let mut lists = lists.into_songlists();
        if !self.api.credential().encrypt_uin.is_empty() {
            let favorites = self.api.user_favorite_songlists(&self.api.credential().encrypt_uin, 1, 200).await?;
            lists.extend(favorites.into_songlists());
        }
        let mut seen = std::collections::HashSet::new();
        Ok(lists
            .into_iter()
            .map(playlist_from_qq)
            .filter(|playlist| seen.insert(playlist.uuid.clone()))
            .skip(offset.unwrap_or(0) as usize)
            .take(limit.unwrap_or(u32::MAX) as usize)
            .collect())
    }

    pub async fn get_user_favorite_tracks(&self, limit: Option<u32>) -> MusicResult<Vec<Track>> {
        self.require_auth()?;
        let euin = self.encrypted_uin()?;
        let limit = limit.unwrap_or(200);
        let data = self.api.user_favorite_songs(euin, 1, limit).await?;
        let (_, songs) = data.into_parts();
        Ok(songs.into_iter().take(limit as usize).map(track_from_qq).collect())
    }

    pub async fn get_user_favorite_albums(&self, limit: Option<u32>) -> MusicResult<Vec<Album>> {
        self.require_auth()?;
        let euin = self.encrypted_uin()?;
        let limit = limit.unwrap_or(200);
        let data = self.api.user_favorite_albums(euin, 1, limit).await?;
        Ok(data.albums.into_iter().take(limit as usize).map(album_from_qq).collect())
    }

    pub async fn get_playlist_tracks(&self, id: &str, limit: Option<u32>, offset: Option<u32>) -> MusicResult<Vec<Track>> {
        let limit = limit.unwrap_or(200);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let data = self.api.playlist_detail(id, offset.unwrap_or(0) / limit + 1, limit).await?;
        let (_, songs) = data.into_parts();
        Ok(songs.into_iter().take(limit as usize).map(track_from_qq).collect())
    }

    pub async fn get_album_tracks(&self, id: &str, limit: Option<u32>, offset: Option<u32>) -> MusicResult<Vec<Track>> {
        let limit = limit.unwrap_or(200);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let data = self.api.album_songs(id, offset.unwrap_or(0) / limit + 1, limit).await?;
        Ok(data.song_list.into_iter().take(limit as usize).map(track_from_qq).collect())
    }

    pub async fn get_album_info(&self, id: &str) -> MusicResult<Album> {
        let data = self.api.album_detail(id).await?;
        Ok(album_from_detail(data.album, data.singers))
    }

    pub async fn get_album_review(&self, _id: &str) -> MusicResult<String> {
        Ok(String::new())
    }

    pub async fn get_track_by_id(&self, id: &str) -> MusicResult<Track> {
        let data = self.api.song_detail(id).await?;
        data.first_song().map(track_from_qq).ok_or_else(|| QqMusicError::InvalidResponse("song detail contained no song".into()))
    }

    pub async fn get_track_playback_url(&self, id: &str) -> MusicResult<PlaybackUrl> {
        let song = self
            .api
            .song_detail(id)
            .await?
            .first_song()
            .ok_or_else(|| QqMusicError::InvalidResponse("song detail contained no song".into()))?;
        let track = track_from_qq(song.clone());
        let file_type = self.audio_quality.qqmusic_file_type();
        let media_mid = song.media_mid_value();
        let song_type = (song.song_type != 0).then_some(song.song_type);
        let payload = self.api.song_url(&song.stable_id(), file_type, song_type, media_mid).await?;
        let url = payload.first_playable_url().ok_or_else(|| QqMusicError::Api {
            code: 104003,
            message: format!("no playable QQ Music source for {}", track.title),
        })?;
        Ok(PlaybackUrl::Direct(url, None, None))
    }

    pub async fn get_track_lyrics(&self, id: &str) -> MusicResult<TrackLyrics> {
        let data = self.api.lyrics(id).await?;
        let plain = decode_qq_text(&data.lyric);
        let trans = decode_qq_text(&data.trans);
        let lrc_lines = parse_lrc(&plain);
        Ok(TrackLyrics {
            provider: Some("QQ Music".into()),
            plain_text: Some(if trans.is_empty() { plain } else { format!("{plain}\n\n{trans}") }),
            lrc_lines,
            is_right_to_left: false,
        })
    }

    pub async fn get_track_credits(&self, _id: &str) -> MusicResult<TrackCredits> {
        Ok(TrackCredits::default())
    }

    pub async fn add_favorite_track(&self, id: &str) -> MusicResult<()> {
        let _ = id;
        Err(unsupported("favorite-track writes"))
    }

    pub async fn remove_favorite_track(&self, id: &str) -> MusicResult<()> {
        let _ = id;
        Err(unsupported("favorite-track writes"))
    }

    pub async fn add_favorite_album(&self, id: &str) -> MusicResult<()> {
        let _ = id;
        Err(unsupported("favorite-album writes"))
    }

    pub async fn remove_favorite_album(&self, id: &str) -> MusicResult<()> {
        let _ = id;
        Err(unsupported("favorite-album writes"))
    }

    pub async fn get_artist_info(&self, id: &str) -> MusicResult<Artist> {
        let data = self.api.singer_info(id).await?;
        let artist_id = data.singer.stable_id();
        let name = if data.singer.name.is_empty() { data.base_info.name.clone() } else { data.singer.name.clone() };
        let picture_url = if data.base_info.avatar.is_empty() {
            data.singer
                .picture_url()
                .or_else(|| (!data.base_info.background_image.is_empty()).then_some(data.base_info.background_image))
        } else {
            Some(data.base_info.avatar)
        };
        Ok(Artist {
            id: if artist_id.is_empty() { id.to_string() } else { artist_id },
            name,
            picture_url,
            url: Some(format!("https://y.qq.com/n/ryqq/singer/{id}")),
            ..Default::default()
        })
    }

    pub async fn get_artist_top_tracks(&self, id: &str, limit: Option<u32>) -> MusicResult<Vec<Track>> {
        let limit = limit.unwrap_or(20);
        let data = self.api.singer_songs(id, 1, limit).await?;
        Ok(data.song_list.into_iter().take(limit as usize).map(track_from_qq).collect())
    }

    pub async fn get_artist_albums(&self, id: &str, limit: Option<u32>) -> MusicResult<Vec<Album>> {
        let limit = limit.unwrap_or(50);
        let data = self.api.singer_albums(id, 1, limit).await?;
        Ok(data
            .album_list
            .into_iter()
            .take(limit as usize)
            .map(|album| {
                let mut album = album_from_qq(album);
                album.artist_id = Some(id.to_string());
                album
            })
            .collect())
    }

    pub async fn get_artist_videos(&self, _id: &str, _limit: Option<u32>) -> MusicResult<Vec<Track>> {
        Ok(Vec::new())
    }

    pub async fn follow_artist(&self, _id: &str) -> MusicResult<()> {
        Err(unsupported("artist follow writes"))
    }

    pub async fn unfollow_artist(&self, _id: &str) -> MusicResult<()> {
        Err(unsupported("artist follow writes"))
    }

    pub async fn get_followed_artists(&self) -> MusicResult<Vec<Artist>> {
        Ok(Vec::new())
    }

    pub async fn get_mixes(&self) -> MusicResult<Vec<Mix>> {
        Ok(Vec::new())
    }

    pub async fn get_mix_tracks(&self, _id: &str) -> MusicResult<Vec<Track>> {
        Ok(Vec::new())
    }

    pub async fn get_track_mix(&self, _id: &str) -> MusicResult<(String, Vec<Track>)> {
        Ok((String::new(), Vec::new()))
    }

    pub async fn get_similar_artists(&self, id: &str, limit: Option<u32>) -> MusicResult<Vec<Artist>> {
        let limit = limit.unwrap_or(20);
        let data = self.api.similar_singers(id, limit).await?;
        Ok(data.singerlist.into_iter().take(limit as usize).map(artist_from_qq).collect())
    }

    pub async fn get_feed(&self) -> MusicResult<Vec<FeedActivity>> {
        Ok(Vec::new())
    }

    pub async fn get_explore_page(&self, _path: &str) -> MusicResult<ExplorePage> {
        Err(QqMusicError::Api { code: -1, message: "Explore is not provided by QQMusicApi".into() })
    }

    pub async fn get_video_hls_url(&self, _id: &str) -> MusicResult<String> {
        Err(QqMusicError::Api { code: -1, message: "QQ Music videos are not supported".into() })
    }

    fn require_auth(&self) -> MusicResult<()> {
        if self.api.credential().is_authenticated() { Ok(()) } else { Err(QqMusicError::NotAuthenticated) }
    }

    fn encrypted_uin(&self) -> MusicResult<&str> {
        let value = self.api.credential().encrypt_uin.trim();
        if value.is_empty() {
            Err(QqMusicError::InvalidResponse("credential does not contain encrypt_uin".into()))
        } else {
            Ok(value)
        }
    }

    fn set_authenticated_profile(&mut self, credential: &QqCredential) {
        self.auth_state = AuthState::Authenticated {
            profile: UserProfile {
                username: Some(credential.musicid.clone()),
                nickname: Some(format!("QQ 音乐 {}", credential.musicid)),
                ..Default::default()
            },
        };
    }

    fn persist_credential(&self, credential: &QqCredential) -> MusicResult<()> {
        let json = serde_json::to_string(credential).map_err(|error| QqMusicError::InvalidResponse(error.to_string()))?;
        let entry =
            keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT).map_err(|error| QqMusicError::Http(error.to_string()))?;
        entry.set_password(&json).map_err(|error| QqMusicError::Http(error.to_string()))?;
        Ok(())
    }
}

fn qr_state(status: &QqQrCodeStatus) -> QqLoginState {
    if status.done && status.credential.is_some() {
        return QqLoginState::Done;
    }
    if event_matches(status, &[0, 405]) {
        QqLoginState::Done
    } else if event_matches(status, &[2, 67, 404]) {
        QqLoginState::Confirming
    } else if event_matches(status, &[3, 65, 402]) {
        QqLoginState::Expired
    } else if event_matches(status, &[4, 68, 403]) {
        QqLoginState::Refused
    } else if event_matches(status, &[1, 66, 408]) {
        QqLoginState::Waiting
    } else {
        QqLoginState::Failed
    }
}

fn event_matches(status: &QqQrCodeStatus, expected: &[i32]) -> bool {
    status.event.iter().any(|code| expected.contains(code))
}

fn normalize_qr_image(img: &str, mimetype: &str, data: &str) -> String {
    if img.starts_with("data:") {
        return img.to_string();
    }
    if !img.is_empty() {
        return format!("data:{mimetype};base64,{img}");
    }
    if data.starts_with("data:") { data.to_string() } else { format!("data:{mimetype};base64,{data}") }
}

fn decode_qq_text(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    general_purpose::STANDARD
        .decode(value)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_else(|| value.to_string())
}

fn track_from_qq(song: super::models::QqSong) -> Track {
    let singer = song.primary_singer();
    let id = song.stable_id();
    let artist_name = singer.map(|s| s.name.clone()).filter(|name| !name.is_empty()).unwrap_or_else(|| "Unknown Artist".into());
    let artist_id = singer.map(super::models::QqSinger::stable_id).filter(|id| !id.is_empty());
    let album = song.album.clone();
    let album_name = album
        .as_ref()
        .map(|a| a.name.clone())
        .filter(|name| !name.is_empty())
        .or_else(|| (!song.album_name.is_empty()).then_some(song.album_name.clone()));
    let album_id = album
        .as_ref()
        .map(|a| if !a.mid.is_empty() { a.mid.clone() } else { value_to_string(&a.id) })
        .filter(|id| !id.is_empty())
        .or_else(|| (!song.album_mid.is_empty()).then_some(song.album_mid.clone()));
    Track {
        id,
        title: song.display_title().to_string(),
        duration: song.duration,
        track_number: 0,
        artist_name,
        artist_id,
        album_name,
        album_id,
        cover_url: song.cover_url(),
        explicit: song.explicit,
        audio_quality: Some("QQ Music".into()),
        is_video: false,
    }
}

fn playlist_from_qq(list: super::models::QqSongList) -> Playlist {
    let id = value_to_string(&list.id);
    let creator_name = (!list.creator_name.is_empty())
        .then_some(list.creator_name.clone())
        .or_else(|| list.creator.as_ref().map(|creator| creator.nick.clone()).filter(|name| !name.is_empty()));
    Playlist {
        uuid: id,
        title: if list.name.is_empty() { "QQ Music Playlist".into() } else { list.name },
        description: (!list.description.is_empty())
            .then_some(list.description)
            .or_else(|| (!list.desc.is_empty()).then_some(list.desc)),
        creator_name,
        num_tracks: list.total_song_num.max(list.songnum),
        duration: 0,
        last_updated: None,
        image_url: (!list.image_url.is_empty()).then_some(list.image_url),
        is_user_playlist: true,
    }
}

fn album_from_qq(album: super::models::QqAlbum) -> Album {
    let id = if album.mid.is_empty() { value_to_string(&album.id) } else { album.mid.clone() };
    let title = if album.name.is_empty() { album.title.clone() } else { album.name.clone() };
    let release_date = (!album.time_public.is_empty()).then_some(album.time_public.clone());
    let cover_url = qq_album_cover(&album);
    let review = (!album.desc.is_empty()).then_some(album.desc.clone());
    let singer = album.singers.first();
    let artist_name = singer
        .map(|item| item.name.clone())
        .filter(|name| !name.is_empty())
        .or_else(|| (!album.singer_name.is_empty()).then_some(album.singer_name.clone()))
        .unwrap_or_else(|| "QQ Music".into());
    Album {
        id,
        title,
        artist_name,
        artist_id: singer.map(super::models::QqSinger::stable_id).filter(|id| !id.is_empty()),
        num_tracks: album.songnum,
        duration: 0,
        release_date,
        cover_url,
        explicit: false,
        audio_quality: Some("QQ Music".into()),
        review,
    }
}

fn album_from_detail(album: super::models::QqAlbum, singers: Vec<super::models::QqSinger>) -> Album {
    let mut result = album_from_qq(album);
    if let Some(singer) = singers.first() {
        result.artist_name = singer.name.clone();
        result.artist_id = Some(singer.stable_id());
    }
    result
}

fn artist_from_qq(singer: super::models::QqSinger) -> Artist {
    Artist {
        id: singer.stable_id(),
        name: singer.name.clone(),
        picture_url: singer.picture_url(),
        url: (!singer.mid.is_empty()).then(|| format!("https://y.qq.com/n/ryqq/singer/{}", singer.mid)),
        ..Default::default()
    }
}

fn unsupported(feature: &str) -> QqMusicError {
    QqMusicError::Api { code: -1, message: format!("QQMusicApi Web does not support {feature}") }
}

fn qq_album_cover(album: &super::models::QqAlbum) -> Option<String> {
    let mid = if album.pmid.is_empty() { album.mid.as_str() } else { album.pmid.as_str() };
    (!mid.is_empty()).then(|| format!("https://y.qq.com/music/photo_new/T002R300x300M000{mid}.jpg"))
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(event: Vec<i32>) -> QqQrCodeStatus {
        QqQrCodeStatus { event, done: false, credential: None, identifier: String::new(), login_type: "qq".into() }
    }

    #[test]
    fn qr_state_maps_native_qqmusicapi_codes() {
        assert_eq!(qr_state(&status(vec![66, 408])), QqLoginState::Waiting);
        assert_eq!(qr_state(&status(vec![67, 404])), QqLoginState::Confirming);
        assert_eq!(qr_state(&status(vec![65, 402])), QqLoginState::Expired);
        assert_eq!(qr_state(&status(vec![68, 403])), QqLoginState::Refused);
        assert_eq!(qr_state(&status(vec![0, 405])), QqLoginState::Done);
    }

    #[test]
    fn qr_state_maps_web_contract_codes() {
        assert_eq!(qr_state(&status(vec![0])), QqLoginState::Done);
        assert_eq!(qr_state(&status(vec![1])), QqLoginState::Waiting);
        assert_eq!(qr_state(&status(vec![2])), QqLoginState::Confirming);
        assert_eq!(qr_state(&status(vec![3])), QqLoginState::Expired);
        assert_eq!(qr_state(&status(vec![4])), QqLoginState::Refused);
    }

    #[test]
    fn qr_state_rejects_unknown_codes() {
        assert_eq!(qr_state(&status(vec![999])), QqLoginState::Failed);
        assert_eq!(qr_state(&status(Vec::new())), QqLoginState::Failed);
    }

    #[test]
    fn track_conversion_prefers_singer_mid_for_navigation() {
        let song: super::super::models::QqSong = serde_json::from_value(serde_json::json!({
            "mid": "song-mid",
            "name": "Title",
            "singer": [{ "id": 4558, "mid": "artist-mid", "name": "Artist" }]
        }))
        .unwrap();
        let track = track_from_qq(song);
        assert_eq!(track.artist_id.as_deref(), Some("artist-mid"));
    }

    #[test]
    fn album_conversion_accepts_singer_album_summary() {
        let album: super::super::models::QqAlbum = serde_json::from_value(serde_json::json!({
            "mid": "album-mid",
            "name": "Album",
            "singer_name": "Artist",
            "total_num": 12
        }))
        .unwrap();
        let album = album_from_qq(album);
        assert_eq!(album.artist_name, "Artist");
        assert_eq!(album.num_tracks, 12);
    }
}
