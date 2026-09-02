// SPDX-License-Identifier: GPL-3.0-only

//! Persisted configuration schema for Glacier Player.
//!
//! Settings are stored via COSMIC's config system and survive restarts.
//! The [`Config`] struct is the single source of truth for user preferences
//! such as audio quality, cache limits, and notification toggles.

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// Audio quality settings for music playback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioQuality {
    /// Low quality (128 kbps MP3)
    Low,
    /// High quality (320 kbps MP3)
    High,
    /// Lossless quality (FLAC 16-bit/44.1kHz)
    Lossless,
    /// Hi-Res quality (FLAC up to 24-bit/192kHz)
    #[default]
    HiRes,
}

impl AudioQuality {
    /// Label for the Settings dropdown and now-playing quality badge.
    ///
    /// Deliberately the same vocabulary the now-playing badge uses for the
    /// *served* stream, so the requested and delivered quality stay comparable.
    ///
    /// The explanatory text underneath the dropdown remains localized.
    pub fn display_name(&self) -> &'static str {
        match self {
            AudioQuality::Low => "Low — 128 kbps MP3",
            AudioQuality::High => "High — 320 kbps MP3",
            AudioQuality::Lossless => "Lossless — 16-bit, 44.1 kHz",
            AudioQuality::HiRes => "Hi-Res Lossless — up to 24-bit, 192 kHz",
        }
    }

    /// Stable integer used by the QQMusicApi Web file-type mapping.
    pub fn qqmusic_file_type(self) -> u8 {
        match self {
            AudioQuality::Low => 13,
            AudioQuality::High => 12,
            AudioQuality::Lossless => 7,
            AudioQuality::HiRes => 1,
        }
    }
}

impl AsRef<str> for AudioQuality {
    fn as_ref(&self) -> &str {
        self.display_name()
    }
}

/// Console/journal log verbosity.
///
/// Controls the base level of the terminal (journal) log layer only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    /// Only errors.
    Error,
    /// Warnings and errors.
    Warn,
    /// Informational messages and above (the default).
    #[default]
    Info,
    /// Debug messages and above.
    Debug,
    /// Everything, including trace-level spans.
    Trace,
}

impl LogLevel {
    /// The `EnvFilter` base directive string for this level.
    pub fn as_filter_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }

    /// Human-readable label for the settings dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            LogLevel::Error => "Error",
            LogLevel::Warn => "Warning",
            LogLevel::Info => "Info (default)",
            LogLevel::Debug => "Debug",
            LogLevel::Trace => "Trace",
        }
    }
}

impl AsRef<str> for LogLevel {
    fn as_ref(&self) -> &str {
        self.display_name()
    }
}

/// Configuration for Glacier Player.
#[derive(Debug, Clone, CosmicConfigEntry, PartialEq)]
#[version = 3]
pub struct Config {
    /// Base URL of the QQMusicApi Web service.
    pub qqmusic_api_url: String,
    /// Preferred audio quality for playback
    pub audio_quality: AudioQuality,
    /// Maximum image cache size in megabytes
    pub image_cache_max_mb: u32,
    /// Console/journal log verbosity
    pub log_level: LogLevel,
    /// Volume level (0.0 to 1.0), persisted across restarts
    pub volume_level: f32,
    /// Fixed loudness pre-amp applied to music **videos**, in decibels.
    ///
    /// The service may provide replay-gain for audio tracks but **not** for
    /// videos, so videos get a fixed pre-amp instead.
    /// -7..-11 dB, so the -8 dB default brings videos roughly in line with
    /// normalized tracks.
    pub video_preamp_db: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            qqmusic_api_url: "http://127.0.0.1:8080".to_string(),
            audio_quality: AudioQuality::HiRes,
            image_cache_max_mb: 200,
            log_level: LogLevel::Info,
            volume_level: 1.0,
            video_preamp_db: -8.0,
        }
    }
}
