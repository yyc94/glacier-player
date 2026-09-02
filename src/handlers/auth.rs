// SPDX-License-Identifier: GPL-3.0-only

//! Authentication message handlers for Glacier Player.
//!
//! This module handles QR login, logout, and session restoration.

use cosmic::prelude::*;

use crate::auth::QrLoginRequest;
use crate::messages::Message;
use crate::qqmusic::QqLoginState;
use crate::state::{AppModel, ViewState};

// =============================================================================
// Task Helper Methods
// =============================================================================

impl AppModel {
    /// Attempt to restore a previous session from stored credentials
    pub(crate) fn restore_session(&self) -> Task<cosmic::Action<Message>> {
        let client = self.music_client.clone();
        let audio_quality = self.config.audio_quality;
        Task::perform(
            async move {
                let mut client = client.lock().await;
                // Apply configured audio quality before restoring session
                client.set_audio_quality(audio_quality).await;
                client.try_restore_session().await.map_err(|e| e.to_string())
            },
            |result| cosmic::Action::App(Message::SessionRestored(result)),
        )
    }

    /// Start the QQ Music QR login flow.
    pub(crate) fn start_login_flow(&self) -> Task<cosmic::Action<Message>> {
        let client = self.music_client.clone();
        let audio_quality = self.config.audio_quality;
        Task::perform(
            async move {
                let mut client = client.lock().await;
                // Apply configured audio quality before starting the login
                client.set_audio_quality(audio_quality).await;
                client.start_login().await.map_err(|e| e.to_string())
            },
            |result| cosmic::Action::App(Message::QrCodeReady(result)),
        )
    }
}

// =============================================================================
// Message Handlers
// =============================================================================

impl AppModel {
    /// Handle start login - requests a QQ Music QR code.
    pub fn handle_start_login(&mut self) -> Task<cosmic::Action<Message>> {
        self.error_message = None;
        self.is_loading = true;
        self.start_login_flow()
    }

    /// Cancel an active QR login and ignore any poll already in flight.
    pub fn handle_cancel_login(&mut self) -> Task<cosmic::Action<Message>> {
        self.qr_login_request = None;
        self.is_loading = false;
        self.view_state = ViewState::Login;
        let client = self.music_client.clone();
        Task::perform(
            async move {
                client.lock().await.cancel_qr_login();
            },
            |_| cosmic::Action::App(Message::Noop),
        )
    }

