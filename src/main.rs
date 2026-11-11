#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use disksize::{
    ExportSheet, FolderNode, ScanProgress, count_folders, default_export_filename,
    direct_files_size, format_count, format_size, list_top_level_folders,
    scan_directory_cancellable, with_xlsx_extension, write_excel_export,
};
use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, UiBuilder};
use rayon::prelude::*;

// ================== THEME ==================

mod theme {
    use egui::Color32;
    use std::sync::RwLock;

    /// Farbpalette für ein helles bzw. dunkles Design.
    #[derive(Clone, Copy)]
    pub struct Palette {
        pub window_bg: Color32,
        pub panel: Color32,
        pub toolbar: Color32,
        pub border: Color32,
        pub text: Color32,
        pub muted: Color32,
        pub accent: Color32,
        pub accent_soft: Color32,
        pub hover: Color32,
        pub row_alt: Color32,
        pub header_bg: Color32,
        pub track: Color32,
        pub summary_bg: Color32,
        pub success_bg: Color32,
        pub error_bg: Color32,
        pub red: Color32,
        pub green: Color32,
        pub orange: Color32,
        pub folder: Color32,
        pub folder_edge: Color32,
    }

    impl Palette {
        pub const fn light() -> Self {
            Self {
                window_bg: Color32::from_rgb(243, 244, 246),
                panel: Color32::WHITE,
                toolbar: Color32::from_rgb(251, 251, 252),
                border: Color32::from_rgb(218, 221, 225),
                text: Color32::from_rgb(32, 33, 36),
                muted: Color32::from_rgb(96, 100, 106),
                accent: Color32::from_rgb(0, 114, 198),
                accent_soft: Color32::from_rgb(219, 236, 250),
                hover: Color32::from_rgb(238, 245, 251),
                row_alt: Color32::from_rgb(248, 249, 251),
                header_bg: Color32::from_rgb(250, 250, 251),
                track: Color32::from_rgb(226, 229, 233),
                summary_bg: Color32::from_rgb(240, 242, 245),
                success_bg: Color32::from_rgb(223, 242, 224),
                error_bg: Color32::from_rgb(252, 228, 230),
                red: Color32::from_rgb(183, 28, 28),
                green: Color32::from_rgb(56, 142, 60),
                orange: Color32::from_rgb(230, 145, 30),
                folder: Color32::from_rgb(242, 195, 68),
                folder_edge: Color32::from_rgb(199, 151, 33),
            }
        }

        pub const fn dark() -> Self {
            Self {
                window_bg: Color32::from_rgb(28, 29, 32),
                panel: Color32::from_rgb(37, 39, 43),
                toolbar: Color32::from_rgb(43, 45, 50),
                border: Color32::from_rgb(62, 65, 71),
                text: Color32::from_rgb(232, 234, 237),
                muted: Color32::from_rgb(158, 163, 170),
                accent: Color32::from_rgb(96, 170, 255),
                accent_soft: Color32::from_rgb(38, 62, 92),
                hover: Color32::from_rgb(50, 58, 68),
                row_alt: Color32::from_rgb(42, 44, 49),
                header_bg: Color32::from_rgb(45, 47, 52),
                track: Color32::from_rgb(66, 70, 77),
                summary_bg: Color32::from_rgb(38, 40, 45),
                success_bg: Color32::from_rgb(31, 58, 38),
                error_bg: Color32::from_rgb(64, 33, 37),
                red: Color32::from_rgb(240, 110, 110),
                green: Color32::from_rgb(106, 200, 120),
                orange: Color32::from_rgb(240, 170, 70),
                folder: Color32::from_rgb(230, 180, 60),
                folder_edge: Color32::from_rgb(172, 132, 40),
            }
        }
    }

    static CURRENT: RwLock<Palette> = RwLock::new(Palette::light());

    pub fn set_dark(dark: bool) {
        let palette = if dark {
            Palette::dark()
        } else {
            Palette::light()
        };

        if let Ok(mut current) = CURRENT.write() {
            *current = palette;
        }
    }

    fn palette() -> Palette {
        CURRENT
            .read()
            .map(|current| *current)
            .unwrap_or_else(|_| Palette::light())
    }

    macro_rules! accessor {
        ($($name:ident),* $(,)?) => {
            $(
                pub fn $name() -> Color32 {
                    palette().$name
                }
            )*
        };
    }

    accessor!(
        window_bg,
        panel,
        toolbar,
        border,
        text,
        muted,
        accent,
        accent_soft,
        hover,
        row_alt,
        header_bg,
        track,
        summary_bg,
        success_bg,
        error_bg,
        red,
        green,
        orange,
        folder,
        folder_edge,
    );
}

// ================== STYLE ==================

fn install_style(ctx: &egui::Context, dark: bool) {
    theme::set_dark(dark);

    let egui_theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };

    ctx.set_theme(if dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });

    let mut style = (*ctx.style_of(egui_theme)).clone();

    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.extreme_bg_color = if dark {
        Color32::from_rgb(24, 25, 28)
    } else {
        Color32::from_rgb(224, 224, 224)
    };
    visuals.panel_fill = theme::window_bg();
    visuals.window_fill = theme::window_bg();
    visuals.override_text_color = Some(theme::text());
    visuals.selection.bg_fill = theme::accent_soft();
    visuals.selection.stroke = Stroke::new(1.0, theme::accent());
    style.visuals = visuals;

    // Etwas größere Schrift, kompakte Abstände wie in einem klassischen Tool.
    style.text_styles = [
        (
            egui::TextStyle::Small,
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Heading,
            egui::FontId::new(21.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(14.0, egui::FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);

    ctx.set_style_of(egui_theme, style);
}

fn settings_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);

    #[cfg(not(target_os = "windows"))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));

    base.map(|base| base.join("DiskSize").join("settings.txt"))
}

fn load_dark_mode() -> Option<bool> {
    let content = fs::read_to_string(settings_path()?).ok()?;

    match content.trim() {
        "dark" => Some(true),
        "light" => Some(false),
        _ => None,
    }
}

fn save_dark_mode(dark: bool) {
    let Some(path) = settings_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let _ = fs::write(path, if dark { "dark\n" } else { "light\n" });
}

