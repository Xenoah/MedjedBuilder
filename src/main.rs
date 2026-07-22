#![windows_subsystem = "windows"]

mod theme;

use eframe::egui;
use html2apk::{build, AppConfig, BuildRequest, Orientation, OutputFormat, StorageMode};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// `.h2aproj`ファイルとして保存・復元されるプロジェクト一式。
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Project {
    web_root: Option<PathBuf>,
    output_apk: Option<PathBuf>,
    icon: Option<PathBuf>,
    signing_key: Option<PathBuf>,
    config: AppConfig,
    format: OutputFormat,
}

fn load_app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/app_icon.png"))
        .expect("app icon should be a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData { rgba: image.into_raw(), width, height }
}

fn main() -> eframe::Result<()> {
    if !ensure_single_instance() {
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MedjedBuilder -メジェドビルダー-")
            .with_icon(load_app_icon())
            .with_inner_size([720.0, 900.0])
            .with_resizable(false)
            .with_maximize_button(false),
        ..Default::default()
    };
    eframe::run_native(
        "MedjedBuilder",
        options,
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            let windows_theme = theme::detect();
            theme::apply(&cc.egui_ctx, &windows_theme);
            Ok(Box::new(BuilderApp {
                theme: windows_theme,
                ..Default::default()
            }))
        }),
    )
}

#[derive(Default)]
struct BuilderApp {
    web_root: Option<PathBuf>,
    output_apk: Option<PathBuf>,
    icon: Option<PathBuf>,
    signing_key: Option<PathBuf>,
    config: AppConfig,
    format: OutputFormat,
    status: String,
    advanced_open: bool,
    theme: theme::WindowsTheme,
    last_theme_poll: Option<Instant>,
}

impl eframe::App for BuilderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.follow_windows_theme(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| self.contents(ui));
        });
    }
}

