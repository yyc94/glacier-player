// SPDX-License-Identifier: GPL-3.0-only

//! HTTP client for the QQMusicApi Web service.

mod app_client;
mod client;
mod models;

pub use app_client::{MusicError, MusicResult, PlaybackUrl, QqLoginState, QqMusicAppClient};
pub use client::{
    ApiEnvelope, QqCredential, QqMusicClient, QqMusicError, QqQrCodeData, QqQrCodeStatus, QqResult, SongUrlItem, SongUrlPayload,
};
pub use models::{
    QqAlbum, QqLyricsData, QqPlaylistDetailData, QqSearchData, QqSinger, QqSong, QqSongDetailData, QqSongFile, QqSongList,
    QqSongListsData,
};