// ================== APP ==================

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "DiskSize",
        options,
        Box::new(|cc| Ok(Box::new(DiskSizeApp::new(cc)))),
    )
}

struct DiskSizeApp {
    screen: Screen,
    drives: Vec<DriveInfo>,
    dark_mode: bool,
}

enum Screen {
    DriveSelection,
    Scan(Box<ScanPage>),
}

impl DiskSizeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let dark_mode = load_dark_mode().unwrap_or_else(|| {
            cc.egui_ctx
                .system_theme()
                .is_some_and(|theme| theme == egui::Theme::Dark)
        });

        install_style(&cc.egui_ctx, dark_mode);

        Self {
            screen: Screen::DriveSelection,
            drives: get_drives(),
            dark_mode,
        }
    }

    fn open_scan(&mut self, root: String) {
        self.screen = Screen::Scan(Box::new(ScanPage::new(root)));
    }
}

impl eframe::App for DiskSizeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        theme::set_dark(self.dark_mode);

        // Zentraler Hintergrund, damit das Fenster nicht schwarz bleibt.
        egui::CentralPanel::default().show(ui, |ui| {
            let mut open_drive = None;
            let mut go_back = false;
            let mut toggle_theme = false;

            match &mut self.screen {
                Screen::DriveSelection => {
                    let mut refresh = false;
                    open_drive = drive_selection_ui(
                        ui,
                        &self.drives,
                        &mut refresh,
                        self.dark_mode,
                        &mut toggle_theme,
                    );

                    if refresh {
                        self.drives = get_drives();
                    }
                }
                Screen::Scan(page) => {
                    page.poll_results();
                    toggle_theme = page.show(ui, self.dark_mode);

                    go_back = page.back_requested;

                    if !page.is_complete {
                        ui.ctx().request_repaint_after(Duration::from_millis(100));
                    }
                }
            }

            if toggle_theme {
                self.dark_mode = !self.dark_mode;
                install_style(ui.ctx(), self.dark_mode);
                save_dark_mode(self.dark_mode);
            }

            if let Some(drive) = open_drive {
                self.open_scan(drive);
            }

            if go_back {
                // Laufenden Scan sauber abbrechen.
                if let Screen::Scan(page) = &mut self.screen {
                    page.cancel_scan();
                }
                self.screen = Screen::DriveSelection;
            }
        });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::window_bg().to_normalized_gamma_f32()
    }
}

// ================== DRIVE SELECTION ==================

fn drive_selection_ui(
    ui: &mut egui::Ui,
    drives: &[DriveInfo],
    refresh: &mut bool,
    dark_mode: bool,
    toggle_theme: &mut bool,
) -> Option<String> {
    let mut selected = None;

    // Kopfleiste
    egui::Frame::new()
        .fill(theme::toolbar())
        .stroke(Stroke::new(1.0, theme::border()))
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                app_mark(ui);

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new("DiskSize").size(18.0).strong());
                    ui.label(
                        RichText::new("Laufwerke und Ordner analysieren")
                            .size(12.5)
                            .color(theme::muted()),
                    );
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme_toggle_button(ui, dark_mode) {
                        *toggle_theme = true;
                    }

                    if ui.button("Aktualisieren").clicked() {
                        *refresh = true;
                    }
                });
            });
        });

    // Fußleiste
    egui::Panel::bottom("drive_status")
        .frame(
            egui::Frame::new()
                .fill(theme::toolbar())
                .stroke(Stroke::new(1.0, theme::border()))
                .inner_margin(egui::Margin::symmetric(12, 4)),
        )
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} Laufwerke erkannt", drives.len()))
                        .size(12.5)
                        .color(theme::muted()),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                           env!("CARGO_PKG_VERSION")
                        ))
                        .size(12.0)
                        .color(theme::muted()),
                    );
                });
            });
        });

    // Inhalt
    egui::Frame::new()
        .fill(theme::window_bg())
        .inner_margin(egui::Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.label(RichText::new("Laufwerk auswählen").size(16.0).strong());
            ui.label(
                RichText::new("Klicken Sie auf ein Laufwerk, um den Scan zu starten.")
                    .size(12.5)
                    .color(theme::muted()),
            );
            ui.add_space(8.0);

            if drives.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Keine Laufwerke gefunden.").color(theme::muted()));
                });
                return;
            }

            drive_table_header(ui);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;

                    for (index, drive) in drives.iter().enumerate() {
                        let response = drive_row(ui, drive, index % 2 == 1);
                        if response.clicked() {
                            selected = Some(drive.mount.clone());
                        }
                    }
                });
        });

    selected
}

fn app_mark(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(7), theme::accent());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "TS",
        FontId::proportional(13.0),
        Color32::WHITE,
    );
    ui.add_space(2.0);
}

/// Runder Icon-Button zum Umschalten zwischen hellem und dunklem Design.
/// Zeigt den Mond im hellen und die Sonne im dunklen Modus.
fn theme_toggle_button(ui: &mut egui::Ui, dark_mode: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 26.0), Sense::click());

    let hovered = response.hovered();
    let background = if hovered {
        theme::hover()
    } else {
        theme::toolbar()
    };

    if hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), background);
    }

    let center = rect.center();
    let color = theme::text();

    if dark_mode {
        // Sonne: Kreis mit acht Strahlen.
        let radius = 5.0;
        ui.painter().circle_filled(center, radius, color);

        for index in 0..8 {
            let angle = std::f32::consts::TAU * index as f32 / 8.0;
            let direction = egui::vec2(angle.cos(), angle.sin());

            ui.painter().line_segment(
                [
                    center + direction * (radius + 2.0),
                    center + direction * (radius + 5.0),
                ],
                Stroke::new(1.6, color),
            );
        }
    } else {
        // Mond: heller Kreis, aus dem ein versetzter Kreis die Sichel schneidet.
        ui.painter().circle_filled(center, 7.0, color);
        ui.painter()
            .circle_filled(center + egui::vec2(4.5, -2.0), 6.2, background);
    }

    response
        .on_hover_text(if dark_mode {
            "Zum hellen Design wechseln"
        } else {
            "Zum dunklen Design wechseln"
        })
        .clicked()
}

