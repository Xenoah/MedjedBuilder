use crate::AppConfig;
use anyhow::{bail, Context, Result};
use std::path::{Component, Path};

pub fn validate_request(config: &AppConfig, web_root: &Path) -> Result<()> {
    if !web_root.is_dir() {
        bail!("HTMLフォルダが存在しません");
    }
    validate_package_id(&config.package_id)?;
    if config.app_name.trim().is_empty() {
        bail!("アプリ名を入力してください");
    }
    if config.app_name.chars().count() > 100 || config.app_name.chars().any(char::is_control) {
        bail!("アプリ名は制御文字を含まない100文字以下にしてください");
    }
    if config.version_name.trim().is_empty()
        || config.version_name.chars().count() > 100
        || config.version_name.chars().any(char::is_control)
    {
        bail!("バージョン名は制御文字を含まない100文字以下にしてください");
    }
    if config.version_code == 0 {
        bail!("versionCodeは1以上にしてください");
    }
    validate_relative_path(&config.start_page)?;
    let start = web_root.join(&config.start_page);
    if !start.is_file() {
        bail!("開始ページが見つかりません: {}", start.display());
    }
    validate_color(&config.status_bar_color).context("ステータスバー色")?;
    validate_color(&config.navigation_bar_color).context("ナビゲーションバー色")?;
    Ok(())
}

pub fn validate_package_id(value: &str) -> Result<()> {
    if value.len() > 255 {
        bail!("パッケージIDは255文字以下にしてください");
    }
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() < 2 {
        bail!("パッケージIDは com.example.app の形式にしてください");
    }
    for part in parts {
        let mut chars = part.chars();
        let first = chars.next().context("空のパッケージID要素があります")?;
        if !(first.is_ascii_alphabetic() || first == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            bail!("パッケージIDに使用できない文字があります");
        }
    }
    Ok(())
}

pub fn validate_relative_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        bail!("開始ページはHTMLフォルダ内の相対パスにしてください");
    }
    Ok(())
}

fn validate_color(value: &str) -> Result<()> {
    let hex = value.strip_prefix('#').context("#RRGGBB形式で入力してください")?;
    if hex.len() != 6 && hex.len() != 8 {
        bail!("#RRGGBBまたは#AARRGGBB形式で入力してください");
    }
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("16進数以外の文字があります");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_id_rules() {
        assert!(validate_package_id("com.example.app").is_ok());
        assert!(validate_package_id("one").is_err());
        assert!(validate_package_id("com.bad-name.app").is_err());
    }

    #[test]
    fn relative_path_rules() {
        assert!(validate_relative_path("pages/index.html").is_ok());
        assert!(validate_relative_path("../secret").is_err());
    }
}
