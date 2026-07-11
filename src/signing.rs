use anyhow::{Context, Result};
use apksig::{Algorithms, Apk};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::rngs::OsRng;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_RSA_SHA256};
use rsa::{
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
    RsaPrivateKey,
};
use rustls_pki_types::PrivatePkcs8KeyDer;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
struct StoredKey {
    format: String,
    private_key_pkcs8: String,
    certificate_der: String,
}

pub fn default_key_path(package_id: &str) -> Result<PathBuf> {
    let root = dirs::document_dir()
        .or_else(dirs::config_dir)
        .context("ユーザーフォルダを取得できません")?
        .join("Html2Apk")
        .join("keys");
    Ok(root.join(format!("{package_id}.h2akey")))
}

pub fn ensure_key(path: &Path, app_name: &str) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let private_key = RsaPrivateKey::new(&mut OsRng, 3072)
        .context("RSA署名鍵を生成できません")?;
    let private_der = private_key.to_pkcs8_der()?.as_bytes().to_vec();
    let pki_key = PrivatePkcs8KeyDer::from(private_der.clone());
    let cert_key = KeyPair::from_pkcs8_der_and_sign_algo(&pki_key, &PKCS_RSA_SHA256)
        .context("証明書用の鍵を作成できません")?;

    let mut params = CertificateParams::new(Vec::<String>::new())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, app_name);
    dn.push(DnType::OrganizationName, "Html2Apk local signing");
    params.distinguished_name = dn;
    let certificate = params.self_signed(&cert_key)?;

    let stored = StoredKey {
        format: "html2apk-signing-key-v1".into(),
        private_key_pkcs8: STANDARD.encode(private_der),
        certificate_der: STANDARD.encode(certificate.der()),
    };
    let temporary = path.with_extension("h2akey.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(&stored)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

pub fn sign_apk(raw_apk: &Path, output_apk: &Path, key_path: &Path) -> Result<()> {
    let stored: StoredKey = serde_json::from_slice(
        &fs::read(key_path).with_context(|| format!("署名鍵を読めません: {}", key_path.display()))?,
    )?;
    if stored.format != "html2apk-signing-key-v1" {
        anyhow::bail!("対応していない署名鍵形式です");
    }
    let private_der = STANDARD.decode(stored.private_key_pkcs8)?;
    let cert_der = STANDARD.decode(stored.certificate_der)?;
    let private_key = RsaPrivateKey::from_pkcs8_der(&private_der)?;

    let mut apk = Apk::new_raw(raw_apk.to_path_buf())?;
    apk.sign_v2(
        &Algorithms::RSASSA_PKCS1_v1_5_256,
        &cert_der,
        private_key,
    )?;
    if let Some(parent) = output_apk.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = output_apk.with_extension("apk.tmp");
    let mut writer = BufWriter::new(File::create(&temp)?);
    apk.write_with_signature(&mut writer)?;
    writer.flush()?;
    drop(writer);
    Apk::new(temp.clone())?
        .verify()
        .map_err(anyhow::Error::msg)
        .context("生成したAPKの署名検証に失敗しました")?;
    if output_apk.exists() {
        fs::remove_file(output_apk)?;
    }
    fs::rename(temp, output_apk)?;
    Ok(())
}