fn drive_table_header(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let bar_width = 170.0;
    let value_width = 105.0;
    let name_width = (width - bar_width - value_width * 3.0 - 24.0).max(180.0);

    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 26.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::header_bg());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, theme::border()),
    );

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );

    header_cell(&mut child, name_width, "Laufwerk", false);
    header_cell(&mut child, value_width, "Gesamt", true);
    header_cell(&mut child, value_width, "Belegt", true);
    header_cell(&mut child, value_width, "Frei", true);
    header_cell(&mut child, bar_width, "Auslastung", false);
}

fn header_cell(ui: &mut egui::Ui, width: f32, text: &str, right_aligned: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), Sense::hover());

    let layout = if right_aligned {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    };

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(5.0, 0.0)))
            .layout(layout),
    );
    child.add(
        egui::Label::new(
            RichText::new(text)
                .size(12.5)
                .strong()
                .color(theme::muted()),
        )
        .selectable(false),
    );
}

fn drive_row(ui: &mut egui::Ui, drive: &DriveInfo, alternate: bool) -> egui::Response {
    let width = ui.available_width();
    let bar_width = 170.0;
    let value_width = 105.0;
    let name_width = (width - bar_width - value_width * 3.0 - 24.0).max(180.0);
    let height = 46.0;

    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());

    let background = if response.hovered() {
        theme::hover()
    } else if alternate {
        theme::row_alt()
    } else {
        theme::panel()
    };

    ui.painter().rect_filled(rect, 0.0, background);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, theme::border()),
    );

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );

    // Name + Einhängepunkt
    let (name_rect, _) = child.allocate_exact_size(egui::vec2(name_width, height), Sense::hover());
    let mut name_ui = child.new_child(
        UiBuilder::new()
            .max_rect(name_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    let (icon_rect, _) = name_ui.allocate_exact_size(egui::vec2(30.0, 24.0), Sense::hover());
    paint_drive_icon(&name_ui, icon_rect, drive.removable);

    name_ui.add_space(4.0);
    name_ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add(
            egui::Label::new(
                RichText::new(&drive.label)
                    .size(15.0)
                    .strong()
                    .color(theme::text()),
            )
            .truncate()
            .selectable(false),
        );
        ui.add(
            egui::Label::new(RichText::new(&drive.mount).size(12.0).color(theme::muted()))
                .truncate()
                .selectable(false),
        );
    });

    let used = drive.used();
    let free = drive.available;

    value_cell(&mut child, value_width, format_size(drive.total));
    value_cell(&mut child, value_width, format_size(used));
    value_cell(&mut child, value_width, format_size(free));

    // Auslastungsbalken
    let (bar_rect, _) = child.allocate_exact_size(egui::vec2(bar_width, height), Sense::hover());
    let mut bar_ui = child.new_child(
        UiBuilder::new()
            .max_rect(bar_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    let fraction = drive.used_fraction();
    let (track_rect, _) = bar_ui.allocate_exact_size(egui::vec2(112.0, 14.0), Sense::hover());

    bar_ui
        .painter()
        .rect_filled(track_rect, egui::CornerRadius::same(3), theme::track());

    let fill_width = track_rect.width() * fraction;
    if fill_width > 0.0 {
        let fill_rect =
            Rect::from_min_size(track_rect.min, egui::vec2(fill_width, track_rect.height()));
        bar_ui.painter().rect_filled(
            fill_rect,
            egui::CornerRadius::same(3),
            usage_color(fraction),
        );
    }

    bar_ui.add_space(6.0);
    bar_ui.add(
        egui::Label::new(
            RichText::new(format!("{:.1} %", fraction * 100.0))
                .size(12.5)
                .color(theme::muted()),
        )
        .selectable(false),
    );

    response
}

fn value_cell(ui: &mut egui::Ui, width: f32, text: String) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 22.0), Sense::hover());
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    child.add(
        egui::Label::new(RichText::new(text).size(13.5).color(theme::text())).selectable(false),
    );
}

fn usage_color(fraction: f32) -> Color32 {
    if fraction >= 0.9 {
        theme::red()
    } else if fraction >= 0.75 {
        theme::orange()
    } else {
        theme::green()
    }
}

fn paint_drive_icon(ui: &egui::Ui, rect: Rect, removable: bool) {
    let body = Rect::from_center_size(rect.center(), egui::vec2(24.0, 17.0));
    ui.painter().rect_filled(
        body,
        egui::CornerRadius::same(3),
        Color32::from_rgb(176, 190, 204),
    );
    ui.painter().rect_stroke(
        body,
        egui::CornerRadius::same(3),
        Stroke::new(1.0, Color32::from_rgb(120, 138, 156)),
        egui::StrokeKind::Inside,
    );

    let led = Rect::from_center_size(
        egui::pos2(body.right() - 6.0, body.bottom() - 4.5),
        egui::vec2(3.0, 3.0),
    );
    ui.painter().rect_filled(
        led,
        egui::CornerRadius::same(2),
        if removable {
            theme::orange()
        } else {
            theme::green()
        },
    );
}

fn paint_folder_icon(ui: &egui::Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();

    let tab = Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + rect.height() * 0.16),
        egui::vec2(rect.width() * 0.46, rect.height() * 0.22),
    );
    painter.rect_filled(tab, egui::CornerRadius::same(2), color);

    let body = Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + rect.height() * 0.3),
        egui::pos2(rect.right(), rect.bottom()),
    );
    painter.rect_filled(body, egui::CornerRadius::same(2), color);
    painter.rect_stroke(
        body,
        egui::CornerRadius::same(2),
        Stroke::new(1.0, theme::folder_edge()),
        egui::StrokeKind::Inside,
    );
}

// ================== SCAN PAGE ==================

enum ScanMessage {
    RootFiles {
        generation: u64,
        size: u64,
    },
    Node {
        generation: u64,
        index: usize,
        node: FolderNode,
    },
    Done {
        generation: u64,
    },
}

struct ScanPage {
    root_path: String,
    drive_total: u64,
    drive_free: u64,

