use crate::{
    axml::patch_manifest,
    signing::{default_key_path, ensure_key, sign_apk},
    validate::validate_request,
    AppConfig, OutputFormat, StorageMode,
};
use anyhow::{bail, Context, Result};
use image::{imageops::FilterType, ImageFormat};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;
use zip::{write::FileOptions, CompressionMethod, ZipArchive, ZipWriter};

const TEMPLATE_APK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/template.apk"));
const MAX_WEB_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(crate) const ICON_PATHS: &[(&str, u32)] = &[
    ("res/drawable-mdpi/icon_payload.png", 48),
    ("res/drawable-hdpi/icon_payload.png", 72),
    ("res/drawable-xhdpi/icon_payload.png", 96),
    ("res/drawable-xxhdpi/icon_payload.png", 144),
    ("res/drawable-xxxhdpi/icon_payload.png", 192),
];

#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub web_root: PathBuf,
    pub output_apk: PathBuf,
    pub icon: Option<PathBuf>,
    pub signing_key: Option<PathBuf>,
    pub config: AppConfig,
    pub format: OutputFormat,
}

/// 出力形式に応じてAPKまたはAABを生成する。
pub fn build(request: &BuildRequest) -> Result<PathBuf> {
    match request.format {
        OutputFormat::Apk => build_apk(request),
        OutputFormat::Aab => crate::aab::build_aab(request),
    }
}

pub fn build_apk(request: &BuildRequest) -> Result<PathBuf> {
    validate_request(&request.config, &request.web_root)?;
    if let Some(icon) = &request.icon {
        if !icon.is_file() {
            bail!("アイコン画像が見つかりません");
        }
    }
    let unsigned = request.output_apk.with_extension("unsigned.apk");
    let files = collect_web_files(&request.web_root, &[&request.output_apk, &unsigned])?;
    if let Some(parent) = unsigned.parent() {
        fs::create_dir_all(parent)?;
    }
    write_unsigned_apk(&unsigned, request, &files)?;

    let key_path = match &request.signing_key {
        Some(path) => path.clone(),
        None => default_key_path(&request.config.package_id)?,
    };
    ensure_key(&key_path, &request.config.app_name)?;
    let sign_result = sign_apk(&unsigned, &request.output_apk, &key_path);
    let _ = fs::remove_file(&unsigned);
    sign_result?;
    Ok(request.output_apk.clone())
}

fn write_unsigned_apk(
    path: &Path,
    request: &BuildRequest,
    web_files: &[(PathBuf, String)],
) -> Result<()> {
    let cursor = Cursor::new(TEMPLATE_APK);
    let mut template = ZipArchive::new(cursor).context("内蔵APKテンプレートが壊れています")?;
    let output = File::create(path)?;
    let mut writer = ZipWriter::new(output);
    let replacements = manifest_replacements(&request.config);
    let icon_data = prepare_icons(request.icon.as_deref())?;

    for index in 0..template.len() {
        let mut entry = template.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        if should_replace(&name, request.icon.is_some()) || entry.is_dir() {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        if name == "AndroidManifest.xml" {
            data = patch_manifest(
                &data,
                &replacements,
                request.config.version_code,
                request.config.allow_cleartext_http,
            )?;
        }
        let compression = if name == "resources.arsc" || name.starts_with("lib/") {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        write_entry(&mut writer, &name, &data, compression, alignment_for(&name))?;
    }

    write_entry(
        &mut writer,
        "assets/app.json",
        &serde_json::to_vec_pretty(&request.config)?,
        CompressionMethod::Deflated,
        1,
    )?;
    for (source, relative) in web_files {
        let data = fs::read(source)
            .with_context(|| format!("HTMLファイルを読めません: {}", source.display()))?;
        write_entry(
            &mut writer,
            &format!("assets/www/{relative}"),
            &data,
            CompressionMethod::Deflated,
            1,
        )?;
    }
    for (name, data) in icon_data {
        write_entry(&mut writer, &name, &data, CompressionMethod::Stored, 4)?;
    }
    writer.finish()?;
    Ok(())
}

fn write_entry<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    name: &str,
    data: &[u8],
    compression: CompressionMethod,
    alignment: u64,
) -> Result<()> {
    let options = FileOptions::default()
        .compression_method(compression)
        .unix_permissions(0o644);
    if alignment <= 1 || compression != CompressionMethod::Stored {
        writer.start_file(name, options)?;
    } else {
        let preliminary = writer.start_file_with_extra_data(name, options)?;
        let padding = (alignment - ((preliminary + 4) % alignment)) % alignment;
        writer.write_all(&0x4841u16.to_le_bytes())?;
        writer.write_all(&(padding as u16).to_le_bytes())?;
        writer.write_all(&vec![0u8; padding as usize])?;
        writer.end_extra_data()?;
    }
    writer.write_all(data)?;
    Ok(())
}

pub(crate) fn collect_web_files(
    root: &Path,
    excluded: &[&PathBuf],
) -> Result<Vec<(PathBuf, String)>> {
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            bail!("シンボリックリンクは取り込めません: {}", entry.path().display());
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if excluded.iter().any(|path| entry.path() == path.as_path()) {
            continue;
        }
        total = total.saturating_add(entry.metadata()?.len());
        if total > MAX_WEB_BYTES {
            bail!("HTMLフォルダは合計2GB以下にしてください");
        }
        let relative = entry.path().strip_prefix(root)?;
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(value) => parts.push(value.to_string_lossy()),
                _ => bail!("安全でないファイルパスがあります"),
            }
        }
        let archive_name = parts.join("/");
        if archive_name.is_empty() || archive_name.split('/').any(|part| part == ".git") {
            continue;
        }
        files.push((entry.path().to_path_buf(), archive_name));
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(files)
}