impl BuilderApp {
    /// Windowsの個人用設定（ダークモード・アクセントカラー）を1秒ごとに反映する。
    fn follow_windows_theme(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let due = self
            .last_theme_poll
            .map_or(true, |t| now.duration_since(t) >= Duration::from_secs(1));
        if due {
            self.last_theme_poll = Some(now);
            let detected = theme::detect();
            if detected != self.theme {
                self.theme = detected;
                theme::apply(ctx, &self.theme);
            }
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("HTMLからAndroid APKを作成");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("プロジェクトを保存").clicked() {
                    self.save_project();
                }
                if ui.button("プロジェクトを読み込み").clicked() {
                    self.load_project();
                }
            });
        });
        ui.label("HTMLフォルダを選び、必要な権限だけ有効にしてください。");
        ui.add_space(10.0);

        card(ui, |ui| {
            section_title(ui, "入力と出力");
            ui.horizontal(|ui| {
                ui.label("出力形式");
                let apk = ui.selectable_value(&mut self.format, OutputFormat::Apk, "APK");
                let aab = ui.selectable_value(
                    &mut self.format,
                    OutputFormat::Aab,
                    "AAB（Google Play用）",
                );
                if apk.changed() || aab.changed() {
                    if let Some(output) = self.output_apk.take() {
                        self.output_apk = Some(output.with_extension(self.format.extension()));
                    }
                }
            });
            let extension = self.format.extension();
            let current = self.web_root.clone();
            path_picker(ui, "HTMLフォルダ", current.as_ref(), || {
                rfd::FileDialog::new().pick_folder()
            }, |path| {
                let name = path
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("myapp")
                    .to_owned();
                self.config.app_name = name.clone();
                self.config.package_id = format!("com.medjedbuilder.{}", slug(&name));
                self.output_apk = Some(path.with_extension(extension));
                self.web_root = Some(path);
            });
            let current = self.icon.clone();
            path_picker(ui, "アイコン（任意）", current.as_ref(), || {
                rfd::FileDialog::new()
                    .add_filter("画像", &["png", "jpg", "jpeg", "webp"])
                    .pick_file()
            }, |path| self.icon = Some(path));
            let current = self.output_apk.clone();
            let filter_name = match self.format {
                OutputFormat::Apk => "Android APK",
                OutputFormat::Aab => "Android App Bundle",
            };
            path_picker(ui, "出力ファイル", current.as_ref(), || {
                rfd::FileDialog::new()
                    .add_filter(filter_name, &[extension])
                    .set_file_name(format!("app.{extension}"))
                    .save_file()
            }, |path| self.output_apk = Some(path));
        });

        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, "アプリ情報");
            egui::Grid::new("app-info").num_columns(2).show(ui, |ui| {
                ui.label("アプリ名");
                ui.text_edit_singleline(&mut self.config.app_name);
                ui.end_row();
                ui.label("パッケージID");
                ui.text_edit_singleline(&mut self.config.package_id);
                ui.end_row();
                ui.label("バージョン");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.config.version_name);
                    ui.label("コード");
                    ui.add(egui::DragValue::new(&mut self.config.version_code).range(1..=2_100_000_000));
                });
                ui.end_row();
                ui.label("開始ページ");
                ui.text_edit_singleline(&mut self.config.start_page);
                ui.end_row();
            });
        });

        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, "表示");
            ui.horizontal(|ui| {
                ui.label("画面向き");
                ui.selectable_value(&mut self.config.orientation, Orientation::Auto, "自動");
                ui.selectable_value(&mut self.config.orientation, Orientation::Portrait, "縦");
                ui.selectable_value(&mut self.config.orientation, Orientation::Landscape, "横");
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.config.fullscreen, "全画面");
                ui.checkbox(&mut self.config.keep_screen_on, "画面を消灯しない");
                ui.checkbox(&mut self.config.open_external_links, "外部URLはブラウザで開く");
                ui.checkbox(&mut self.config.disable_splash, "起動スプラッシュを表示しない");
            });
            if self.config.disable_splash {
                ui.small("起動時のシステムスプラッシュ（アイコン画面）と白画面フラッシュを表示しません。");
            }
        });

        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, "ストレージ");
            egui::ComboBox::from_id_salt("storage-mode")
                .selected_text(storage_label(self.config.storage))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.config.storage, StorageMode::Private, "アプリ専用（権限不要）");
                    ui.selectable_value(&mut self.config.storage, StorageMode::Media, "音楽・画像・動画");
                    ui.selectable_value(&mut self.config.storage, StorageMode::Saf, "ユーザー選択フォルダ（推奨）");
                    ui.selectable_value(&mut self.config.storage, StorageMode::AllFiles, "全ファイル（サイドロード向け）");
                });
            ui.checkbox(&mut self.config.file_api, "JavaScriptファイルAPIを有効化");
            if self.config.storage == StorageMode::AllFiles {
                ui.colored_label(
                    theme::warning_color(ui.visuals().dark_mode),
                    "全ファイル権限はGoogle Play公開に制限があります。",
                );
            }
        });

        egui::CollapsingHeader::new("権限・詳細設定")
            .default_open(self.advanced_open)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.config.internet, "インターネット");
                    ui.checkbox(&mut self.config.camera, "カメラ");
                    ui.checkbox(&mut self.config.microphone, "マイク");
                    ui.checkbox(&mut self.config.location, "位置情報");
                    ui.checkbox(&mut self.config.notifications, "通知");
                });
                ui.checkbox(&mut self.config.allow_cleartext_http, "暗号化されていないHTTP通信を許可");
                egui::Grid::new("colors").num_columns(2).show(ui, |ui| {
                    ui.label("ステータスバー色");
                    ui.text_edit_singleline(&mut self.config.status_bar_color);
                    ui.end_row();
                    ui.label("ナビゲーションバー色");
                    ui.text_edit_singleline(&mut self.config.navigation_bar_color);
                    ui.end_row();
                });
                let current = self.signing_key.clone();
                path_picker(ui, "署名鍵（任意）", current.as_ref(), || {
                    rfd::FileDialog::new()
                        .add_filter("MedjedBuilder署名鍵", &["h2akey"])
                        .pick_file()
                }, |path| self.signing_key = Some(path));
                ui.small("未指定の場合はパッケージIDごとの鍵を自動生成し、次回も再利用します。");
            });

        ui.add_space(12.0);
        let ready = self.web_root.is_some() && self.output_apk.is_some();
        let build_label = match self.format {
            OutputFormat::Apk => "APKをビルド",
            OutputFormat::Aab => "AABをビルド",
        };
        let build_button = if ready {
            egui::Button::new(egui::RichText::new(build_label).color(self.theme.on_accent()))
                .fill(self.theme.accent)
        } else {
            egui::Button::new(build_label)
        }
        .min_size([180.0, 38.0].into());
        if ui.add_enabled(ready, build_button).clicked() {
            self.run_build();
        }
        if !self.status.is_empty() {
            ui.separator();
            ui.label(&self.status);
        }
    }

    fn save_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("MedjedBuilderプロジェクト", &["h2aproj"])
            .set_file_name(format!("{}.h2aproj", slug(&self.config.app_name)))
            .save_file()
        else {
            return;
        };
        let project = Project {
            web_root: self.web_root.clone(),
            output_apk: self.output_apk.clone(),
            icon: self.icon.clone(),
            signing_key: self.signing_key.clone(),
            config: self.config.clone(),
            format: self.format,
        };
        self.status = match serde_json::to_string_pretty(&project)
            .map_err(anyhow::Error::from)
            .and_then(|json| std::fs::write(&path, json).map_err(Into::into))
        {
            Ok(()) => format!("プロジェクトを保存しました: {}", path.display()),
            Err(error) => format!("エラー: プロジェクトを保存できません: {error:#}"),
        };
    }

    fn load_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("MedjedBuilderプロジェクト", &["h2aproj"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|text| serde_json::from_str::<Project>(&text).map_err(Into::into))
        {
            Ok(project) => {
                self.web_root = project.web_root;
                self.output_apk = project.output_apk;
                self.icon = project.icon;
                self.signing_key = project.signing_key;
                self.config = project.config;
                self.format = project.format;
                self.status = format!("プロジェクトを読み込みました: {}", path.display());
            }
            Err(error) => {
                self.status = format!("エラー: プロジェクトを読み込めません: {error:#}");
            }
        }
    }

    fn run_build(&mut self) {
        let Some(web_root) = self.web_root.clone() else { return };
        let Some(output_apk) = self.output_apk.clone() else { return };
        let started = Instant::now();
        let request = BuildRequest {
            web_root,
            output_apk,
            icon: self.icon.clone(),
            signing_key: self.signing_key.clone(),
            config: self.config.clone(),
            format: self.format,
        };
        self.status = match build(&request) {
            Ok(path) => format!("完成: {}（{:.1}秒）", path.display(), started.elapsed().as_secs_f32()),
            Err(error) => format!("エラー: {error:#}"),
        };
    }
}