    top_folders: Vec<FolderNode>,
    denied_folders: Vec<FolderNode>,

    total_root_size: u64,

    scanned_folders: usize,
    total_folders: usize,
    scanned_directories: usize,
    scanned_files: usize,

    progress: Arc<ScanProgress>,

    is_complete: bool,

    generation: u64,
    receiver: Option<Receiver<ScanMessage>>,
    cancel: Option<Arc<AtomicBool>>,

    back_requested: bool,

    notice: Option<(bool, String)>,

    selected_path: Option<PathBuf>,
    root_expanded: bool,
}

impl ScanPage {
    fn new(root_path: String) -> Self {
        let top_folders = list_top_level_folders(&root_path);
        let total_folders = top_folders.len();

        let generation = 0;

        let (receiver, cancel, progress) = start_scan(&root_path, &top_folders, generation);

        let (drive_total, drive_free) = drive_space(&root_path);

        Self {
            drive_total,
            drive_free,
            selected_path: Some(PathBuf::from(&root_path)),
            root_expanded: true,
            root_path,
            top_folders,
            denied_folders: Vec::new(),
            total_root_size: 0,
            scanned_folders: 0,
            total_folders,
            scanned_directories: 0,
            scanned_files: 0,
            progress,
            is_complete: false,
            generation,
            receiver: Some(receiver),
            cancel: Some(cancel),
            back_requested: false,
            notice: None,
        }
    }

