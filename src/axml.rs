use anyhow::{bail, Context, Result};
use std::collections::HashMap;

const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const UTF8_FLAG: u32 = 0x0000_0100;

#[derive(Debug)]
struct StringPool {
    offset: usize,
    chunk_size: usize,
    header_size: usize,
    string_count: usize,
    style_count: usize,
    flags: u32,
    strings_start: usize,
    styles_start: usize,
    style_offsets: Vec<u32>,
    style_data: Vec<u8>,
    strings: Vec<String>,
}

pub fn patch_manifest(
    manifest: &[u8],
    replacements: &HashMap<String, String>,
    version_code: u32,
    allow_cleartext_http: bool,
) -> Result<Vec<u8>> {
    if manifest.len() < 8 {
        bail!("AndroidManifest.xmlが短すぎます");
    }
    let pool = parse_string_pool(manifest)?;
    let version_code_index = pool.strings.iter().position(|s| s == "versionCode");
    let cleartext_index = pool
        .strings
        .iter()
        .position(|s| s == "usesCleartextTraffic");

    let mut strings = pool.strings.clone();
    for value in &mut strings {
        if let Some(replacement) = replacements.get(value) {
            *value = replacement.clone();
        }
    }

    let replacement_pool = encode_string_pool(&pool, &strings)?;
    let mut out = Vec::with_capacity(
        manifest.len() - pool.chunk_size + replacement_pool.len(),
    );
    out.extend_from_slice(&manifest[..pool.offset]);
    out.extend_from_slice(&replacement_pool);
    out.extend_from_slice(&manifest[pool.offset + pool.chunk_size..]);
    let total_size = out.len() as u32;
    put_u32(&mut out, 4, total_size)?;

    if let Some(index) = version_code_index {
        patch_typed_attribute(&mut out, index as u32, version_code)?;
    }
    if let Some(index) = cleartext_index {
        patch_typed_attribute(&mut out, index as u32, u32::from(allow_cleartext_http))?;
    }
    Ok(out)
}

fn parse_string_pool(bytes: &[u8]) -> Result<StringPool> {
    let mut offset = 8usize;
    while offset + 8 <= bytes.len() {
        let kind = get_u16(bytes, offset)?;
        let header_size = get_u16(bytes, offset + 2)? as usize;
        let chunk_size = get_u32(bytes, offset + 4)? as usize;
        if chunk_size < header_size || offset + chunk_size > bytes.len() {
            bail!("AXMLチャンクサイズが不正です");
        }
        if kind == RES_STRING_POOL_TYPE {
            if header_size < 28 {
                bail!("文字列プールのヘッダーが不正です");
            }
            let string_count = get_u32(bytes, offset + 8)? as usize;
            let style_count = get_u32(bytes, offset + 12)? as usize;
            let flags = get_u32(bytes, offset + 16)?;
            let strings_start = get_u32(bytes, offset + 20)? as usize;
            let styles_start = get_u32(bytes, offset + 24)? as usize;
            let offsets_start = offset + header_size;
            let table_bytes = (string_count + style_count)
                .checked_mul(4)
                .context("文字列数が大きすぎます")?;
            if offsets_start + table_bytes > offset + chunk_size {
                bail!("文字列オフセットテーブルが不正です");
            }
            let mut offsets = Vec::with_capacity(string_count);
            for i in 0..string_count {
                offsets.push(get_u32(bytes, offsets_start + i * 4)? as usize);
            }
            let mut style_offsets = Vec::with_capacity(style_count);
            for i in 0..style_count {
                style_offsets.push(get_u32(
                    bytes,
                    offsets_start + (string_count + i) * 4,
                )?);
            }
            let data_start = offset + strings_start;
            let data_end = if styles_start == 0 {
                offset + chunk_size
            } else {
                offset + styles_start
            };
            if data_start > data_end || data_end > offset + chunk_size {
                bail!("文字列データ範囲が不正です");
            }
            let utf8 = flags & UTF8_FLAG != 0;
            let mut strings = Vec::with_capacity(string_count);
            for string_offset in offsets {
                let start = data_start
                    .checked_add(string_offset)
                    .context("文字列オフセットが大きすぎます")?;
                if start >= data_end {
                    bail!("文字列オフセットが範囲外です");
                }
                strings.push(if utf8 {
                    decode_utf8_string(&bytes[start..data_end])?
                } else {
                    decode_utf16_string(&bytes[start..data_end])?
                });
            }
            let style_data = if styles_start == 0 {
                Vec::new()
            } else {
                bytes[offset + styles_start..offset + chunk_size].to_vec()
            };
            return Ok(StringPool {
                offset,
                chunk_size,
                header_size,
                string_count,
                style_count,
                flags,
                strings_start,
                styles_start,
                style_offsets,
                style_data,
                strings,
            });
        }
        offset += chunk_size;
    }
    bail!("AXML文字列プールが見つかりません")
}

