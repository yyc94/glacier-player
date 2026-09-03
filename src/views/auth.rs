// SPDX-License-Identifier: GPL-3.0-only

//! Authentication views for Glacier Player.
//!
//! This module contains the login and QR waiting views.

use base64::{Engine, engine::general_purpose};
use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, container, text};

use crate::auth::QrLoginProvider;
use crate::fl;
use crate::messages::Message;
use crate::state::AppModel;
use crate::views::components::branded_title;

impl AppModel {
    /// Render the login view prompting user to sign in.
    pub fn view_login(&self) -> Element<'_, Message> {
        let content = widget::Column::new()
            .push(branded_title(24))
            .push(text(fl!("sign-in-prompt")).size(14))
            .push(widget::space::vertical().height(20))
            .push(button::suggested("QQ Music").on_press(Message::StartLogin(QrLoginProvider::Qq)).width(Length::Fill))
            .push(button::standard("WeChat").on_press(Message::StartLogin(QrLoginProvider::WeChat)).width(Length::Fill))
            .spacing(12)
            .align_x(Alignment::Center)
            .padding(20)
            .width(Length::Fill);

        container(content).width(Length::Fill).align_x(Alignment::Center).align_y(Alignment::Center).into()
    }

    /// Render the view shown while QQ Music QR authentication is in progress.
    pub fn view_awaiting_qr(&self) -> Element<'_, Message> {
        let provider = self.qr_login_request.as_ref().map_or(QrLoginProvider::Qq, |request| request.provider);
        let qr_image = self.qr_login_request.as_ref().and_then(|request| qr_handle(&request.image_data_url));
        let content = if let Some(handle) = qr_image {
            let mut col = widget::Column::new()
                .push(text(format!("Sign in with {}", provider.display_name())).size(20))
                .push(text(format!("Scan this QR code with {}", provider.scanner_name())).size(12))
                .spacing(10)
                .align_x(Alignment::Center);
            col = col.push(widget::image(handle).width(Length::Fixed(220.0)).height(Length::Fixed(220.0)));
            col.push(text("Waiting for confirmation...").size(12))
                .push(widget::space::vertical().height(8))
                .push(button::text(fl!("cancel")).on_press(Message::CancelLogin))
        } else if self.is_loading {
            // Waiting for the QR request to complete
            widget::Column::new()
                .push(text(format!("Preparing {} login", provider.display_name())).size(20))
                .push(widget::space::vertical().height(20))
                .push(text("⏳").size(32))
                .push(widget::space::vertical().height(10))
                .push(text(fl!("verifying-auth")).size(14))
                .push(text(fl!("verifying-auth-wait")).size(12))
                .push(widget::space::vertical().height(20))
                .push(button::text(fl!("cancel")).on_press(Message::CancelLogin))
                .spacing(8)
                .align_x(Alignment::Center)
        } else if self.qr_login_request.is_some() {
            widget::Column::new()
                .push(text(fl!("sign-in-title")).size(20))
                .push(text("Preparing QR code...").size(12))
                .push(widget::space::vertical().height(10))
                .push(button::text(fl!("cancel")).on_press(Message::CancelLogin))
                .spacing(8)
                .align_x(Alignment::Center)
        } else {
            widget::Column::new().push(text(fl!("preparing-login")).size(16)).align_x(Alignment::Center)
        };

        container(content.padding(20)).width(Length::Fill).align_x(Alignment::Center).align_y(Alignment::Center).into()
    }

    /// Render a simple loading view.
    pub fn view_loading(&self) -> Element<'_, Message> {
        let content = widget::Column::new().push(text(fl!("loading")).size(16)).spacing(8).align_x(Alignment::Center);

        container(content).width(Length::Fill).align_x(Alignment::Center).align_y(Alignment::Center).padding(20).into()
    }
}

fn qr_handle(data_url: &str) -> Option<widget::image::Handle> {
    let encoded = data_url.split_once(",")?.1;
    let bytes = general_purpose::STANDARD.decode(encoded).ok()?;
    Some(widget::image::Handle::from_bytes(bytes))
}
