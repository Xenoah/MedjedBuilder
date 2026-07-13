use std::{env, fs, path::PathBuf};

fn main() {
    embed_template("H2A_TEMPLATE_APK", "template/template.apk", "template.apk");
    embed_template("H2A_TEMPLATE_AAB", "template/template.aab", "template.aab");
}

fn embed_template(env_name: &str, default_path: &str, file_name: &str) {
    println!("cargo:rerun-if-changed={default_path}");
    println!("cargo:rerun-if-env-changed={env_name}");

    let source = env::var_os(env_name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default_path));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let destination = out.join(file_name);

    if source.is_file() {
        fs::copy(&source, &destination).expect("failed to copy template");
    } else {
        panic!("{file_name} is missing: build template/android first or set {env_name}");
    }
}