    fn cancel_scan(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn restart(&mut self) {
        self.cancel_scan();

        self.generation += 1;

        self.scanned_folders = 0;
        self.scanned_directories = 0;
        self.scanned_files = 0;

        self.total_folders = self.top_folders.len();

        self.denied_folders.clear();
        self.total_root_size = 0;
        self.is_complete = false;
        self.notice = None;
        self.root_expanded = true;
        self.selected_path = Some(PathBuf::from(&self.root_path));

        for node in &mut self.top_folders {
            node.size = 0;
            node.children.clear();
            node.is_scanned = false;
            node.has_access = true;
            node.expanded = false;
        }

        let (receiver, cancel, progress) =
            start_scan(&self.root_path, &self.top_folders, self.generation);

        self.receiver = Some(receiver);
        self.cancel = Some(cancel);
        self.progress = progress;
    }

    fn poll_results(&mut self) {
        self.scanned_directories = self.progress.directories.load(Ordering::Relaxed);
        self.scanned_files = self.progress.files.load(Ordering::Relaxed);

        let mut messages = Vec::new();
        let mut disconnected = false;

        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        for message in messages {
            match message {
                ScanMessage::RootFiles { generation, size } if generation == self.generation => {
                    self.total_root_size = self.total_root_size.saturating_add(size);
                }

                ScanMessage::Node {
                    generation,
                    index,
                    node,
                } if generation == self.generation => {
                    if let Some(slot) = self.top_folders.get_mut(index) {
                        if node.has_access {
                            self.total_root_size = self.total_root_size.saturating_add(node.size);
                        } else {
                            // Verweigerte Ordner haben keine Kinder -> Kopie ist billig.
                            self.denied_folders.push(node.clone());
                        }

                        *slot = node;
                    }

                    self.scanned_folders += 1;
                }

                ScanMessage::Done { generation } if generation == self.generation => {
                    self.finish_scan();
                }

                _ => {}
            }
        }

        if disconnected && !self.is_complete {
            self.finish_scan();
        }
    }

    fn finish_scan(&mut self) {
        if self.is_complete {
            return;
        }

        self.top_folders
            .sort_by_key(|folder| std::cmp::Reverse(folder.size));

        self.is_complete = true;
        self.receiver = None;
        self.cancel = None;
    }

    fn export_excel(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Excel-Export speichern")
            .set_file_name(default_export_filename(&self.root_path))
            .add_filter("Excel-Arbeitsmappe", &["xlsx"])
            .save_file()
        else {
            // Dialog abgebrochen.
            return;
        };

        let path = with_xlsx_extension(path);

        match write_excel_export(
            &path,
            &[ExportSheet {
                name: "Ordner".to_string(),
                root_path: self.root_path.clone(),
                total_size: self.total_root_size,
                folders: &self.top_folders,
            }],
        ) {
            Ok(folder_count) => {
                self.notice = Some((
                    true,
                    format!(
                        "Excel-Export gespeichert ({} Ordner): {}",
                        folder_count,
                        path.display()
                    ),
                ));
            }
            Err(error) => {
                self.notice = Some((false, format!("Excel-Export fehlgeschlagen: {error}")));
            }
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, dark_mode: bool) -> bool {
        let toggle_theme = self.toolbar(ui, dark_mode);
        self.summary_bar(ui);
        self.notice_banner(ui);
        self.status_bar(ui);

        egui::Panel::left("scan_tree_panel")
            .resizable(true)
            .default_size(390.0)
            .min_size(280.0)
            .max_size(660.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .stroke(Stroke::new(1.0, theme::border()))
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ui, |ui| {
                self.tree_panel(ui);
            });

        egui::Frame::new()
            .fill(theme::panel())
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                self.details_panel(ui);
            });

        toggle_theme
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, dark_mode: bool) -> bool {
        let mut toggle_theme = false;

        egui::Frame::new()
            .fill(theme::toolbar())
            .stroke(Stroke::new(1.0, theme::border()))
            .inner_margin(egui::Margin::symmetric(12, 7))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    if ui.button("← Zurück").clicked() {
                        self.back_requested = true;
                    }

                    ui.separator();

                    let buttons_width = 330.0;
                    let path_width = (ui.available_width() - buttons_width).max(150.0);

                    // Adressleiste wie in einem Datei-Explorer
                    egui::Frame::new()
                        .fill(theme::panel())
                        .stroke(Stroke::new(1.0, theme::border()))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.set_width(path_width);

                            ui.horizontal(|ui| {
                                let (icon_rect, _) =
                                    ui.allocate_exact_size(egui::vec2(18.0, 16.0), Sense::hover());
                                paint_folder_icon(ui, icon_rect, theme::folder());

                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&self.root_path)
                                            .size(14.0)
                                            .color(theme::text()),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            });
                        });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme_toggle_button(ui, dark_mode) {
                            toggle_theme = true;
                        }

                        let export_button = ui
                            .add_enabled(self.is_complete, egui::Button::new("Excel-Export"))
                            .on_hover_text(if self.is_complete {
                                "Ordnerübersicht als Excel-Datei (.xlsx) speichern"
                            } else {
                                "Erst nach Abschluss des Scans verfügbar"
                            });

                        if export_button.clicked() {
                            self.export_excel();
                        }

                        if ui.button("Erneut scannen").clicked() {
                            self.restart();
                        }
                    });
                });
            });

        toggle_theme
    }

    fn summary_bar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(theme::summary_bg())
            .stroke(Stroke::new(1.0, theme::border()))
            .inner_margin(egui::Margin::symmetric(12, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    summary_item(ui, "Größe", &format_size(self.total_root_size));
                    ui.separator();
                    summary_item(
                        ui,
                        "Belegt",
                        &format_size(self.drive_total.saturating_sub(self.drive_free)),
                    );
                    ui.separator();
                    summary_item(ui, "Frei", &format_size(self.drive_free));
                    ui.separator();
                    summary_item(ui, "Dateien", &format_count(self.scanned_files));
                    ui.separator();
                    summary_item(ui, "Ordner", &format_count(self.scanned_directories));

                    if !self.denied_folders.is_empty() {
                        ui.separator();
                        ui.label(
                            RichText::new(format!(
                                "Zugriff verweigert: {}",
                                self.denied_folders.len()
                            ))
                            .size(12.5)
                            .color(theme::red()),
                        );
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.is_complete {
                            ui.label(
                                RichText::new("Scan abgeschlossen")
                                    .size(12.5)
                                    .color(theme::green()),
                            );
                        } else {
                            let progress = if self.total_folders > 0 {
                                self.scanned_folders as f32 / self.total_folders as f32
                            } else {
                                0.0
                            };

                            ui.add(
                                egui::ProgressBar::new(progress)
                                    .desired_width(150.0)
                                    .show_percentage(),
                            );
                            ui.label(RichText::new("Scan läuft").size(12.5).color(theme::muted()));
                        }
                    });
                });
            });
    }

    fn notice_banner(&mut self, ui: &mut egui::Ui) {
        let Some((success, message)) = self.notice.clone() else {
            return;
        };

        let fill = if success {
            theme::success_bg()
        } else {
            theme::error_bg()
        };
        let text_color = if success {
            theme::green()
        } else {
            theme::red()
        };

        let mut dismiss = false;

        egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(1.0, text_color))
            .inner_margin(egui::Margin::symmetric(12, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(message).size(13.0).color(text_color))
                            .wrap(),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            dismiss = true;
                        }
                    });
                });
            });

        if dismiss {
            self.notice = None;
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("scan_status")
            .frame(
                egui::Frame::new()
                    .fill(theme::toolbar())
                    .stroke(Stroke::new(1.0, theme::border()))
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if self.is_complete {
                            "Bereit"
                        } else {
                            "Scan läuft …"
                        })
                        .size(12.5)
                        .color(theme::muted()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{} Ordner · {} Dateien",
                            format_count(self.scanned_directories),
                            format_count(self.scanned_files)
                        ))
                        .size(12.5)
                        .color(theme::muted()),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("DiskSize {}", env!("CARGO_PKG_VERSION")))
                                .size(12.0)
                                .color(theme::muted()),
                        );
                    });
                });
            });
    }

    fn tree_panel(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());

        egui::Frame::new()
            .fill(theme::header_bg())
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ORDNER")
                            .size(11.5)
                            .strong()
                            .color(theme::muted()),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format_count(count_folders(&self.top_folders)))
                                .size(11.5)
                                .color(theme::muted()),
                        );
                    });
                });
            });

        let rows = self.tree_rows();
        let mut select: Option<PathBuf> = None;
        let mut toggle: Option<PathBuf> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;

                for row in &rows {
                    let selected = self.selected_path.as_deref() == Some(row.path.as_path());
                    let (response, arrow_clicked) = tree_row(ui, row, selected);

                    if arrow_clicked {
                        toggle = Some(row.path.clone());
                    } else if response.clicked() {
                        select = Some(row.path.clone());
                    }

                    if response.double_clicked() {
                        toggle = Some(row.path.clone());
                    }
                }
            });

        if let Some(path) = toggle {
            self.toggle_folder(&path);
        }

        if let Some(path) = select {
            self.select_folder(path);
        }
    }

    fn tree_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::with_capacity(64);

        rows.push(TreeRow {
            path: PathBuf::from(&self.root_path),
            name: self.root_path.clone(),
            size: self.total_root_size,
            depth: 0,
            has_children: !self.top_folders.is_empty(),
            expanded: self.root_expanded,
            has_access: true,
            is_root: true,
        });

        if self.root_expanded {
            collect_tree_rows(&self.top_folders, 1, &mut rows);
        }

        rows
    }

    fn toggle_folder(&mut self, path: &Path) {
        if Path::new(&self.root_path) == path {
            self.root_expanded = !self.root_expanded;
            return;
        }

        toggle_folder_in(&mut self.top_folders, path);
    }

    fn select_folder(&mut self, path: PathBuf) {
        if Path::new(&self.root_path) == path {
            self.root_expanded = true;
        } else {
            expand_ancestors(&mut self.top_folders, &path);
            self.root_expanded = true;
        }

        self.selected_path = Some(path);
    }

    fn details_panel(&mut self, ui: &mut egui::Ui) {
        let root_path = PathBuf::from(&self.root_path);
        let selected_path = self.selected_path.clone().unwrap_or(root_path.clone());

        let is_root = selected_path == root_path;

        let (title, subtitle, total, children, has_access): (
            String,
            String,
            u64,
            &[FolderNode],
            bool,
        ) = if is_root {
            (
                self.root_path.clone(),
                "Stammverzeichnis".to_string(),
                self.total_root_size,
                &self.top_folders,
                true,
            )
        } else if let Some(node) = find_node(&self.top_folders, &selected_path) {
            (
                node.name.clone(),
                node.path.display().to_string(),
                node.size,
                &node.children,
                node.has_access,
            )
        } else {
            (
                "Unbekannter Ordner".to_string(),
                selected_path.display().to_string(),
                0,
                &[],
                true,
            )
        };

        let mut open_path: Option<PathBuf> = None;
        let mut open_explorer = false;
        let explorer_target = selected_path.clone();
        let explorer_available = explorer_target.exists();

        egui::Frame::new()
            .fill(theme::toolbar())
            .stroke(Stroke::new(1.0, theme::border()))
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 24.0), Sense::hover());
                    paint_folder_icon(
                        ui,
                        icon_rect,
                        if has_access {
                            theme::folder()
                        } else {
                            theme::red()
                        },
                    );

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;

                        ui.add(
                            egui::Label::new(RichText::new(&title).size(17.0).strong().color(
                                if has_access {
                                    theme::text()
                                } else {
                                    theme::red()
                                },
                            ))
                            .truncate()
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&subtitle).size(12.0).color(theme::muted()),
                            )
                            .truncate()
                            .selectable(false),
                        );
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(170.0);
                            ui.spacing_mut().item_spacing.y = 0.0;

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format_size(total))
                                        .size(18.0)
                                        .strong()
                                        .color(theme::text()),
                                );
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let share = if self.drive_total > 0 {
                                    total as f64 / self.drive_total as f64
                                } else {
                                    0.0
                                };

                                ui.label(
                                    RichText::new(format!("{:.1} % des Laufwerks", share * 100.0))
                                        .size(11.5)
                                        .color(theme::muted()),
                                );
                            });
                        });
                    });
                });
            });

        egui::Frame::new()
            .fill(theme::panel())
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if is_root {
                            "Inhalt des Stammverzeichnisses"
                        } else {
                            "Unterordner"
                        })
                        .size(13.0)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(format!("({})", children.len()))
                            .size(12.0)
                            .color(theme::muted()),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let button = ui
                            .add_enabled(
                                explorer_available,
                                egui::Button::new("Im Explorer öffnen"),
                            )
                            .on_hover_text(if explorer_available {
                                "Diesen Ordner im Windows-Explorer anzeigen"
                            } else {
                                "Ordner ist nicht mehr vorhanden"
                            });

                        if button.clicked() {
                            open_explorer = true;
                        }
                    });
                });

                ui.add_space(4.0);

                if children.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(if has_access {
                                "Dieser Ordner enthält keine Unterordner."
                            } else {
                                "Kein Zugriff auf diesen Ordner."
                            })
                            .size(13.0)
                            .color(theme::muted()),
                        );
                    });
                    return;
                }

                detail_table_header(ui);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;

                        for (index, node) in children.iter().enumerate() {
                            let response = detail_row(ui, node, total, index % 2 == 1);

                            if response.clicked() || response.double_clicked() {
                                open_path = Some(node.path.clone());
                            }
                        }
                    });
            });

        if open_explorer && let Err(error) = open_in_explorer(&explorer_target) {
            self.notice = Some((
                false,
                format!("Explorer konnte nicht geöffnet werden: {error}"),
            ));
        }

        if let Some(path) = open_path {
            self.select_folder(path);
        }
    }
}

