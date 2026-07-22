//! Android App Bundle（.aab）の生成。
//!
//! Gradleで事前ビルドした`template.aab`のマニフェスト（protobuf XML）を
//! 書き換え、HTML資産とアイコンを差し込み、JAR署名（v1）を付与する。

use crate::{
    bundle_proto, jarsign,
    package::{collect_web_files, icon_density_for, manifest_replacements, prepare_icons, BuildRequest},
    signing::{default_key_path, ensure_key, load_key},
    validate::validate_request,
};
use anyhow::{bail, Context, Result};
use rsa::{
    sha2::{Digest, Sha256},
    RsaPrivateKey,
};
use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};
use zip::{write::FileOptions, CompressionMethod, ZipArchive, ZipWriter};

const TEMPLATE_AAB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/template.aab"));

pub fn build_aab(request: &BuildRequest) -> Result<PathBuf> {
    validate_request(&request.config, &request.web_root)?;
    if let Some(icon) = &request.icon {
        if !icon.is_file() {
            bail!("アイコン画像が見つかりません");
        }
    }
    let files = collect_web_files(&request.web_root, &[&request.output_apk])?;
    let key_path = match &request.signing_key {
        Some(path) => path.clone(),
        None => default_key_path(&request.config.package_id)?,
    };
    ensure_key(&key_path, &request.config.app_name)?;
    let (certificate_der, private_key) = load_key(&key_path)?;

    if let Some(parent) = request.output_apk.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = request.output_apk.with_extension("aab.tmp");
    match write_signed_aab(&temp, request, &files, &certificate_der, private_key) {
        Ok(()) => {
            if request.output_apk.exists() {
                fs::remove_file(&request.output_apk)?;
            }
            fs::rename(&temp, &request.output_apk)?;
            Ok(request.output_apk.clone())
        }
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

fn write_signed_aab(
    path: &Path,
    request: &BuildRequest,
    web_files: &[(PathBuf, String)],
    certificate_der: &[u8],
    private_key: RsaPrivateKey,
) -> Result<()> {
    let cursor = Cursor::new(TEMPLATE_AAB);
    let mut template = ZipArchive::new(cursor).context("内蔵AABテンプレートが壊れています")?;
    let output = File::create(path)?;
    let mut writer = ZipWriter::new(output);
    let replacements = manifest_replacements(&request.config);
    let icon_data = prepare_icons(request.icon.as_deref())?;
    let mut replaced_icons = 0usize;
    let mut digests: Vec<(String, [u8; 32])> = Vec::new();

    for index in 0..template.len() {
        let mut entry = template.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        if should_replace(&name) || entry.is_dir() {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        if name == "base/manifest/AndroidManifest.xml" {
            data = bundle_proto::patch_manifest(
                &data,
                &replacements,
                request.config.version_code,
                request.config.allow_cleartext_http,
                request.config.disable_splash,
            )?;
        }
        // アイコンはテンプレート内の実エントリ（`-v4`等の修飾付きパスを含む）を
        // 名前で照合して中身だけ差し替える
        if let Some(icons) = &icon_data {
            if let Some(size) = icon_density_for(&name) {
                if let Some(bytes) = icons.get(&size) {
                    data = bytes.clone();
                    replaced_icons += 1;
                }
            }
        }
        write_tracked(&mut writer, &mut digests, &name, &data)?;
    }
    if icon_data.is_some() && replaced_icons == 0 {
        bail!("テンプレート内にicon_payload.pngが見つからず、アイコンを差し替えられません");
    }

    write_tracked(
        &mut writer,
        &mut digests,
        "base/assets/app.json",
        &serde_json::to_vec_pretty(&request.config)?,
    )?;
    for (source, relative) in web_files {
        let data = fs::read(source)
            .with_context(|| format!("HTMLファイルを読めません: {}", source.display()))?;
        write_tracked(
            &mut writer,
            &mut digests,
            &format!("base/assets/www/{relative}"),
            &data,
        )?;
    }

    let signature = jarsign::sign_entries(&digests, certificate_der, private_key)?;
    write_plain(&mut writer, "META-INF/MANIFEST.MF", &signature.manifest)?;
    write_plain(&mut writer, "META-INF/CERT.SF", &signature.signature_file)?;
    write_plain(&mut writer, "META-INF/CERT.RSA", &signature.pkcs7)?;
    writer.finish()?;
    Ok(())
}

fn should_replace(name: &str) -> bool {
    name.starts_with("META-INF/")
        || name.starts_with("base/assets/www/")
        || name == "base/assets/app.json"
}

fn write_tracked<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    digests: &mut Vec<(String, [u8; 32])>,
    name: &str,
    data: &[u8],
) -> Result<()> {
    write_plain(writer, name, data)?;
    digests.push((name.to_owned(), Sha256::digest(data).into()));
    Ok(())
}

fn write_plain<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    name: &str,
    data: &[u8],
) -> Result<()> {
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    writer.start_file(name, options)?;
    writer.write_all(data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppConfig, OutputFormat};

    #[test]
    fn builds_fixture_aab_with_jar_signature() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("fixture.aab");
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
            format: OutputFormat::Aab,
        };
        build_aab(&request).unwrap();
        let mut aab = ZipArchive::new(File::open(&output).unwrap()).unwrap();
        for name in [
            "base/manifest/AndroidManifest.xml",
            "base/assets/www/index.html",
            "base/assets/app.json",
            "META-INF/MANIFEST.MF",
            "META-INF/CERT.SF",
            "META-INF/CERT.RSA",
        ] {
            assert!(aab.by_name(name).is_ok(), "missing {name}");
        }
        let mut manifest = Vec::new();
        aab.by_name("base/manifest/AndroidManifest.xml")
            .unwrap()
            .read_to_end(&mut manifest)
            .unwrap();
        let values = bundle_proto::attribute_values(&manifest).unwrap();
        assert!(values.iter().any(|v| v == "com.example.html2apkfixture"));
        assert!(!values.iter().any(|v| v == "io.html2apk.generated"));
    }
}
