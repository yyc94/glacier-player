// SPDX-License-Identifier: GPL-3.0-only

//! Authentication state shared by the QQ Music QR login flow and the views.

/// A pending QQ Music QR login request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrLoginRequest {
    /// Data URL containing the QR image returned by QQMusicApi.
    pub image_data_url: String,
}

/// Authentication state
/// Profile information for the authenticated user
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserProfile {
    /// Provider username or account id
    pub username: Option<String>,
    /// User's first name
    pub first_name: Option<String>,
    /// User's last name
    pub last_name: Option<String>,
    /// Full display name supplied by the provider
    pub full_name: Option<String>,
    /// Nickname / display name chosen by the user
    pub nickname: Option<String>,
    /// Email address
    pub email: Option<String>,
    /// Profile picture URL
    pub picture_url: Option<String>,
    /// Subscription plan label, when available
    pub subscription_plan: Option<String>,
}

impl UserProfile {
    /// Best display name, checked in order:
    /// 1. "First Last" (if both non-empty)
    /// 2. full_name
    /// 3. nickname
    /// 4. first_name alone
    /// 5. username (if it doesn't look like an email)
    /// 6. email
    /// 7. "Signed in"
    pub fn display_name(&self) -> String {
        // Try "First Last"
        match (&self.first_name, &self.last_name) {
            (Some(f), Some(l)) if !f.is_empty() && !l.is_empty() => {
                return format!("{} {}", f, l);
            }
            _ => {}
        }
        // Try the provider's full display name
        if let Some(name) = &self.full_name
            && !name.is_empty()
        {
            return name.clone();
        }
        // Try nickname
        if let Some(nick) = &self.nickname
            && !nick.is_empty()
        {
            return nick.clone();
        }
        // Try first_name alone
        if let Some(f) = &self.first_name
            && !f.is_empty()
        {
            return f.clone();
        }
        // Fall back to username (skip if it looks like an email)
        if let Some(u) = &self.username
            && !u.is_empty()
            && !u.contains('@')
        {
            return u.clone();
        }
        // Fall back to email
        if let Some(e) = &self.email
            && !e.is_empty()
        {
            return e.clone();
        }
        "Signed in".to_string()
    }

    /// First letter of the display name (for avatar fallback)
    pub fn initials(&self) -> String {
        let name = self.display_name();
        name.chars().next().unwrap_or('?').to_uppercase().to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    /// Not authenticated, need to start login flow
    NotAuthenticated,
    /// Successfully authenticated
    Authenticated {
        /// Full user profile (name, email, picture, etc.)
        profile: UserProfile,
    },
    /// Authentication failed with error
    Failed(String),
}