// ================== SCAN UI HELPERS ==================

struct TreeRow {
    path: PathBuf,
    name: String,
    size: u64,
    depth: usize,
    has_children: bool,
    expanded: bool,
    has_access: bool,
    is_root: bool,
}

fn collect_tree_rows(folders: &[FolderNode], depth: usize, rows: &mut Vec<TreeRow>) {
    for folder in folders {
        rows.push(TreeRow {
            path: folder.path.clone(),
            name: folder.name.clone(),
            size: folder.size,
            depth,
            has_children: !folder.children.is_empty(),
            expanded: folder.expanded,
            has_access: folder.has_access,
            is_root: false,
        });

        if folder.expanded {
            collect_tree_rows(&folder.children, depth + 1, rows);
        }
    }
}

fn tree_row(ui: &mut egui::Ui, row: &TreeRow, selected: bool) -> (egui::Response, bool) {
    let height = 27.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());

    let background = if selected {
        theme::accent_soft()
    } else if response.hovered() {
        theme::hover()
    } else {
        Color32::TRANSPARENT
    };

    if background != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 0.0, background);
    }

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );

    child.add_space(row.depth as f32 * 14.0);

    let arrow = if row.expanded { "▾" } else { "▸" };
    let arrow_sense = if row.has_children {
        Sense::click()
    } else {
        Sense::hover()
    };

    let arrow_response = child.add_sized(
        [18.0, 18.0],
        egui::Label::new(
            RichText::new(if row.has_children { arrow } else { "" })
                .size(13.0)
                .color(theme::muted()),
        )
        .selectable(false)
        .sense(arrow_sense),
    );

    let (icon_rect, _) = child.allocate_exact_size(egui::vec2(18.0, 16.0), Sense::hover());

    paint_folder_icon(
        &child,
        icon_rect,
        if row.has_access {
            theme::folder()
        } else {
            theme::red()
        },
    );

    child.add_space(2.0);

    let size_width = 92.0;
    let name_width = (child.available_width() - size_width - 8.0).max(40.0);

    let mut text = RichText::new(&row.name)
        .size(14.0)
        .color(if row.has_access {
            theme::text()
        } else {
            theme::red()
        });

    if row.is_root {
        text = text.strong();
    }
    if !row.has_access {
        text = text.italics();
    }

    child.add_sized(
        [name_width, 20.0],
        egui::Label::new(text).truncate().selectable(false),
    );

    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.add(
            egui::Label::new(
                RichText::new(format_size(row.size))
                    .size(12.5)
                    .color(theme::muted()),
            )
            .selectable(false),
        );
    });

    (response, arrow_response.clicked())
}

fn find_node<'a>(folders: &'a [FolderNode], path: &Path) -> Option<&'a FolderNode> {
    for folder in folders {
        if folder.path == path {
            return Some(folder);
        }

        if let Some(node) = find_node(&folder.children, path) {
            return Some(node);
        }
    }

    None
}