fn encode_string_pool(pool: &StringPool, strings: &[String]) -> Result<Vec<u8>> {
    if strings.len() != pool.string_count {
        bail!("文字列数が一致しません");
    }
    let utf8 = pool.flags & UTF8_FLAG != 0;
    let mut data = Vec::new();
    let mut offsets = Vec::with_capacity(strings.len());
    for string in strings {
        offsets.push(data.len() as u32);
        if utf8 {
            encode_utf8_string(string, &mut data)?;
        } else {
            encode_utf16_string(string, &mut data)?;
        }
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }

    let strings_start = pool.header_size + (pool.string_count + pool.style_count) * 4;
    let styles_start = if pool.style_count == 0 {
        0
    } else {
        strings_start + data.len()
    };
    let chunk_size = strings_start + data.len() + pool.style_data.len();
    let mut out = vec![0u8; pool.header_size];
    put_u16(&mut out, 0, RES_STRING_POOL_TYPE)?;
    put_u16(&mut out, 2, pool.header_size as u16)?;
    put_u32(&mut out, 4, chunk_size as u32)?;
    put_u32(&mut out, 8, pool.string_count as u32)?;
    put_u32(&mut out, 12, pool.style_count as u32)?;
    put_u32(&mut out, 16, pool.flags)?;
    put_u32(&mut out, 20, strings_start as u32)?;
    put_u32(&mut out, 24, styles_start as u32)?;
    for value in offsets {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in &pool.style_offsets {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&data);
    out.extend_from_slice(&pool.style_data);
    Ok(out)
}

fn patch_typed_attribute(bytes: &mut [u8], target_name_index: u32, value: u32) -> Result<()> {
    let mut offset = 8usize;
    while offset + 8 <= bytes.len() {
        let kind = get_u16(bytes, offset)?;
        let header_size = get_u16(bytes, offset + 2)? as usize;
        let chunk_size = get_u32(bytes, offset + 4)? as usize;
        if chunk_size < header_size || offset + chunk_size > bytes.len() {
            bail!("AXML要素チャンクが不正です");
        }
        if kind == RES_XML_START_ELEMENT_TYPE && header_size >= 16 && chunk_size >= header_size + 20 {
            let ext = offset + header_size;
            let attribute_start = get_u16(bytes, ext + 8)? as usize;
            let attribute_size = get_u16(bytes, ext + 10)? as usize;
            let attribute_count = get_u16(bytes, ext + 12)? as usize;
            if attribute_size >= 20 {
                let first = ext + attribute_start;
                for i in 0..attribute_count {
                    let attribute = first + i * attribute_size;
                    if attribute + 20 > offset + chunk_size {
                        bail!("AXML属性がチャンク範囲外です");
                    }
                    if get_u32(bytes, attribute + 4)? == target_name_index {
                        put_u32(bytes, attribute + 16, value)?;
                    }
                }
            }
        }
        offset += chunk_size;
    }
    Ok(())
}

fn decode_utf8_string(bytes: &[u8]) -> Result<String> {
    let (_, a) = decode_length8(bytes)?;
    let (byte_len, b) = decode_length8(&bytes[a..])?;
    let start = a + b;
    let end = start.checked_add(byte_len).context("UTF-8長が不正です")?;
    if end >= bytes.len() || bytes[end] != 0 {
        bail!("UTF-8文字列が切れています");
    }
    Ok(std::str::from_utf8(&bytes[start..end])?.to_owned())
}

fn decode_utf16_string(bytes: &[u8]) -> Result<String> {
    let (unit_len, used) = decode_length16(bytes)?;
    let byte_len = unit_len.checked_mul(2).context("UTF-16長が不正です")?;
    let end = used.checked_add(byte_len).context("UTF-16長が不正です")?;
    if end + 2 > bytes.len() {
        bail!("UTF-16文字列が切れています");
    }
    let mut units = Vec::with_capacity(unit_len);
    for chunk in bytes[used..end].chunks_exact(2) {
        units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    Ok(String::from_utf16(&units)?)
}

fn encode_utf8_string(value: &str, out: &mut Vec<u8>) -> Result<()> {
    let utf16_len = value.encode_utf16().count();
    encode_length8(utf16_len, out)?;
    encode_length8(value.len(), out)?;
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    Ok(())
}

fn encode_utf16_string(value: &str, out: &mut Vec<u8>) -> Result<()> {
    let units: Vec<u16> = value.encode_utf16().collect();
    encode_length16(units.len(), out)?;
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(())
}

fn decode_length8(bytes: &[u8]) -> Result<(usize, usize)> {
    let first = *bytes.first().context("文字列長がありません")?;
    if first & 0x80 == 0 {
        Ok((first as usize, 1))
    } else {
        let second = *bytes.get(1).context("文字列長が切れています")?;
        Ok(((((first & 0x7f) as usize) << 8) | second as usize, 2))
    }
}

fn decode_length16(bytes: &[u8]) -> Result<(usize, usize)> {
    let first = get_u16(bytes, 0)?;
    if first & 0x8000 == 0 {
        Ok((first as usize, 2))
    } else {
        let second = get_u16(bytes, 2)?;
        Ok(((((first & 0x7fff) as usize) << 16) | second as usize, 4))
    }
}

fn encode_length8(value: usize, out: &mut Vec<u8>) -> Result<()> {
    if value > 0x7fff {
        bail!("文字列が長すぎます");
    }
    if value > 0x7f {
        out.push(((value >> 8) as u8) | 0x80);
    }
    out.push(value as u8);
    Ok(())
}

fn encode_length16(value: usize, out: &mut Vec<u8>) -> Result<()> {
    if value > 0x7fff_ffff {
        bail!("文字列が長すぎます");
    }
    if value > 0x7fff {
        out.extend_from_slice(&(((value >> 16) as u16) | 0x8000).to_le_bytes());
    }
    out.extend_from_slice(&(value as u16).to_le_bytes());
    Ok(())
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes.get(offset..offset + 2).context("u16が範囲外です")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes.get(offset..offset + 4).context("u32が範囲外です")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<()> {
    bytes
        .get_mut(offset..offset + 2)
        .context("u16書き込みが範囲外です")?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<()> {
    bytes
        .get_mut(offset..offset + 4)
        .context("u32書き込みが範囲外です")?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_lengths_round_trip() {
        for value in [0, 1, 127, 128, 1024, 32767] {
            let mut bytes = Vec::new();
            encode_length8(value, &mut bytes).unwrap();
            assert_eq!(decode_length8(&bytes).unwrap().0, value);
        }
    }

    #[test]
    fn utf16_lengths_round_trip() {
        for value in [0, 1, 32767, 32768, 100_000] {
            let mut bytes = Vec::new();
            encode_length16(value, &mut bytes).unwrap();
            assert_eq!(decode_length16(&bytes).unwrap().0, value);
        }
    }
}
