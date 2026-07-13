//! AAB内の`base/manifest/AndroidManifest.xml`（aapt2 protobuf形式XML）の書き換え。
//!
//! メッセージ定義はAOSP `frameworks/base/tools/aapt2/Resources.proto` のうち
//! XMLドキュメントに現れる型を完全に写したもの。全フィールドをモデル化して
//! いるため、デコード→エンコードで情報が失われない。

use anyhow::{Context, Result};
use prost::Message;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Message)]
pub struct XmlNode {
    #[prost(oneof = "xml_node::Node", tags = "1, 2")]
    pub node: Option<xml_node::Node>,
    #[prost(message, optional, tag = "3")]
    pub source: Option<SourcePosition>,
}

pub mod xml_node {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Node {
        #[prost(message, tag = "1")]
        Element(super::XmlElement),
        #[prost(string, tag = "2")]
        Text(String),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct XmlElement {
    #[prost(message, repeated, tag = "1")]
    pub namespace_declaration: Vec<XmlNamespace>,
    #[prost(string, tag = "2")]
    pub namespace_uri: String,
    #[prost(string, tag = "3")]
    pub name: String,
    #[prost(message, repeated, tag = "4")]
    pub attribute: Vec<XmlAttribute>,
    #[prost(message, repeated, tag = "5")]
    pub child: Vec<XmlNode>,
}

#[derive(Clone, PartialEq, Message)]
pub struct XmlNamespace {
    #[prost(string, tag = "1")]
    pub prefix: String,
    #[prost(string, tag = "2")]
    pub uri: String,
    #[prost(message, optional, tag = "3")]
    pub source: Option<SourcePosition>,
}

#[derive(Clone, PartialEq, Message)]
pub struct XmlAttribute {
    #[prost(string, tag = "1")]
    pub namespace_uri: String,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(string, tag = "3")]
    pub value: String,
    #[prost(message, optional, tag = "4")]
    pub source: Option<SourcePosition>,
    #[prost(uint32, tag = "5")]
    pub resource_id: u32,
    #[prost(message, optional, tag = "6")]
    pub compiled_item: Option<Item>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SourcePosition {
    #[prost(uint32, tag = "1")]
    pub line_number: u32,
    #[prost(uint32, tag = "2")]
    pub column_number: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct Item {
    #[prost(oneof = "item::Value", tags = "1, 2, 3, 4, 5, 6, 7")]
    pub value: Option<item::Value>,
    #[prost(uint32, tag = "8")]
    pub flag_status: u32,
    #[prost(bool, tag = "9")]
    pub flag_negated: bool,
    #[prost(string, tag = "10")]
    pub flag_name: String,
}

pub mod item {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Value {
        #[prost(message, tag = "1")]
        Ref(super::Reference),
        #[prost(message, tag = "2")]
        Str(super::StringValue),
        #[prost(message, tag = "3")]
        RawStr(super::RawString),
        #[prost(message, tag = "4")]
        StyledStr(super::StyledString),
        #[prost(message, tag = "5")]
        File(super::FileReference),
        #[prost(message, tag = "6")]
        Id(super::Id),
        #[prost(message, tag = "7")]
        Prim(super::Primitive),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct Reference {
    #[prost(int32, tag = "1")]
    pub r#type: i32,
    #[prost(uint32, tag = "2")]
    pub id: u32,
    #[prost(string, tag = "3")]
    pub name: String,
    #[prost(bool, tag = "4")]
    pub private: bool,
    #[prost(message, optional, tag = "5")]
    pub is_dynamic: Option<BooleanValue>,
    #[prost(uint32, tag = "6")]
    pub type_flags: u32,
    #[prost(bool, tag = "7")]
    pub allow_raw: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct BooleanValue {
    #[prost(bool, tag = "1")]
    pub value: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct Id {}

#[derive(Clone, PartialEq, Message)]
pub struct StringValue {
    #[prost(string, tag = "1")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct RawString {
    #[prost(string, tag = "1")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct StyledString {
    #[prost(string, tag = "1")]
    pub value: String,
    #[prost(message, repeated, tag = "2")]
    pub span: Vec<styled_string::Span>,
}

pub mod styled_string {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Span {
        #[prost(string, tag = "1")]
        pub tag: String,
        #[prost(uint32, tag = "2")]
        pub first_char: u32,
        #[prost(uint32, tag = "3")]
        pub last_char: u32,
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct FileReference {
    #[prost(string, tag = "1")]
    pub path: String,
    #[prost(int32, tag = "2")]
    pub r#type: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct Primitive {
    #[prost(
        oneof = "primitive::OneofValue",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14"
    )]
    pub oneof_value: Option<primitive::OneofValue>,
}

pub mod primitive {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct NullType {}

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct EmptyType {}

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum OneofValue {
        #[prost(message, tag = "1")]
        NullValue(NullType),
        #[prost(message, tag = "2")]
        EmptyValue(EmptyType),
        #[prost(float, tag = "3")]
        FloatValue(f32),
        #[prost(float, tag = "4")]
        DimensionValueDeprecated(f32),
        #[prost(float, tag = "5")]
        FractionValueDeprecated(f32),
        #[prost(int32, tag = "6")]
        IntDecimalValue(i32),
        #[prost(uint32, tag = "7")]
        IntHexadecimalValue(u32),
        #[prost(bool, tag = "8")]
        BooleanValue(bool),
        #[prost(uint32, tag = "9")]
        ColorArgb8Value(u32),
        #[prost(uint32, tag = "10")]
        ColorRgb8Value(u32),
        #[prost(uint32, tag = "11")]
        ColorArgb4Value(u32),
        #[prost(uint32, tag = "12")]
        ColorRgb4Value(u32),
        #[prost(uint32, tag = "13")]
        DimensionValue(u32),
        #[prost(uint32, tag = "14")]
        FractionValue(u32),
    }
}

/// AXML版`patch_manifest`と同じ置換規則をprotobuf形式マニフェストへ適用する。
pub fn patch_manifest(
    manifest: &[u8],
    replacements: &HashMap<String, String>,
    version_code: u32,
    allow_cleartext_http: bool,
) -> Result<Vec<u8>> {
    let mut root =
        XmlNode::decode(manifest).context("AAB内のAndroidManifest.xmlを解析できません")?;
    patch_node(&mut root, replacements, version_code, allow_cleartext_http);
    Ok(root.encode_to_vec())
}

/// マニフェスト内の全属性値を列挙する（検証用）。
pub fn attribute_values(manifest: &[u8]) -> Result<Vec<String>> {
    let root = XmlNode::decode(manifest).context("protobufマニフェストを解析できません")?;
    let mut values = Vec::new();
    collect_values(&root, &mut values);
    Ok(values)
}

fn collect_values(node: &XmlNode, values: &mut Vec<String>) {
    if let Some(xml_node::Node::Element(element)) = &node.node {
        for attribute in &element.attribute {
            values.push(attribute.value.clone());
        }
        for child in &element.child {
            collect_values(child, values);
        }
    }
}

fn patch_node(
    node: &mut XmlNode,
    replacements: &HashMap<String, String>,
    version_code: u32,
    allow_cleartext_http: bool,
) {
    let Some(xml_node::Node::Element(element)) = &mut node.node else {
        return;
    };
    for attribute in &mut element.attribute {
        if let Some(new_value) = replacements.get(&attribute.value) {
            attribute.value = new_value.clone();
            set_compiled_string(attribute, new_value);
        }
        if attribute.name == "versionCode" {
            attribute.value = version_code.to_string();
            set_compiled_primitive(
                attribute,
                primitive::OneofValue::IntDecimalValue(version_code as i32),
            );
        }
        if attribute.name == "usesCleartextTraffic" {
            attribute.value = if allow_cleartext_http { "true" } else { "false" }.into();
            set_compiled_primitive(
                attribute,
                primitive::OneofValue::BooleanValue(allow_cleartext_http),
            );
        }
    }
    for child in &mut element.child {
        patch_node(child, replacements, version_code, allow_cleartext_http);
    }
}

fn set_compiled_string(attribute: &mut XmlAttribute, new_value: &str) {
    if let Some(item) = &mut attribute.compiled_item {
        match &mut item.value {
            Some(item::Value::Str(value)) => value.value = new_value.to_owned(),
            Some(item::Value::RawStr(value)) => value.value = new_value.to_owned(),
            _ => {}
        }
    }
}

fn set_compiled_primitive(attribute: &mut XmlAttribute, value: primitive::OneofValue) {
    let item = attribute.compiled_item.get_or_insert_with(Item::default);
    item.value = Some(item::Value::Prim(Primitive {
        oneof_value: Some(value),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> XmlNode {
        XmlNode {
            node: Some(xml_node::Node::Element(XmlElement {
                namespace_declaration: vec![XmlNamespace {
                    prefix: "android".into(),
                    uri: "http://schemas.android.com/apk/res/android".into(),
                    source: None,
                }],
                namespace_uri: String::new(),
                name: "manifest".into(),
                attribute: vec![
                    XmlAttribute {
                        namespace_uri: String::new(),
                        name: "package".into(),
                        value: "io.html2apk.generated".into(),
                        source: None,
                        resource_id: 0,
                        compiled_item: None,
                    },
                    XmlAttribute {
                        namespace_uri: "http://schemas.android.com/apk/res/android".into(),
                        name: "versionCode".into(),
                        value: "1".into(),
                        source: None,
                        resource_id: 0x0101021b,
                        compiled_item: Some(Item {
                            value: Some(item::Value::Prim(Primitive {
                                oneof_value: Some(primitive::OneofValue::IntDecimalValue(1)),
                            })),
                            ..Default::default()
                        }),
                    },
                ],
                child: vec![XmlNode {
                    node: Some(xml_node::Node::Element(XmlElement {
                        namespace_declaration: vec![],
                        namespace_uri: String::new(),
                        name: "application".into(),
                        attribute: vec![XmlAttribute {
                            namespace_uri: "http://schemas.android.com/apk/res/android".into(),
                            name: "label".into(),
                            value: "__H2A_APP_NAME__".into(),
                            source: None,
                            resource_id: 0x01010001,
                            compiled_item: Some(Item {
                                value: Some(item::Value::Str(StringValue {
                                    value: "__H2A_APP_NAME__".into(),
                                })),
                                ..Default::default()
                            }),
                        }],
                        child: vec![],
                    })),
                    source: None,
                }],
            })),
            source: None,
        }
    }

    #[test]
    fn patches_package_label_and_version() {
        let encoded = sample_manifest().encode_to_vec();
        let replacements = HashMap::from([
            ("io.html2apk.generated".to_string(), "com.example.app".to_string()),
            ("__H2A_APP_NAME__".to_string(), "テスト".to_string()),
        ]);
        let patched = patch_manifest(&encoded, &replacements, 42, true).unwrap();
        let decoded = XmlNode::decode(patched.as_slice()).unwrap();
        let Some(xml_node::Node::Element(manifest)) = &decoded.node else {
            panic!("manifest要素がありません");
        };
        assert_eq!(manifest.attribute[0].value, "com.example.app");
        assert_eq!(manifest.attribute[1].value, "42");
        let Some(xml_node::Node::Element(application)) = &manifest.child[0].node else {
            panic!("application要素がありません");
        };
        assert_eq!(application.attribute[0].value, "テスト");
        let Some(Item { value: Some(item::Value::Str(label)), .. }) =
            &application.attribute[0].compiled_item
        else {
            panic!("labelのcompiled_itemが失われました");
        };
        assert_eq!(label.value, "テスト");
    }
}