fn toggle_folder_in(folders: &mut [FolderNode], path: &Path) -> bool {
    for folder in folders {
        if folder.path == path {
            folder.expanded = !folder.expanded;
            return true;
        }

        if toggle_folder_in(&mut folder.children, path) {
            return true;
        }
    }

    false
}

fn expand_ancestors(folders: &mut [FolderNode], path: &Path) {
    for folder in folders {
        if path == folder.path || path.starts_with(&folder.path) {
            folder.expanded = true;
            expand_ancestors(&mut folder.children, path);
        }
    }
}

struct DetailColumns {
    name: f32,
    size: f32,
    percent: f32,
    elements: f32,
    bar: f32,
}

fn detail_columns(width: f32) -> DetailColumns {
    let size = 100.0;
    let percent = 72.0;
    let elements = 80.0;
    let bar = 140.0;
    let gaps = 32.0;
    let name = (width - size - percent - elements - bar - gaps).max(120.0);

    DetailColumns {
        name,
        size,
        percent,
        elements,
        bar,
    }
}

fn detail_table_header(ui: &mut egui::Ui) {
    let cols = detail_columns(ui.available_width());

    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), Sense::hover());

    ui.painter().rect_filled(rect, 0.0, theme::header_bg());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, theme::border()),
    );

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );

    header_cell(&mut child, cols.name, "Name", false);
    header_cell(&mut child, cols.size, "Größe", true);
    header_cell(&mut child, cols.percent, "Anteil", true);
    header_cell(&mut child, cols.elements, "Elemente", true);
    header_cell(&mut child, cols.bar, "Größenverteilung", false);
}

fn detail_row(
    ui: &mut egui::Ui,
    node: &FolderNode,
    parent_size: u64,
    alternate: bool,
) -> egui::Response {
    let cols = detail_columns(ui.available_width());
    let height = 28.0;

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());

    let background = if response.hovered() {
        theme::hover()
    } else if alternate {
        theme::row_alt()
    } else {
        theme::panel()
    };

    ui.painter().rect_filled(rect, 0.0, background);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, theme::border()),
    );

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );

    // Name
    let (name_rect, _) = child.allocate_exact_size(egui::vec2(cols.name, 22.0), Sense::hover());
    let mut name_ui = child.new_child(
        UiBuilder::new()
            .max_rect(name_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    let (icon_rect, _) = name_ui.allocate_exact_size(egui::vec2(17.0, 15.0), Sense::hover());
    paint_folder_icon(
        &name_ui,
        icon_rect,
        if node.has_access {
            theme::folder()
        } else {
            theme::red()
        },
    );
    name_ui.add_space(4.0);

    let mut text = RichText::new(&node.name)
        .size(14.0)
        .color(if node.has_access {
            theme::text()
        } else {
            theme::red()
        });

    if !node.has_access {
        text = text.italics();
    }

    name_ui.add(egui::Label::new(text).truncate().selectable(false));

    let share = if parent_size > 0 {
        node.size as f64 / parent_size as f64
    } else {
        0.0
    };

    number_cell(&mut child, cols.size, format_size(node.size));
    number_cell(&mut child, cols.percent, format!("{:.1} %", share * 100.0));
    number_cell(
        &mut child,
        cols.elements,
        if node.has_access {
            format_count(node.children.len())
        } else {
            "—".to_string()
        },
    );

    // Balken
    let (bar_rect, _) = child.allocate_exact_size(egui::vec2(cols.bar, 22.0), Sense::hover());
    let mut bar_ui = child.new_child(
        UiBuilder::new()
            .max_rect(bar_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    let (track, _) =
        bar_ui.allocate_exact_size(egui::vec2((cols.bar - 8.0).max(40.0), 10.0), Sense::hover());

    bar_ui
        .painter()
        .rect_filled(track, egui::CornerRadius::same(3), theme::track());

    let filled_width = track.width() * (share as f32).clamp(0.0, 1.0);

    if filled_width > 0.0 {
        let filled = Rect::from_min_size(track.min, egui::vec2(filled_width, track.height()));

        bar_ui.painter().rect_filled(
            filled,
            egui::CornerRadius::same(3),
            if node.has_access {
                theme::accent()
            } else {
                theme::red()
            },
        );
    }

    response
}

fn number_cell(ui: &mut egui::Ui, width: f32, text: String) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 22.0), Sense::hover());

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::right_to_left(Align::Center)),
    );

    child.add(
        egui::Label::new(RichText::new(text).size(13.0).color(theme::text())).selectable(false),
    );
}

fn summary_item(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).size(12.0).color(theme::muted()));
    ui.label(
        RichText::new(value)
            .size(12.5)
            .strong()
            .color(theme::text()),
    );
}

// ================== OPEN IN EXPLORER ==================

fn open_in_explorer(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}

// ================== DRIVE DETECTION ==================

struct DriveInfo {
    mount: String,
    label: String,
    total: u64,
    available: u64,
    removable: bool,
}

impl DriveInfo {
    fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    fn used_fraction(&self) -> f32 {
        if self.total > 0 {
            (self.used() as f64 / self.total as f64) as f32
        } else {
            0.0
        }
    }
}

fn normalize_mount(path: &Path) -> String {
    let mut mount = path.to_string_lossy().into_owned();

    #[cfg(target_os = "windows")]
    if !mount.ends_with('\\') {
        mount.push('\\');
    }

    if mount.is_empty() {
        mount = "/".to_string();
    }

    mount
}

#[cfg(target_os = "windows")]
fn is_drive_mount(mount: &Path) -> bool {
    let text = mount.to_string_lossy();
    let bytes = text.as_bytes();

    bytes.len() >= 2 && bytes.len() <= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(not(target_os = "windows"))]
fn is_drive_mount(_mount: &Path) -> bool {
    true
}

