#![windows_subsystem = "windows"]

mod theme;

use eframe::egui;
use html2apk::{build_apk, AppConfig, BuildRequest, Orientation, StorageMode};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Html2Apk")
            .with_inner_size([720.0, 790.0])
            .with_min_inner_size([620.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Html2Apk",
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
        ui.heading("HTMLからAndroid APKを作成");
        ui.label("HTMLフォルダを選び、必要な権限だけ有効にしてください。");
        ui.add_space(10.0);

        card(ui, |ui| {
            section_title(ui, "入力と出力");
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
                self.config.package_id = format!("com.html2apk.{}", slug(&name));
                self.output_apk = Some(path.with_extension("apk"));
                self.web_root = Some(path);
            });
            let current = self.icon.clone();
            path_picker(ui, "アイコン（任意）", current.as_ref(), || {
                rfd::FileDialog::new()
                    .add_filter("画像", &["png", "jpg", "jpeg", "webp"])
                    .pick_file()
            }, |path| self.icon = Some(path));
            let current = self.output_apk.clone();
            path_picker(ui, "出力APK", current.as_ref(), || {
                rfd::FileDialog::new()
                    .add_filter("Android APK", &["apk"])
                    .set_file_name("app.apk")
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
            });
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
                        .add_filter("Html2Apk署名鍵", &["h2akey"])
                        .pick_file()
                }, |path| self.signing_key = Some(path));
                ui.small("未指定の場合はパッケージIDごとの鍵を自動生成し、次回も再利用します。");
            });

        ui.add_space(12.0);
        let ready = self.web_root.is_some() && self.output_apk.is_some();
        let build_button = if ready {
            egui::Button::new(egui::RichText::new("APKをビルド").color(self.theme.on_accent()))
                .fill(self.theme.accent)
        } else {
            egui::Button::new("APKをビルド")
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
        };
        self.status = match build_apk(&request) {
            Ok(path) => format!("完成: {}（{:.1}秒）", path.display(), started.elapsed().as_secs_f32()),
            Err(error) => format!("エラー: {error:#}"),
        };
    }
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
        .inner_margin(egui::Margin::same(12))
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
