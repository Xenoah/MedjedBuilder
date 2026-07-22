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

/// アイコンの密度キーワードと出力ピクセルサイズ。
pub(crate) const ICON_DENSITIES: &[(&str, u32)] = &[
    ("mdpi", 48),
    ("hdpi", 72),
    ("xhdpi", 96),
    ("xxhdpi", 144),
    ("xxxhdpi", 192),
];

/// テンプレート内エントリがアイコン差し替え対象なら出力サイズを返す。
/// aapt2はフォルダ名へ`-v4`等の修飾子を付けることがあるため、固定パスの
/// 完全一致ではなく「drawable-<密度>」ディレクトリ + `icon_payload.png` で照合する。
pub(crate) fn icon_density_for(name: &str) -> Option<u32> {
    let file = name.rsplit('/').next()?;
    if file != "icon_payload.png" {
        return None;
    }
    ICON_DENSITIES.iter().find_map(|(density, size)| {
        let dir = format!("drawable-{density}");
        let matched = name.split('/').any(|part| {
            part == dir || part.strip_prefix(dir.as_str()).is_some_and(|rest| rest.starts_with('-'))
        });
        if matched { Some(*size) } else { None }
    })
}

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
    let mut replaced_icons = 0usize;

    for index in 0..template.len() {
        let mut entry = template.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        if should_replace(&name) || entry.is_dir() {
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
                request.config.disable_splash,
            )?;
        }
        // アイコンはテンプレート内の実際のエントリ名を保ったまま中身だけ差し替える
        // （パスを決め打ちして追加すると、リソーステーブルが参照しない
        //  死にエントリになりアイコンが反映されない）
        if let Some(icons) = &icon_data {
            if let Some(size) = icon_density_for(&name) {
                if let Some(bytes) = icons.get(&size) {
                    data = bytes.clone();
                    replaced_icons += 1;
                }
            }
        }
        let compression = if name == "resources.arsc" || name.starts_with("lib/") {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        write_entry(&mut writer, &name, &data, compression, alignment_for(&name))?;
    }
    if icon_data.is_some() && replaced_icons == 0 {
        bail!("テンプレート内にicon_payload.pngが見つからず、アイコンを差し替えられません");
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

/// 指定画像を各密度サイズのPNGへ変換する（サイズ → PNGバイト列）。
pub(crate) fn prepare_icons(icon: Option<&Path>) -> Result<Option<BTreeMap<u32, Vec<u8>>>> {
    let Some(icon) = icon else {
        return Ok(None);
    };
    let source = image::open(icon).context("アイコン画像を開けません")?;
    let mut output = BTreeMap::new();
    for (_, size) in ICON_DENSITIES {
        let resized = source.resize_exact(*size, *size, FilterType::Lanczos3);
        let mut bytes = Cursor::new(Vec::new());
        resized.write_to(&mut bytes, ImageFormat::Png)?;
        output.insert(*size, bytes.into_inner());
    }
    Ok(Some(output))
}

fn should_replace(name: &str) -> bool {
    name.starts_with("META-INF/")
        || name.starts_with("assets/www/")
        || name == "assets/app.json"
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
    fn icon_density_matches_plain_and_versioned_paths() {
        assert_eq!(icon_density_for("res/drawable-mdpi/icon_payload.png"), Some(48));
        assert_eq!(icon_density_for("res/drawable-hdpi-v4/icon_payload.png"), Some(72));
        assert_eq!(icon_density_for("base/res/drawable-xhdpi-v4/icon_payload.png"), Some(96));
        assert_eq!(icon_density_for("res/drawable-xxhdpi-v4/icon_payload.png"), Some(144));
        assert_eq!(icon_density_for("res/drawable-xxxhdpi/icon_payload.png"), Some(192));
        assert_eq!(icon_density_for("res/Hu.png"), None);
        assert_eq!(icon_density_for("res/drawable-xxhdpi/other.png"), None);
        assert_eq!(icon_density_for("assets/www/drawable-mdpi/icon_payload.png"), Some(48));
    }

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