fn get_drives() -> Vec<DriveInfo> {
    #[cfg(target_os = "windows")]
    let mut drives = windows_drives();

    #[cfg(not(target_os = "windows"))]
    let mut drives = sysinfo_drives();

    drives.sort_by_key(|drive| drive.mount.to_lowercase());
    drives.dedup_by(|a, b| a.mount.eq_ignore_ascii_case(&b.mount));

    if drives.is_empty()
        && let Some(system_drive) = std::env::var_os("SystemDrive")
    {
        let mount = normalize_mount(Path::new(&system_drive));
        let (total, available) = drive_space(&mount);

        drives.push(DriveInfo {
            label: mount.clone(),
            mount,
            total,
            available,
            removable: false,
        });
    }

    drives
}

/// Liest alle logischen Laufwerke inklusive verbundener Netzlaufwerke.
#[cfg(target_os = "windows")]
fn windows_drives() -> Vec<DriveInfo> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDriveStringsW};
    use windows_sys::Win32::System::WindowsProgramming::{
        DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOTE, DRIVE_REMOVABLE,
    };

    // Liste hat die Form "C:\\\0D:\\\0...\0".
    let mut buffer = [0u16; 256];
    let length = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) };

    if length == 0 || length as usize >= buffer.len() {
        return Vec::new();
    }

    let mut drives = Vec::new();
    let mut index = 0;

    while index < length as usize {
        let remaining = &buffer[index..length as usize];
        let Some(end) = remaining.iter().position(|&character| character == 0) else {
            break;
        };

        if end == 0 {
            break;
        }

        let mount = String::from_utf16_lossy(&remaining[..end]);
        index += end + 1;

        if !is_drive_mount(Path::new(&mount)) {
            continue;
        }

        let mount_wide = wide_null(&mount);
        let drive_type = unsafe { GetDriveTypeW(mount_wide.as_ptr()) };

        // Feste, wechselbare und Netzlaufwerke sowie RAM-Disks anzeigen.
        if !matches!(
            drive_type,
            DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_REMOTE | DRIVE_RAMDISK
        ) {
            continue;
        }

        let (total, available) = match windows_disk_space(&mount) {
            Some(space) => space,
            // Nicht verbundene Netzlaufwerke nicht als leere Einträge anzeigen.
            None if drive_type == DRIVE_REMOTE => continue,
            None => (0, 0),
        };

        let label = windows_volume_label(&mount).unwrap_or_else(|| mount.clone());

        drives.push(DriveInfo {
            mount,
            label,
            total,
            available,
            removable: drive_type == DRIVE_REMOVABLE,
        });
    }

    drives
}

#[cfg(not(target_os = "windows"))]
fn sysinfo_drives() -> Vec<DriveInfo> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();

    disks
        .list()
        .iter()
        .filter(|disk| is_drive_mount(disk.mount_point()))
        .map(|disk| {
            let mount = normalize_mount(disk.mount_point());
            let label = disk.name().to_string_lossy().trim().to_string();

            DriveInfo {
                label: if label.is_empty() {
                    mount.clone()
                } else {
                    label
                },
                mount,
                total: disk.total_space(),
                available: disk.available_space(),
                removable: disk.is_removable(),
            }
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn drive_space(path: &str) -> (u64, u64) {
    windows_disk_space(path).unwrap_or((0, 0))
}

#[cfg(not(target_os = "windows"))]
fn drive_space(path: &str) -> (u64, u64) {
    use sysinfo::{Disk, Disks};

    let disks = Disks::new_with_refreshed_list();
    let path = Path::new(path);

    disks
        .list()
        .iter()
        .filter(|disk: &&Disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk: &&Disk| disk.mount_point().as_os_str().len())
        .map(|disk: &Disk| (disk.total_space(), disk.available_space()))
        .unwrap_or((0, 0))
}

#[cfg(target_os = "windows")]
fn windows_disk_space(path: &str) -> Option<(u64, u64)> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let path = wide_null(path);
    let mut total = 0u64;
    let mut available = 0u64;

    let success = unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            std::ptr::null_mut(),
            &mut total,
            &mut available,
        )
    };

    (success != 0).then_some((total, available))
}

#[cfg(target_os = "windows")]
fn windows_volume_label(root: &str) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

    let root = wide_null(root);
    let mut name = [0u16; 256];

    let success = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            name.as_mut_ptr(),
            name.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };

    if success == 0 {
        return None;
    }

    let length = name.iter().position(|&character| character == 0)?;
    let label = String::from_utf16_lossy(&name[..length]).trim().to_string();

    if label.is_empty() { None } else { Some(label) }
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

// ================== SCAN ==================

fn start_scan(
    root: &str,
    folders: &[FolderNode],
    generation: u64,
) -> (Receiver<ScanMessage>, Arc<AtomicBool>, Arc<ScanProgress>) {
    let (sender, receiver) = mpsc::channel();

    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(ScanProgress::default());

    let worker_cancel = Arc::clone(&cancel);
    let worker_progress = Arc::clone(&progress);

    let root_path = PathBuf::from(root);
    let paths: Vec<PathBuf> = folders.iter().map(|folder| folder.path.clone()).collect();

    thread::spawn(move || {
        let worker_count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 8);

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .thread_name(|i| format!("DiskSize-{i}"))
            .build();

        let Ok(pool) = pool else {
            // Fallback: Rayon-Initialisierung fehlgeschlagen.
            let _ = sender.send(ScanMessage::Done { generation });
            return;
        };

        pool.install(|| {
            // Dateien direkt im Stammverzeichnis.
            let size = direct_files_size(&root_path);
            let _ = sender.send(ScanMessage::RootFiles { generation, size });

            // Jeder Top-Level-Ordner wird parallel gescannt und sofort gemeldet.
            paths.into_par_iter().enumerate().for_each_with(
                sender.clone(),
                |sender, (index, path)| {
                    if worker_cancel.load(Ordering::Relaxed) {
                        return;
                    }

                    let node = scan_directory_cancellable(&path, &worker_cancel, &worker_progress);

                    if worker_cancel.load(Ordering::Relaxed) {
                        return;
                    }

                    let _ = sender.send(ScanMessage::Node {
                        generation,
                        index,
                        node,
                    });
                },
            );

            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = sender.send(ScanMessage::Done { generation });
            }
        });
    });

    (receiver, cancel, progress)
}
