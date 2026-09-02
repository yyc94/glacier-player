// SPDX-License-Identifier: GPL-3.0-only

//! Lenient DTOs for the QQMusicApi Web response shapes.
//!
//! QQMusicApi mirrors the upstream QQ Music JSON and therefore uses a few
//! different wrappers depending on the route (`body.song`, `song`, or a
//! playlist-specific object).  These DTOs intentionally model those wrappers
//! without coupling the rest of the application to `serde_json::Value`.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSinger {
    #[serde(default, alias = "singer_id")]
    pub id: Value,
    #[serde(default, alias = "singermid")]
    pub mid: String,
    #[serde(default, alias = "singer_name")]
    pub name: String,
    #[serde(default, alias = "singerPic")]
    pub singer_pic: String,
    #[serde(default, alias = "pic_mid", alias = "singer_pmid")]
    pub pmid: String,
}

impl QqSinger {
    pub fn stable_id(&self) -> String {
        if !self.mid.is_empty() { self.mid.clone() } else { value_as_string(&self.id) }
    }

    pub fn picture_url(&self) -> Option<String> {
        if !self.singer_pic.is_empty() {
            Some(self.singer_pic.clone())
        } else {
            let mid = if self.pmid.is_empty() { self.mid.as_str() } else { self.pmid.as_str() };
            (!mid.is_empty()).then(|| format!("https://y.qq.com/music/photo_new/T001R300x300M000{mid}.jpg"))
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqAlbum {
    #[serde(default, alias = "album_id")]
    pub id: Value,
    #[serde(default, alias = "albummid")]
    pub mid: String,
    #[serde(default, alias = "album_name")]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub time_public: String,
    #[serde(default)]
    pub pmid: String,
    #[serde(default, alias = "total_num")]
    pub songnum: u32,
    #[serde(default)]
    pub singers: Vec<QqSinger>,
    #[serde(default)]
    pub singer_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSongFile {
    #[serde(default, alias = "media_mid")]
    pub media_mid: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSong {
    #[serde(default)]
    pub id: Value,
    #[serde(default, alias = "songmid")]
    pub mid: String,
    #[serde(default)]
    pub name: String,
    /// Some QQ Music responses include both `name` and the legacy `songname`
    /// key. Keeping them as separate wire fields avoids Serde's duplicate
    /// alias error and lets the canonical `name` value win when present.
    #[serde(default)]
    pub songname: String,
    #[serde(default, alias = "song_title")]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default, alias = "interval")]
    pub duration: u32,
    #[serde(default, alias = "songtype", alias = "type")]
    pub song_type: i32,
    #[serde(default, alias = "media_mid")]
    pub media_mid: String,
    #[serde(default)]
    pub singer: Vec<QqSinger>,
    #[serde(default)]
    pub artists: Vec<QqSinger>,
    #[serde(default)]
    pub album: Option<QqAlbum>,
    #[serde(default, alias = "album_mid")]
    pub album_mid: String,
    #[serde(default, alias = "album_name")]
    pub album_name: String,
    #[serde(default)]
    pub pmid: String,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub file: Option<QqSongFile>,
}

impl QqSong {
    pub fn display_title(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else if !self.songname.is_empty() {
            &self.songname
        } else {
            &self.title
        }
    }

    pub fn primary_singer(&self) -> Option<&QqSinger> {
        self.singer.first().or_else(|| self.artists.first())
    }

    pub fn stable_id(&self) -> String {
        if !self.mid.is_empty() {
            return self.mid.clone();
        }
        value_as_string(&self.id)
    }

    pub fn cover_url(&self) -> Option<String> {
        let mid = self
            .album
            .as_ref()
            .map(|album| if !album.pmid.is_empty() { album.pmid.as_str() } else { album.mid.as_str() })
            .filter(|mid| !mid.is_empty())
            .or_else(|| (!self.pmid.is_empty()).then_some(self.pmid.as_str()))
            .or_else(|| (!self.album_mid.is_empty()).then_some(self.album_mid.as_str()))?;
        Some(format!("https://y.qq.com/music/photo_new/T002R300x300M000{mid}.jpg"))
    }

    pub fn media_mid_value(&self) -> Option<&str> {
        if !self.media_mid.is_empty() {
            Some(self.media_mid.as_str())
        } else {
            self.file.as_ref().map(|file| file.media_mid.as_str()).filter(|mid| !mid.is_empty())
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSongContainer {
    #[serde(default)]
    pub list: Vec<QqSong>,
    #[serde(default)]
    pub totalnum: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum QqSongResults {
    List(Vec<QqSong>),
    Container(QqSongContainer),
}

impl QqSongResults {
    fn into_songs(self) -> Vec<QqSong> {
        match self {
            Self::List(songs) => songs,
            Self::Container(container) => container.list,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSearchBody {
    #[serde(default)]
    pub song: Option<QqSongResults>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSearchData {
    #[serde(default)]
    pub body: Option<QqSearchBody>,
    #[serde(default)]
    pub song: Option<QqSongResults>,
}

impl QqSearchData {
    pub fn songs(self) -> Vec<QqSong> {
        self.body.and_then(|body| body.song).or(self.song).map(QqSongResults::into_songs).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSongDetailData {
    #[serde(default, alias = "track_info")]
    pub track: Option<QqSong>,
    #[serde(default)]
    pub songinfo: Option<QqSong>,
    #[serde(default)]
    pub info: Option<QqSong>,
    #[serde(default)]
    pub song: Option<QqSong>,
    #[serde(default)]
    pub songs: Vec<QqSong>,
    #[serde(default)]
    pub list: Vec<QqSong>,
}

impl QqSongDetailData {
    pub fn first_song(self) -> Option<QqSong> {
        self.track
            .or(self.songinfo)
            .or(self.info)
            .or(self.song)
            .or_else(|| self.songs.into_iter().next())
            .or_else(|| self.list.into_iter().next())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSonglistCreator {
    #[serde(default, alias = "nickname")]
    pub nick: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSongList {
    #[serde(default, alias = "dissid", alias = "disstid", alias = "songlist_id")]
    pub id: Value,
    #[serde(default, alias = "dissname", alias = "songlist_name", alias = "title")]
    pub name: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub songlist: Vec<QqSong>,
    #[serde(default)]
    pub songlist_songlist: Vec<QqSong>,
    #[serde(default)]
    pub songs: Vec<QqSong>,
    #[serde(default)]
    pub total_song_num: u32,
    #[serde(default, alias = "picurl", alias = "cover_url")]
    pub image_url: String,
    #[serde(default, alias = "creator_nick", alias = "nick", alias = "nickname")]
    pub creator_name: String,
    #[serde(default)]
    pub creator: Option<QqSonglistCreator>,
    #[serde(default)]
    pub songnum: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqAlbumDetailData {
    #[serde(default, alias = "basicInfo")]
    pub album: QqAlbum,
    #[serde(default)]
    pub singers: Vec<QqSinger>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqAlbumSongsData {
    #[serde(default, alias = "albumMid")]
    pub album_mid: String,
    #[serde(default, alias = "totalNum")]
    pub total_num: u32,
    #[serde(default, alias = "songList")]
    pub song_list: Vec<QqSong>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqFavoriteAlbumsData {
    #[serde(default, alias = "v_list")]
    pub albums: Vec<QqAlbum>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSingerBaseInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub background_image: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSingerInfoData {
    #[serde(default)]
    pub singer: QqSinger,
    #[serde(default)]
    pub base_info: QqSingerBaseInfo,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSingerSongsData {
    #[serde(default)]
    pub singer_mid: String,
    #[serde(default)]
    pub total_num: u32,
    #[serde(default)]
    pub song_list: Vec<QqSong>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSingerAlbumsData {
    #[serde(default)]
    pub singer_mid: String,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub album_list: Vec<QqAlbum>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSimilarSingersData {
    #[serde(default)]
    pub singerlist: Vec<QqSinger>,
}

impl QqSongList {
    pub fn songs(self) -> Vec<QqSong> {
        if !self.songlist.is_empty() {
            self.songlist
        } else if !self.songlist_songlist.is_empty() {
            self.songlist_songlist
        } else {
            self.songs
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqPlaylistDetailData {
    #[serde(default)]
    pub info: Option<QqSongList>,
    #[serde(default)]
    pub songlist: Option<QqSongList>,
    #[serde(default)]
    pub playlist: Option<QqSongList>,
    #[serde(default)]
    pub songs: Vec<QqSong>,
}

impl QqPlaylistDetailData {
    pub fn into_parts(self) -> (Option<QqSongList>, Vec<QqSong>) {
        let songs = if !self.songs.is_empty() {
            self.songs
        } else if let Some(list) = self.songlist.as_ref().or(self.playlist.as_ref()) {
            list.clone().songs()
        } else {
            Vec::new()
        };
        (self.info.or(self.songlist).or(self.playlist), songs)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqSongListsData {
    #[serde(default, alias = "songlists", alias = "list")]
    pub songlists: Vec<QqSongList>,
    #[serde(default, alias = "created_songlists")]
    pub created: Vec<QqSongList>,
    #[serde(default, alias = "playlists")]
    pub favorites: Vec<QqSongList>,
}

impl QqSongListsData {
    pub fn into_songlists(self) -> Vec<QqSongList> {
        let mut lists = if self.songlists.is_empty() { self.created } else { self.songlists };
        lists.extend(self.favorites);
        lists
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QqLyricsData {
    #[serde(default)]
    pub lyric: String,
    #[serde(default)]
    pub trans: String,
    #[serde(default)]
    pub roma: String,
    #[serde(default)]
    pub qrc: String,
}

fn value_as_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_data_accepts_body_song_shape() {
        let data: QqSearchData = serde_json::from_value(serde_json::json!({
            "body": { "song": { "list": [{ "songmid": "mid", "songname": "ignored", "name": "Title" }] } }
        }))
        .unwrap();
        assert_eq!(data.songs().first().map(QqSong::stable_id).as_deref(), Some("mid"));
    }

    #[test]
    fn search_data_accepts_web_song_array() {
        let data: QqSearchData = serde_json::from_value(serde_json::json!({
            "song": [{ "mid": "mid", "name": "Title" }]
        }))
        .unwrap();
        assert_eq!(data.songs().first().map(QqSong::stable_id).as_deref(), Some("mid"));
    }

    #[test]
    fn song_detail_accepts_web_track_field() {
        let data: QqSongDetailData = serde_json::from_value(serde_json::json!({
            "track": { "mid": "mid", "name": "Title" }
        }))
        .unwrap();
        assert_eq!(data.first_song().map(|song| song.stable_id()).as_deref(), Some("mid"));
    }

    #[test]
    fn playlist_detail_accepts_web_contract_shape() {
        let data: QqPlaylistDetailData = serde_json::from_value(serde_json::json!({
            "info": { "id": 42, "title": "List", "creator": { "nick": "Owner" } },
            "songs": [{ "mid": "mid", "name": "Title" }],
            "total": 1
        }))
        .unwrap();
        let (info, songs) = data.into_parts();
        assert_eq!(info.and_then(|item| item.creator).map(|creator| creator.nick).as_deref(), Some("Owner"));
        assert_eq!(songs.first().map(QqSong::stable_id).as_deref(), Some("mid"));
    }

    #[test]
    fn song_cover_uses_album_mid() {
        let song: QqSong =
            serde_json::from_value(serde_json::json!({ "mid": "song", "album": { "albummid": "album" } })).unwrap();
        assert_eq!(song.cover_url().as_deref(), Some("https://y.qq.com/music/photo_new/T002R300x300M000album.jpg"));
    }

    #[test]
    fn singer_payloads_accept_web_contract_shapes() {
        let info: QqSingerInfoData = serde_json::from_value(serde_json::json!({
            "singer": { "id": 4558, "mid": "artist-mid", "name": "Artist" },
            "base_info": { "name": "Artist", "avatar": "https://example.test/avatar.jpg" }
        }))
        .unwrap();
        assert_eq!(info.singer.stable_id(), "artist-mid");
        assert_eq!(info.base_info.avatar, "https://example.test/avatar.jpg");

        let albums: QqSingerAlbumsData = serde_json::from_value(serde_json::json!({
            "singer_mid": "artist-mid",
            "total": 1,
            "album_list": [{ "mid": "album-mid", "name": "Album", "total_num": 12, "singer_name": "Artist" }]
        }))
        .unwrap();
        assert_eq!(albums.album_list[0].songnum, 12);
        assert_eq!(albums.album_list[0].singer_name, "Artist");

        let similar: QqSimilarSingersData = serde_json::from_value(serde_json::json!({
            "singerlist": [{ "id": 1, "mid": "similar-mid", "name": "Similar", "singer_pic": "https://example.test/pic.jpg" }]
        }))
        .unwrap();
        assert_eq!(similar.singerlist[0].stable_id(), "similar-mid");
        assert_eq!(similar.singerlist[0].picture_url().as_deref(), Some("https://example.test/pic.jpg"));
    }
}
