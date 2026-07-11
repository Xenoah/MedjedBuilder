use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=template/template.apk");
    println!("cargo:rerun-if-env-changed=H2A_TEMPLATE_APK");

    let source = env::var_os("H2A_TEMPLATE_APK")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("template/template.apk"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let destination = out.join("template.apk");

    if source.is_file() {
        fs::copy(&source, &destination).expect("failed to copy template APK");
    } else {
        panic!(
            "template APK is missing: build template/android first or set H2A_TEMPLATE_APK"
        );
    }
}

