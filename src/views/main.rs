// SPDX-License-Identifier: GPL-3.0-only

//! Main collection view for Glacier Player.
//!
//! This module renders the main view showing the user's collection categories:
//! Playlists, Albums, and Favorite Tracks.

use crate::fl;
use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, text};

use crate::messages::Message;
use crate::state::AppModel;
use crate::views::components::branded_text;

impl AppModel {
    /// Render the main collection view with category navigation.
    pub fn view_main(&self) -> Element<'_, Message> {
        let big_size: u16 = 18;

        // Title row: text on the left, buttons on the right.
        let mut title_row = widget::Row::new().push(branded_text(big_size)).spacing(6).align_y(Alignment::Center);

        if cfg!(debug_assertions) {
            title_row = title_row.push(text(fl!("debug-unoptimized")).size(12).class(cosmic::theme::Text::Custom(|_theme| {
                cosmic::iced::widget::text::Style {
                    color: Some(cosmic::iced::Color::from_rgb(0.9, 0.2, 0.2)),
                    selected_fill: cosmic::iced::Color::from_rgb(0.9, 0.2, 0.2),
                }
            })));
        }

        let header = widget::Row::new()
            .push(title_row)
            .push(widget::space::horizontal())
            .push(
                button::icon(widget::icon::from_name("system-search-symbolic"))
                    .tooltip(fl!("tooltip-search"))
                    .on_press(Message::ShowSearch)
                    .padding(4),
            )
            .push(
                button::icon(widget::icon::from_name("emblem-system-symbolic"))
                    .tooltip(fl!("tooltip-settings"))
                    .on_press(Message::ShowSettings)
                    .padding(4),
            )
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill);

        // Collection category items
        let playlists_count =
            if self.user_playlists.is_empty() { String::new() } else { format!(" ({})", self.user_playlists.len()) };

        let albums_count = if self.user_albums.is_empty() { String::new() } else { format!(" ({})", self.user_albums.len()) };

        let tracks_count =
            if self.user_favorite_tracks.is_empty() { String::new() } else { format!(" ({})", self.user_favorite_tracks.len()) };

        let playlists_btn = AppModel::menu_row(
            "folder-music-symbolic",
            format!("{}{}", fl!("playlists"), playlists_count),
            Message::ShowPlaylists,
        );

        let albums_btn =
            AppModel::menu_row("media-optical-symbolic", format!("{}{}", fl!("albums"), albums_count), Message::ShowAlbums);

        let tracks_btn = AppModel::menu_row(
            "emblem-favorite-symbolic",
            format!("{}{}", fl!("tracks"), tracks_count),
            Message::ShowFavoriteTracks,
        );

        let history_count = if self.play_history.is_empty() { String::new() } else { format!(" ({})", self.play_history.len()) };

        let history_btn = AppModel::menu_row(
            "document-open-recent-symbolic",
            format!("{}{}", fl!("history"), history_count),
            Message::ShowHistory,
        );

        let collection_section =
            widget::Column::new().push(albums_btn).push(history_btn).push(playlists_btn).push(tracks_btn).spacing(4);

        widget::Column::new().push(header).push(collection_section).spacing(8).padding(12).width(Length::Fill).into()
    }
}
