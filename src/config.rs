use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    Auto,
    Portrait,
    Landscape,
}

impl Default for Orientation {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    Private,
    Media,
    Saf,
    AllFiles,
}

impl Default for StorageMode {
    fn default() -> Self {
        Self::Private
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Apk,
    Aab,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Apk => "apk",
            Self::Aab => "aab",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub app_name: String,
    pub package_id: String,
    pub version_name: String,
    pub version_code: u32,
    pub start_page: String,
    pub orientation: Orientation,
    pub fullscreen: bool,
    pub keep_screen_on: bool,
    pub open_external_links: bool,
    pub allow_cleartext_http: bool,
    pub storage: StorageMode,
    pub internet: bool,
    pub camera: bool,
    pub microphone: bool,
    pub location: bool,
    pub notifications: bool,
    pub file_api: bool,
    pub disable_splash: bool,
    pub status_bar_color: String,
    pub navigation_bar_color: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_name: "My App".into(),
            package_id: "com.example.myapp".into(),
            version_name: "1.0.0".into(),
            version_code: 1,
            start_page: "index.html".into(),
            orientation: Orientation::Auto,
            fullscreen: false,
            keep_screen_on: false,
            open_external_links: true,
            allow_cleartext_http: false,
            storage: StorageMode::Private,
            internet: false,
            camera: false,
            microphone: false,
            location: false,
            notifications: false,
            file_api: true,
            disable_splash: false,
            status_bar_color: "#202124".into(),
            navigation_bar_color: "#000000".into(),
        }
    }
}