pub(crate) fn prepare_icons(icon: Option<&Path>) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut output = BTreeMap::new();
    let Some(icon) = icon else {
        return Ok(output);
    };
    let source = image::open(icon).context("アイコン画像を開けません")?;
    for (path, size) in ICON_PATHS {
        let resized = source.resize_exact(*size, *size, FilterType::Lanczos3);
        let mut bytes = Cursor::new(Vec::new());
        resized.write_to(&mut bytes, ImageFormat::Png)?;
        output.insert((*path).to_string(), bytes.into_inner());
    }
    Ok(output)
}

fn should_replace(name: &str, replacing_icon: bool) -> bool {
    name.starts_with("META-INF/")
        || name.starts_with("assets/www/")
        || name == "assets/app.json"
        || replacing_icon && ICON_PATHS.iter().any(|(icon, _)| *icon == name)
}

fn alignment_for(name: &str) -> u64 {
    if name == "resources.arsc" || name.starts_with("lib/") {
        4
    } else {
        1
    }
}

pub(crate) fn manifest_replacements(config: &AppConfig) -> HashMap<String, String> {
    let mut values = HashMap::from([
        ("io.html2apk.generated".into(), config.package_id.clone()),
        ("__H2A_APP_NAME__".into(), config.app_name.clone()),
        ("__H2A_VERSION_NAME__".into(), config.version_name.clone()),
    ]);
    let permissions = [
        ("android.permission.INTERNET", config.internet),
        ("android.permission.CAMERA", config.camera),
        ("android.permission.RECORD_AUDIO", config.microphone),
        ("android.permission.ACCESS_FINE_LOCATION", config.location),
        ("android.permission.POST_NOTIFICATIONS", config.notifications),
        (
            "android.permission.READ_MEDIA_AUDIO",
            matches!(config.storage, StorageMode::Media | StorageMode::AllFiles),
        ),
        (
            "android.permission.READ_MEDIA_IMAGES",
            matches!(config.storage, StorageMode::Media | StorageMode::AllFiles),
        ),
        (
            "android.permission.READ_MEDIA_VIDEO",
            matches!(config.storage, StorageMode::Media | StorageMode::AllFiles),
        ),
        (
            "android.permission.READ_EXTERNAL_STORAGE",
            matches!(config.storage, StorageMode::Media | StorageMode::AllFiles),
        ),
        (
            "android.permission.WRITE_EXTERNAL_STORAGE",
            matches!(config.storage, StorageMode::AllFiles),
        ),
        (
            "android.permission.MANAGE_EXTERNAL_STORAGE",
            matches!(config.storage, StorageMode::AllFiles),
        ),
    ];
    for (name, enabled) in permissions {
        if !enabled {
            values.insert(name.into(), format!("io.html2apk.disabled.{}", name.rsplit('.').next().unwrap()));
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_verifies_fixture_apk() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("fixture.apk");
        let key = temp.path().join("fixture.h2akey");
        let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/web");
        let mut config = AppConfig::default();
        config.app_name = "検証アプリ".into();
        config.package_id = "com.example.html2apkfixture".into();
        let request = BuildRequest {
            web_root: web,
            output_apk: output.clone(),
            icon: None,
            signing_key: Some(key),
            config,
            format: OutputFormat::Apk,
        };
        build_apk(&request).unwrap();
        assert!(output.is_file());
        let mut apk = ZipArchive::new(File::open(&output).unwrap()).unwrap();
        assert!(apk.by_name("assets/www/index.html").is_ok());
        assert!(apk.by_name("assets/app.json").is_ok());
    }
}
