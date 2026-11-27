#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 极简 PDF 合并工具：选择若干 PDF、调整顺序、合并成一份。

mod merge;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use egui::{Align, Color32, Layout, RichText};

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PDF 合并工具")
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([600.0, 420.0])
            .with_icon(window_icon()),
        ..Default::default()
    };

    let result = eframe::run_native(
        "PDF 合并工具",
        options,
        Box::new(|cc| {
            install_cjk_font(&cc.egui_ctx);
            Ok(Box::new(BinderApp::default()))
        }),
    );

    if let Err(err) = result {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("PDF 合并工具")
            .set_description(format!("窗口创建失败：{err}"))
            .show();
    }
}

/// 窗口与任务栏图标：tools/make-icon.ps1 产出的 64x64 未预乘 RGBA。
fn window_icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!("../assets/icon64.rgba").to_vec(),
        width: 64,
        height: 64,
    }
}

/// egui 自带字体没有中文字形，这里借用 Windows 系统字体。
fn install_cjk_font(ctx: &egui::Context) {
    const CANDIDATES: [&str; 5] = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhl.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
    ];

    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "cjk".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        return;
    }
}

struct Entry {
    path: PathBuf,
    pages: u32,
}

impl Entry {
    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

enum Notice {
    Info(String),
    Error(String),
}

impl Notice {
    fn show(&self, ui: &mut egui::Ui) {
        let (text, color) = match self {
            Notice::Info(text) => (text, Color32::from_rgb(52, 150, 86)),
            Notice::Error(text) => (text, Color32::from_rgb(200, 66, 66)),
        };
        ui.label(RichText::new(text).color(color));
    }
}

enum Action {
    Up(usize),
    Down(usize),
    Remove(usize),
}

#[derive(Default)]
struct BinderApp {
    files: Vec<Entry>,
    notice: Option<Notice>,
}

impl BinderApp {
    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let mut errors = Vec::new();

        for path in paths {
            if path.is_dir() {
                errors.push(format!("{}：这是文件夹，请选择 PDF 文件", path.display()));
                continue;
            }

            let is_pdf = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));
            if !is_pdf {
                errors.push(format!("{}：不是 PDF 文件", path.display()));
                continue;
            }

            match merge::page_count(&path) {
                Ok(pages) => self.files.push(Entry { path, pages }),
                Err(err) => errors.push(format!("{}：{err}", path.display())),
            }
        }

        self.notice = if errors.is_empty() {
            None
        } else {
            Some(Notice::Error(errors.join("\n")))
        };
    }

    fn collect_dropped(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });

        if !dropped.is_empty() {
            self.add_paths(dropped);
        }
    }

    fn merge_now(&mut self) {
        if self.files.is_empty() {
            return;
        }

        let stem = self.files[0]
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("合并结果");
        let default_name = format!("{stem}-合并.pdf");

        let Some(output) = rfd::FileDialog::new()
            .add_filter("PDF 文件", &["pdf"])
            .set_title("保存合并后的 PDF")
            .set_file_name(default_name.as_str())
            .save_file()
        else {
            return;
        };

        let paths: Vec<PathBuf> = self.files.iter().map(|entry| entry.path.clone()).collect();
        self.notice = Some(match merge::merge_to_file(&paths, &output) {
            Ok(pages) => Notice::Info(format!("合并完成：共 {pages} 页 → {}", output.display())),
            Err(err) => Notice::Error(format!("合并失败：{err}")),
        });
    }
}

impl eframe::App for BinderApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.collect_dropped(&ctx);

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("添加 PDF 文件…").clicked()
                    && let Some(paths) = rfd::FileDialog::new()
                        .add_filter("PDF 文件", &["pdf"])
                        .set_title("选择要合并的 PDF")
                        .pick_files()
                {
                    self.add_paths(paths);
                }

                if ui
                    .add_enabled(!self.files.is_empty(), egui::Button::new("清空"))
                    .clicked()
                {
                    self.files.clear();
                    self.notice = None;
                }

                ui.separator();
                ui.label(RichText::new("也可以把 PDF 直接拖进窗口").weak());
            });
            ui.add_space(6.0);
        });

        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let total: u32 = self.files.iter().map(|entry| entry.pages).sum();
                ui.label(format!("{} 个文件 · 共 {} 页", self.files.len(), total));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(!self.files.is_empty(), egui::Button::new("合并并保存…"))
                        .clicked()
                    {
                        self.merge_now();
                    }
                });
            });

            if let Some(notice) = &self.notice {
                ui.add_space(4.0);
                notice.show(ui);
            }
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.files.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("还没有添加文件").heading().weak());
                    ui.add_space(6.0);
                    ui.label("点击「添加 PDF 文件…」，或者把 PDF 拖进这个窗口");
                });
                return;
            }

            if ui.ctx().input(|input| !input.raw.hovered_files.is_empty()) {
                ui.label(
                    RichText::new("松开鼠标即可添加文件").color(Color32::from_rgb(64, 128, 220)),
                );
                ui.add_space(4.0);
            }

            let mut action = None;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let count = self.files.len();

                    for (index, entry) in self.files.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let row_width = ui.available_width();
                            let name_width = (row_width - 215.0).max(90.0);

                            ui.add_sized(
                                [26.0, 20.0],
                                egui::Label::new(
                                    RichText::new(format!("{}", index + 1)).monospace().weak(),
                                ),
                            );
                            ui.add_sized(
                                [name_width, 20.0],
                                egui::Label::new(entry.name()).truncate(),
                            )
                            .on_hover_text(entry.path.display().to_string());
                            ui.add_sized(
                                [56.0, 20.0],
                                egui::Label::new(format!("{} 页", entry.pages)),
                            );

                            let size = egui::vec2(26.0, 20.0);
                            if ui
                                .add_enabled(index > 0, egui::Button::new("↑").min_size(size))
                                .on_hover_text("上移")
                                .clicked()
                            {
                                action = Some(Action::Up(index));
                            }
                            if ui
                                .add_enabled(
                                    index + 1 < count,
                                    egui::Button::new("↓").min_size(size),
                                )
                                .on_hover_text("下移")
                                .clicked()
                            {
                                action = Some(Action::Down(index));
                            }
                            if ui
                                .add_sized(size, egui::Button::new("×"))
                                .on_hover_text("移除")
                                .clicked()
                            {
                                action = Some(Action::Remove(index));
                            }
                        });
                        ui.separator();
                    }
                });

            match action {
                Some(Action::Up(index)) => {
                    self.files.swap(index, index - 1);
                }
                Some(Action::Down(index)) => {
                    self.files.swap(index, index + 1);
                }
                Some(Action::Remove(index)) => {
                    self.files.remove(index);
                }
                None => {}
            }
        });
    }
}