/// 二重起動を防ぐ。既に起動済みの場合は既存ウィンドウを前面に出してfalseを返す。
#[cfg(windows)]
fn ensure_single_instance() -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};

    let mutex_name: Vec<u16> = "MedjedBuilder.SingleInstance\0".encode_utf16().collect();
    unsafe {
        // ミューテックスはプロセス終了まで保持するため、ハンドルは意図的に閉じない。
        CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let title: Vec<u16> = "MedjedBuilder -メジェドビルダー-\0".encode_utf16().collect();
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                SetForegroundWindow(hwnd);
            }
            return false;
        }
    }
    true
}

#[cfg(not(windows))]
fn ensure_single_instance() -> bool {
    true
}

/// セクションタイトル（LINE Seed JP Boldで表示）。
fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(egui::FontFamily::Name(theme::BOLD_FAMILY.into()))
            .size(15.0),
    );
}

/// WinUIのカード風フレーム。
fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let fill = ui.visuals().faint_bg_color;
    let stroke = ui.visuals().window_stroke;
    egui::Frame::default()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

fn path_picker(
    ui: &mut egui::Ui,
    label: &str,
    value: Option<&PathBuf>,
    pick: impl FnOnce() -> Option<PathBuf>,
    set: impl FnOnce(PathBuf),
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let text = value
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "未選択".into());
        ui.add_sized([400.0, 22.0], egui::Label::new(text).truncate());
        if ui.button("選択").clicked() {
            if let Some(path) = pick() {
                set(path);
            }
        }
    });
}

fn storage_label(mode: StorageMode) -> &'static str {
    match mode {
        StorageMode::Private => "アプリ専用（権限不要）",
        StorageMode::Media => "音楽・画像・動画",
        StorageMode::Saf => "ユーザー選択フォルダ（推奨）",
        StorageMode::AllFiles => "全ファイル（サイドロード向け）",
    }
}

fn slug(value: &str) -> String {
    let slug: String = value
        .chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() { Some(c.to_ascii_lowercase()) }
            else if c == '-' || c == ' ' || c == '_' { Some('_') }
            else { None }
        })
        .collect();
    let slug = slug.trim_matches('_');
    if slug.is_empty() { "myapp".into() } else { slug.into() }
}
