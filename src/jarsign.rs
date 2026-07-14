//! AAB用のJAR署名（v1署名）。
//!
//! `META-INF/MANIFEST.MF`・`META-INF/CERT.SF`・`META-INF/CERT.RSA`を生成する。
//! ダイジェストはSHA-256、署名はRSASSA-PKCS1-v1_5。CERT.RSAは証明書と
//! SignerInfoを含むPKCS#7 SignedData（DER）を自前で組み立てる。

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rsa::{
    pkcs1v15::SigningKey,
    sha2::{Digest, Sha256},
    signature::{SignatureEncoding, Signer},
    RsaPrivateKey,
};

pub struct JarSignature {
    pub manifest: Vec<u8>,
    pub signature_file: Vec<u8>,
    pub pkcs7: Vec<u8>,
}

/// zipへ書き込んだ各エントリの（名前, SHA-256）から署名一式を作る。
pub fn sign_entries(
    entries: &[(String, [u8; 32])],
    certificate_der: &[u8],
    private_key: RsaPrivateKey,
) -> Result<JarSignature> {
    let main_section = b"Manifest-Version: 1.0\r\nCreated-By: MedjedBuilder\r\n\r\n".to_vec();
    let mut manifest = main_section.clone();
    let mut sections = Vec::with_capacity(entries.len());
    for (name, digest) in entries {
        let mut section = Vec::new();
        section.extend_from_slice(&wrap_attribute("Name", name));
        section.extend_from_slice(&wrap_attribute("SHA-256-Digest", &STANDARD.encode(digest)));
        section.extend_from_slice(b"\r\n");
        manifest.extend_from_slice(&section);
        sections.push((name.clone(), section));
    }

    let mut signature_file = Vec::new();
    signature_file.extend_from_slice(b"Signature-Version: 1.0\r\nCreated-By: MedjedBuilder\r\n");
    signature_file.extend_from_slice(&wrap_attribute(
        "SHA-256-Digest-Manifest",
        &STANDARD.encode(Sha256::digest(&manifest)),
    ));
    signature_file.extend_from_slice(&wrap_attribute(
        "SHA-256-Digest-Manifest-Main-Attributes",
        &STANDARD.encode(Sha256::digest(&main_section)),
    ));
    signature_file.extend_from_slice(b"\r\n");
    for (name, section) in &sections {
        signature_file.extend_from_slice(&wrap_attribute("Name", name));
        signature_file.extend_from_slice(&wrap_attribute(
            "SHA-256-Digest",
            &STANDARD.encode(Sha256::digest(section)),
        ));
        signature_file.extend_from_slice(b"\r\n");
    }

    let signer = SigningKey::<Sha256>::new(private_key);
    let signature = signer.sign(&signature_file).to_vec();
    let pkcs7 = build_pkcs7(certificate_der, &signature)?;

    Ok(JarSignature {
        manifest,
        signature_file,
        pkcs7,
    })
}

/// `key: value`行をJAR仕様（1行最大72バイト、継続行は先頭スペース）で折り返す。
fn wrap_attribute(key: &str, value: &str) -> Vec<u8> {
    let line = format!("{key}: {value}");
    let mut out = Vec::with_capacity(line.len() + 8);
    let mut rest = line.as_str();
    let mut first = true;
    while !rest.is_empty() {
        let limit = if first { 70 } else { 69 };
        let mut cut = rest.len().min(limit);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        if !first {
            out.push(b' ');
        }
        out.extend_from_slice(rest[..cut].as_bytes());
        out.extend_from_slice(b"\r\n");
        rest = &rest[cut..];
        first = false;
    }
    out
}

