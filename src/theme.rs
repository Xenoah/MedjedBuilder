use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

/// Windowsの個人用設定（ライト/ダーク、アクセントカラー）のスナップショット。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowsTheme {
    pub dark: bool,
    pub accent: Color32,
}

impl Default for WindowsTheme {
    fn default() -> Self {
        Self {
            dark: false,
            accent: Color32::from_rgb(0x00, 0x67, 0xC0),
        }
    }
}

impl WindowsTheme {
    /// アクセント色上で読めるテキスト色（白か黒）。
    pub fn on_accent(&self) -> Color32 {
        let [r, g, b, _] = self.accent.to_array();
        let luma = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
        if luma > 160.0 {
            Color32::BLACK
        } else {
            Color32::WHITE
        }
    }
}

#[cfg(windows)]
pub fn detect() -> WindowsTheme {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut theme = WindowsTheme::default();

    if let Ok(key) =
        hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
    {
        if let Ok(light) = key.get_value::<u32, _>("AppsUseLightTheme") {
            theme.dark = light == 0;
        }
    }

    if let Ok(key) = hkcu.open_subkey(r"Software\Microsoft\Windows\DWM") {
        // AccentColor はABGR順のDWORD。
        if let Ok(abgr) = key.get_value::<u32, _>("AccentColor") {
            theme.accent = Color32::from_rgb(
                (abgr & 0xFF) as u8,
                ((abgr >> 8) & 0xFF) as u8,
                ((abgr >> 16) & 0xFF) as u8,
            );
        }
    }

    theme
}

#[cfg(not(windows))]
pub fn detect() -> WindowsTheme {
    WindowsTheme::default()
}

/// 見出し・強調用の太字ファミリー名。
pub const BOLD_FAMILY: &str = "line_seed_jp_bold";

/// 同梱のLINE Seed JP（Regular/Bold）を既定フォントとして登録する。
/// Thinウェイトは使用しない。
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "line_seed_jp".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/LINESeedJP_A_TTF_Rg.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        BOLD_FAMILY.to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/LINESeedJP_A_TTF_Bd.ttf"))
            .into(),
    );
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("proportional family")
        .insert(0, "line_seed_jp".to_owned());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .expect("monospace family")
        .push("line_seed_jp".to_owned());
    // 太字ファミリー: 先頭をBoldに置き換え、残りはフォールバックとして流用。
    let mut bold_stack = fonts.families[&FontFamily::Proportional].clone();
    bold_stack[0] = BOLD_FAMILY.to_owned();
    fonts
        .families
        .insert(FontFamily::Name(BOLD_FAMILY.into()), bold_stack);
    ctx.set_fonts(fonts);
}

/// WinUI 3（Fluent Design）風のスタイルを適用する。
pub fn apply(ctx: &egui::Context, theme: &WindowsTheme) {
    let mut style = (*ctx.style()).clone();

    style.text_styles = [
        (TextStyle::Heading, FontId::new(22.0, FontFamily::Name(BOLD_FAMILY.into()))),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();

    style.spacing.item_spacing = egui::vec2(8.0, 5.0);
    style.spacing.button_padding = egui::vec2(12.0, 4.0);
    style.spacing.interact_size.y = 28.0;

    let mut visuals = if theme.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let radius = CornerRadius::same(4);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = radius;
    }
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(8);

    if theme.dark {
        let text = Color32::from_gray(0xFF);
        let text_weak = Color32::from_gray(0xC5);
        visuals.panel_fill = Color32::from_gray(0x20);
        visuals.window_fill = Color32::from_gray(0x2B);
        visuals.faint_bg_color = Color32::from_gray(0x2B); // カード背景
        visuals.window_stroke = Stroke::new(1.0, Color32::from_gray(0x38)); // カード枠
        visuals.extreme_bg_color = Color32::from_gray(0x1E); // テキスト入力背景
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_weak);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0x38));
        visuals.widgets.inactive.weak_bg_fill = Color32::from_gray(0x2D);
        visuals.widgets.inactive.bg_fill = Color32::from_gray(0x2D);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0x39));
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(0x32);
        visuals.widgets.hovered.bg_fill = Color32::from_gray(0x32);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(0x45));
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, text);
        visuals.widgets.active.weak_bg_fill = Color32::from_gray(0x27);
        visuals.widgets.active.bg_fill = Color32::from_gray(0x27);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, theme.accent);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, text_weak);
    } else {
        let text = Color32::from_gray(0x1B);
        let text_weak = Color32::from_gray(0x5D);
        visuals.panel_fill = Color32::from_gray(0xF3);
        visuals.window_fill = Color32::from_gray(0xFB);
        visuals.faint_bg_color = Color32::from_gray(0xFB); // カード背景
        visuals.window_stroke = Stroke::new(1.0, Color32::from_gray(0xE5)); // カード枠
        visuals.extreme_bg_color = Color32::WHITE; // テキスト入力背景
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_weak);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0xE5));
        visuals.widgets.inactive.weak_bg_fill = Color32::from_gray(0xFD);
        visuals.widgets.inactive.bg_fill = Color32::from_gray(0xFD);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0xD0));
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(0xF6);
        visuals.widgets.hovered.bg_fill = Color32::from_gray(0xF6);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(0xC5));
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, text);
        visuals.widgets.active.weak_bg_fill = Color32::from_gray(0xF5);
        visuals.widgets.active.bg_fill = Color32::from_gray(0xF5);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, theme.accent);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, text_weak);
    }

    visuals.selection.bg_fill = theme.accent;
    visuals.selection.stroke = Stroke::new(1.0, theme.on_accent());
    visuals.hyperlink_color = theme.accent;

    style.visuals = visuals;
    ctx.set_style(style);
}

/// 警告テキスト用の色（WinUIのCaution系に準拠）。
pub fn warning_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xFC, 0xE1, 0x00)
    } else {
        Color32::from_rgb(0x9D, 0x5D, 0x00)
    }
}