    /// Handle the QR image request completing.
    pub fn handle_qr_code_ready(&mut self, result: Result<QrLoginRequest, String>) -> Task<cosmic::Action<Message>> {
        self.is_loading = false;
        match result {
            Ok(request) => {
                self.qr_login_request = Some(request);
                self.view_state = ViewState::AwaitingQr;
                // QQ Music login is completed by scanning the displayed QR
                // code. Polling starts after the image has been rendered.
                Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    },
                    |_| cosmic::Action::App(Message::QqQrPoll),
                )
            }
            Err(e) => {
                tracing::error!("Login failed: {}", e);
                self.error_message = Some(format!("Login failed: {}", e));
                self.view_state = ViewState::Login;
                Task::none()
            }
        }
    }

    pub fn handle_qq_qr_poll(&self) -> Task<cosmic::Action<Message>> {
        if self.qr_login_request.is_none() {
            return Task::none();
        }
        let client = self.music_client.clone();
        Task::perform(
            async move {
                let mut client = client.lock().await;
                client.poll_qr_login().await.map_err(|e| e.to_string())
            },
            |result| cosmic::Action::App(Message::QqQrStatus(result)),
        )
    }

    pub fn handle_qq_qr_status(&mut self, result: Result<QqLoginState, String>) -> Task<cosmic::Action<Message>> {
        if self.qr_login_request.is_none() {
            return Task::none();
        }
        match result {
            Ok(QqLoginState::Done) => self.handle_login_complete(Ok(())),
            Ok(QqLoginState::Waiting | QqLoginState::Confirming) => Task::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                },
                |_| cosmic::Action::App(Message::QqQrPoll),
            ),
            Ok(QqLoginState::Expired) => {
                self.qr_login_request = None;
                self.view_state = ViewState::Login;
                self.error_message = Some("QQ Music QR code expired. Please try again.".into());
                Task::none()
            }
            Ok(QqLoginState::Refused) => {
                self.qr_login_request = None;
                self.view_state = ViewState::Login;
                self.error_message = Some("QQ Music login was refused.".into());
                Task::none()
            }
            Ok(QqLoginState::Failed) => {
                self.qr_login_request = None;
                self.view_state = ViewState::Login;
                self.error_message = Some("QQ Music returned an unknown login status. Please try again.".into());
                Task::none()
            }
            Err(error) => {
                tracing::warn!("QQ Music QR poll failed: {error}");
                Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    },
                    |_| cosmic::Action::App(Message::QqQrPoll),
                )
            }
        }
    }

    /// Handle the login flow completing
    pub fn handle_login_complete(&mut self, result: Result<(), String>) -> Task<cosmic::Action<Message>> {
        tracing::info!("LoginComplete received with result: {:?}", result.is_ok());
        self.is_loading = false;
        match result {
            Ok(()) => {
                tracing::info!("Login successful! Transitioning to Main view");
                self.qr_login_request = None;
                self.enter_main_view()
            }
            Err(e) => {
                // Stay on the login view so the user can request a fresh QR code.
                tracing::error!("Login failed: {}", e);
                self.error_message = Some("QQ Music login could not be completed. Please request a new QR code.".into());
                Task::none()
            }
        }
    }

    /// Transition to the main view after successful authentication.
    ///
    /// Restores cached API data for instant UI population, then kicks off
    /// background refreshes from the QQ Music API so content stays current.
    /// Used by both [`Self::handle_login_complete`] and
    /// [`Self::handle_session_restored`].
    fn enter_main_view(&mut self) -> Task<cosmic::Action<Message>> {
        self.view_state = ViewState::Main;

        let cache_task = self.restore_cached_api_data();

        Task::batch(vec![cache_task, self.load_playlists(), self.load_albums(), self.load_favorite_tracks()])
    }

    /// Populate the UI with the last-seen library from the cache database
    /// (playlists, albums, favorite tracks, mixes, profiles) so the user sees
    /// content instantly on startup. Reads run through the same view-cache path
    /// the navigation handlers use; the parallel network loads in
    /// [`Self::enter_main_view`] then refresh everything.
    fn restore_cached_api_data(&self) -> Task<cosmic::Action<Message>> {
        use crate::music::models::{Album, Playlist, Track};
        Task::batch([
            self.read_view_cache::<Vec<Playlist>, _>("library:playlists", |p| Message::PlaylistsLoaded(Ok(p))),
            self.read_view_cache::<Vec<Album>, _>("library:albums", |a| Message::AlbumsLoaded(Ok(a))),
            self.read_view_cache::<Vec<Track>, _>("favorites:tracks", |t| Message::FavoriteTracksLoaded(Ok(t))),
        ])
    }

    /// Handle session restored result
    pub fn handle_session_restored(&mut self, result: Result<bool, String>) -> Task<cosmic::Action<Message>> {
        self.is_loading = false;
        match result {
            Ok(true) => {
                self.error_message = None;
                self.enter_main_view()
            }
            Ok(false) => {
                self.view_state = ViewState::Login;
                Task::none()
            }
            Err(ref e)
                if self.error_message.is_none()
                    && (e.to_lowercase().contains("network") || e.to_lowercase().contains("http request failed")) =>
            {
                // First network failure — likely resuming from suspend / lid-open.
                // Schedule one more attempt so we cover slower reconnects.
                tracing::info!("Session restore hit a network error, scheduling retry in 5s");
                self.error_message = Some("Network unavailable, retrying\u{2026}".into());
                self.is_loading = true;
                let client = self.music_client.clone();
                let aq = self.config.audio_quality;
                Task::perform(
                    async move {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        let mut c = client.lock().await;
                        c.set_audio_quality(aq).await;
                        c.try_restore_session().await.map_err(|e| e.to_string())
                    },
                    |r| cosmic::Action::App(Message::SessionRestored(r)),
                )
            }
            Err(e) => {
                self.view_state = ViewState::Login;
                self.error_message = Some(e);
                Task::none()
            }
        }
    }

    /// Handle logout
    pub fn handle_logout(&self) -> Task<cosmic::Action<Message>> {
        let client = self.music_client.clone();
        Task::perform(
            async move {
                let mut client = client.lock().await;
                client.logout().await;
            },
            |_| cosmic::Action::App(Message::ShowMain),
        )
        .chain(Task::done(cosmic::Action::App(Message::StartLogin)))
    }
}