const OID_SIGNED_DATA: &[u8] = &[0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];
const OID_DATA: &[u8] = &[0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];
const OID_SHA256: &[u8] = &[0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
const OID_RSA: &[u8] = &[0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
const DER_NULL: &[u8] = &[0x05, 0x00];

fn build_pkcs7(certificate_der: &[u8], signature: &[u8]) -> Result<Vec<u8>> {
    let (issuer, serial) = issuer_and_serial(certificate_der)?;
    let alg_sha256 = der(0x30, &[OID_SHA256, DER_NULL].concat());
    let alg_rsa = der(0x30, &[OID_RSA, DER_NULL].concat());
    let version = [0x02, 0x01, 0x01]; // INTEGER 1

    let signer_info = der(
        0x30,
        &[
            version.as_slice(),
            &der(0x30, &[issuer.as_slice(), serial.as_slice()].concat()),
            &alg_sha256,
            &alg_rsa,
            &der(0x04, signature),
        ]
        .concat(),
    );
    let signed_data = der(
        0x30,
        &[
            version.as_slice(),
            &der(0x31, &alg_sha256),      // digestAlgorithms SET
            &der(0x30, OID_DATA),         // encapContentInfo（内容なし）
            &der(0xA0, certificate_der),  // certificates [0] IMPLICIT
            &der(0x31, &signer_info),     // signerInfos SET
        ]
        .concat(),
    );
    Ok(der(
        0x30,
        &[OID_SIGNED_DATA, &der(0xA0, &signed_data)].concat(),
    ))
}

/// DER TLVを1つ組み立てる。
fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let significant = &bytes[bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1)..];
        out.push(0x80 | significant.len() as u8);
        out.extend_from_slice(significant);
    }
    out.extend_from_slice(content);
    out
}

/// 証明書DERからissuer Nameとserial NumberのTLVを取り出す。
fn issuer_and_serial(certificate: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let (tag, header, length) = read_tlv(certificate, 0)?;
    if tag != 0x30 {
        bail!("証明書DERが不正です");
    }
    let mut position = header;
    let end = header + length;

    // tbsCertificate
    let (tag, tbs_header, _) = read_tlv(certificate, position)?;
    if tag != 0x30 {
        bail!("tbsCertificateが不正です");
    }
    position += tbs_header;
    if position >= end {
        bail!("tbsCertificateが短すぎます");
    }

    // [0] version（省略可）
    let (tag, field_header, field_length) = read_tlv(certificate, position)?;
    if tag == 0xA0 {
        position += field_header + field_length;
    }

    // serialNumber INTEGER
    let (tag, field_header, field_length) = read_tlv(certificate, position)?;
    if tag != 0x02 {
        bail!("serialNumberが見つかりません");
    }
    let serial = certificate[position..position + field_header + field_length].to_vec();
    position += field_header + field_length;

    // signature AlgorithmIdentifier（読み飛ばし）
    let (_, field_header, field_length) = read_tlv(certificate, position)?;
    position += field_header + field_length;

    // issuer Name
    let (tag, field_header, field_length) = read_tlv(certificate, position)?;
    if tag != 0x30 {
        bail!("issuerが見つかりません");
    }
    let issuer = certificate[position..position + field_header + field_length].to_vec();
    Ok((issuer, serial))
}

/// (タグ, ヘッダー長, 内容長)を返す最小限のDERリーダー。
fn read_tlv(bytes: &[u8], offset: usize) -> Result<(u8, usize, usize)> {
    let tag = *bytes.get(offset).context("DERが短すぎます")?;
    let first = *bytes.get(offset + 1).context("DER長がありません")?;
    if first < 0x80 {
        return Ok((tag, 2, first as usize));
    }
    let count = (first & 0x7F) as usize;
    if count == 0 || count > 8 {
        bail!("DER長の形式が不正です");
    }
    let mut length = 0usize;
    for i in 0..count {
        length = (length << 8)
            | *bytes.get(offset + 2 + i).context("DER長が切れています")? as usize;
    }
    Ok((tag, 2 + count, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_lines_at_70_bytes() {
        let long_name = format!("base/assets/www/{}.js", "a".repeat(100));
        let wrapped = wrap_attribute("Name", &long_name);
        let text = String::from_utf8(wrapped).unwrap();
        for line in text.split("\r\n") {
            assert!(line.len() <= 70, "line too long: {}", line.len());
        }
        let rejoined: String = text
            .split("\r\n")
            .enumerate()
            .map(|(i, line)| if i == 0 { line } else { line.strip_prefix(' ').unwrap_or(line) })
            .collect();
        assert_eq!(rejoined, format!("Name: {long_name}"));
    }

    #[test]
    fn der_length_encodings() {
        assert_eq!(der(0x04, &[0u8; 3])[..2], [0x04, 0x03]);
        assert_eq!(der(0x04, &vec![0u8; 200])[..3], [0x04, 0x81, 200]);
        assert_eq!(der(0x04, &vec![0u8; 500])[..4], [0x04, 0x82, 0x01, 0xF4]);
    }
}
