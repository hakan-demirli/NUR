use chrono::{Local, Utc};
use egui::{self, Color32, Margin, RichText, Stroke, Vec2};

use crate::charts::{self, ThemeColors};
use crate::collector;
use crate::config::Config;
use crate::db::{BatteryRecord, Database};

// ---------------------------------------------------------------------------

const SECTION_GAP: i8 = 8;
const SECTION_INNER_MARGIN: i8 = 10;
const SECTION_CORNER_RADIUS: f32 = 6.0;
const SIDEBAR_ITEM_SPACING: Vec2 = Vec2::new(8.0, 4.0);
const GRID_SPACING: [f32; 2] = [12.0, 4.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeRange {
    Recent(u64),
    Yesterday,
    Week,
    Month,
    ThreeMonths,
    Year,
    All,
}

impl TimeRange {
    pub fn label(&self) -> String {
        match self {
            Self::Recent(h) => format!("{}h", h),
            Self::Yesterday => "Yesterday".into(),
            Self::Week => "7 Days".into(),
            Self::Month => "30 Days".into(),
            Self::ThreeMonths => "90 Days".into(),
            Self::Year => "1 Year".into(),
            Self::All => "All".into(),
        }
    }

    pub fn time_format(&self) -> &'static str {
        match self {
            Self::Recent(_) | Self::Yesterday => "%H:%M",
            Self::Week => "%a %H:%M",
            Self::Month | Self::ThreeMonths => "%b %d",
            Self::Year | Self::All => "%Y-%m-%d",
        }
    }

    fn range_seconds(&self) -> Option<i64> {
        match self {
            Self::Recent(h) => Some(*h as i64 * 3600),
            Self::Yesterday => Some(86400),
            Self::Week => Some(7 * 86400),
            Self::Month => Some(30 * 86400),
            Self::ThreeMonths => Some(90 * 86400),
            Self::Year => Some(365 * 86400),
            Self::All => None,
        }
    }

    pub fn start_end(&self) -> (i64, i64) {
        let now = Utc::now().timestamp();
        match self {
            Self::Yesterday => {
                let today_start = {
                    let local = Local::now();
                    local
                        .date_naive()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_local_timezone(Local)
                        .unwrap()
                        .timestamp()
                };
                (today_start - 86400, today_start)
            }
            Self::All => (0, now),
            _ => {
                let secs = self.range_seconds().unwrap();
                (now - secs, now)
            }
        }
    }

    pub fn display_range(&self) -> (i64, i64) {
        let (start, end) = self.start_end();
        let span = end - start;
        let pad = span / 8;
        (start - pad, end + pad)
    }

    pub fn downsample_interval(&self) -> Option<i64> {
        match self {
            Self::Recent(_) | Self::Yesterday => None,
            Self::Week => Some(300),
            Self::Month => Some(900),
            Self::ThreeMonths => Some(3600),
            Self::Year => Some(7200),
            Self::All => Some(14400),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChartTab {
    Capacity,
    Power,
    Voltage,
    Energy,
    Health,
}

impl ChartTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Capacity => "Capacity",
            Self::Power => "Power",
            Self::Voltage => "Voltage",
            Self::Energy => "Energy",
            Self::Health => "Health",
        }
    }

    const ALL: [ChartTab; 5] = [
        Self::Capacity,
        Self::Power,
        Self::Voltage,
        Self::Energy,
        Self::Health,
    ];
}

pub struct HibatApp {
    config: Config,
    db: Database,
    colors: ThemeColors,

    selected_range: TimeRange,
    selected_tab: ChartTab,
    selected_battery: String,
    available_batteries: Vec<String>,

    records: Vec<BatteryRecord>,
    latest: Option<BatteryRecord>,
    db_stats: (i64, Option<i64>),

    last_refresh: std::time::Instant,
    refresh_interval: std::time::Duration,
    needs_reload: bool,

    refresh_clicked_at: Option<std::time::Instant>,
    collect_clicked_at: Option<std::time::Instant>,
}

impl HibatApp {
    pub fn new(config: Config, db: Database) -> Self {
        let colors = ThemeColors::from_config(&config);
        let refresh_interval = std::time::Duration::from_secs(config.polling_interval_secs.max(10));
        let recent_hours = config.ui.recent_hours;

        let mut app = Self {
            config,
            db,
            colors,
            selected_range: TimeRange::Recent(recent_hours),
            selected_tab: ChartTab::Capacity,
            selected_battery: String::new(),
            available_batteries: Vec::new(),
            records: Vec::new(),
            latest: None,
            db_stats: (0, None),
            last_refresh: std::time::Instant::now(),
            refresh_interval,
            needs_reload: true,
            refresh_clicked_at: None,
            collect_clicked_at: None,
        };

        app.discover_batteries();
        app.reload_data();
        app
    }

    fn discover_batteries(&mut self) {
        let mut batteries = self.db.list_batteries().unwrap_or_default();
        let live = collector::discover_batteries(&self.config.collector.battery_path);
        for b in live {
            if !batteries.contains(&b) {
                batteries.push(b);
            }
        }
        if self.selected_battery.is_empty() || !batteries.contains(&self.selected_battery) {
            self.selected_battery = batteries.first().cloned().unwrap_or_default();
        }
        self.available_batteries = batteries;
    }

    fn reload_data(&mut self) {
        if self.selected_battery.is_empty() {
            self.records.clear();
            self.latest = None;
            return;
        }

        let (start, end) = self.selected_range.start_end();

        self.records = if let Some(interval) = self.selected_range.downsample_interval() {
            self.db
                .query_range_downsampled(&self.selected_battery, start, end, interval)
                .unwrap_or_default()
        } else {
            self.db
                .query_range(&self.selected_battery, start, end)
                .unwrap_or_default()
        };

        self.latest = self.db.query_latest(&self.selected_battery).ok().flatten();
        self.db_stats = (
            self.db.record_count().unwrap_or(0),
            self.db.oldest_timestamp().unwrap_or(None),
        );
        self.last_refresh = std::time::Instant::now();
        self.needs_reload = false;
    }

    fn section_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .inner_margin(Margin::same(SECTION_INNER_MARGIN))
            .outer_margin(Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: SECTION_GAP,
            })
            .corner_radius(SECTION_CORNER_RADIUS)
            .fill(self.colors.surface)
    }

    fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = SIDEBAR_ITEM_SPACING;

        let sidebar_w = ui.available_width();
        ui.set_min_width(sidebar_w);

        ui.vertical_centered(|ui| {
            ui.add_space(4.0);
            ui.heading(
                RichText::new("hibat")
                    .strong()
                    .size(18.0)
                    .color(self.colors.text),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new("Battery History")
                    .size(11.0)
                    .color(self.colors.text_dim),
            );
            ui.add_space(4.0);
        });

        if self.available_batteries.len() > 1 {
            self.section_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    RichText::new("Battery")
                        .strong()
                        .size(12.0)
                        .color(self.colors.text),
                );
                ui.add_space(2.0);
                let prev = self.selected_battery.clone();
                egui::ComboBox::from_id_salt("battery_select")
                    .selected_text(&self.selected_battery)
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for bat in &self.available_batteries {
                            ui.selectable_value(&mut self.selected_battery, bat.clone(), bat);
                        }
                    });
                if self.selected_battery != prev {
                    self.needs_reload = true;
                }
            });
        }

        if let Some(latest) = self.latest.clone() {
            self.section_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    RichText::new("Current Status")
                        .strong()
                        .size(12.0)
                        .color(self.colors.text),
                );
                ui.add_space(4.0);

                egui::Grid::new("status_grid")
                    .num_columns(2)
                    .spacing(GRID_SPACING)
                    .show(ui, |ui| {
                        let dim = self.colors.text_dim;

                        let status_text = latest.status.as_str();
                        let status_color = self.colors.status_color(latest.status);
                        ui.label(RichText::new("Status").color(dim));
                        ui.label(RichText::new(status_text).color(status_color).strong());
                        ui.end_row();

                        ui.label(RichText::new("Charge").color(dim));
                        ui.label(
                            RichText::new(format!("{}%", latest.capacity.unwrap_or(0)))
                                .strong()
                                .size(14.0)
                                .color(self.colors.text),
                        );
                        ui.end_row();

                        if let Some(power) = latest.power_watts() {
                            ui.label(RichText::new("Power").color(dim));
                            ui.label(
                                RichText::new(format!("{:.2} W", power)).color(self.colors.text),
                            );
                            ui.end_row();
                        }

                        if let Some(voltage) = latest.voltage_volts() {
                            ui.label(RichText::new("Voltage").color(dim));
                            ui.label(
                                RichText::new(format!("{:.3} V", voltage)).color(self.colors.text),
                            );
                            ui.end_row();
                        }

                        if let Some(energy) = latest.energy_wh() {
                            ui.label(RichText::new("Energy").color(dim));
                            ui.label(
                                RichText::new(format!("{:.1} Wh", energy)).color(self.colors.text),
                            );
                            ui.end_row();
                        }

                        if let Some(health) = latest.health_pct() {
                            let color = if health > 80.0 {
                                self.colors.health_good
                            } else if health > 50.0 {
                                self.colors.health_warn
                            } else {
                                self.colors.health_bad
                            };
                            ui.label(RichText::new("Health").color(dim));
                            ui.label(RichText::new(format!("{:.1}%", health)).color(color));
                            ui.end_row();
                        }

                        if let Some(cycles) = latest.cycle_count {
                            ui.label(RichText::new("Cycles").color(dim));
                            ui.label(RichText::new(format!("{}", cycles)).color(self.colors.text));
                            ui.end_row();
                        }
                    });
            });
        }

        self.section_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new("Database")
                    .strong()
                    .size(12.0)
                    .color(self.colors.text),
            );
            ui.add_space(4.0);

            let dim = self.colors.text_dim;
            egui::Grid::new("db_grid")
                .num_columns(2)
                .spacing(GRID_SPACING)
                .show(ui, |ui| {
                    ui.label(RichText::new("Records").color(dim));
                    ui.label(RichText::new(format!("{}", self.db_stats.0)).color(self.colors.text));
                    ui.end_row();

                    ui.label(RichText::new("In view").color(dim));
                    ui.label(
                        RichText::new(format!("{}", self.records.len())).color(self.colors.text),
                    );
                    ui.end_row();

                    if let Some(oldest) = self.db_stats.1 {
                        let dt =
                            chrono::DateTime::<Utc>::from_timestamp(oldest, 0).unwrap_or_default();
                        let local: chrono::DateTime<Local> = dt.into();
                        ui.label(RichText::new("Since").color(dim));
                        ui.label(
                            RichText::new(local.format("%Y-%m-%d").to_string())
                                .color(self.colors.text),
                        );
                        ui.end_row();
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let btn_width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;

            let refresh_fill = flash_color(
                self.refresh_clicked_at,
                self.colors.accent,
                self.colors.surface,
            );
            let refresh_stroke = flash_color(
                self.refresh_clicked_at,
                self.colors.accent,
                self.colors.border,
            );
            let refresh_btn = egui::Button::new(RichText::new("Refresh").color(self.colors.text))
                .fill(refresh_fill)
                .stroke(Stroke::new(1.0, refresh_stroke))
                .corner_radius(5.0);

            if ui.add_sized([btn_width, 28.0], refresh_btn).clicked() {
                self.refresh_clicked_at = Some(std::time::Instant::now());
                self.needs_reload = true;
            }

            let collect_fill = flash_color(
                self.collect_clicked_at,
                self.colors.accent,
                self.colors.surface,
            );
            let collect_stroke = flash_color(
                self.collect_clicked_at,
                self.colors.accent,
                self.colors.border,
            );
            let collect_btn = egui::Button::new(RichText::new("Collect").color(self.colors.text))
                .fill(collect_fill)
                .stroke(Stroke::new(1.0, collect_stroke))
                .corner_radius(5.0);

            if ui.add_sized([btn_width, 28.0], collect_btn).clicked() {
                self.collect_clicked_at = Some(std::time::Instant::now());
                let new_records = collector::collect_all(&self.config.collector.battery_path);
                for record in &new_records {
                    let _ = self.db.insert(record);
                }
                self.needs_reload = true;
            }

            if is_flashing(self.refresh_clicked_at) || is_flashing(self.collect_clicked_at) {
                ui.ctx().request_repaint();
            }
        });
    }

    fn draw_main_area(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);

        let tab_count = ChartTab::ALL.len() as f32;
        let tab_spacing = 6.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tab_spacing;
            let available = ui.available_width();
            let btn_w = (available - tab_spacing * (tab_count - 1.0)) / tab_count;
            let btn_h = 32.0;

            for tab in &ChartTab::ALL {
                let is_selected = self.selected_tab == *tab;
                let text = RichText::new(tab.label()).size(14.0);
                let (text, fill, stroke) = if is_selected {
                    (
                        text.strong().color(self.colors.text),
                        lighten(self.colors.surface, 15),
                        Stroke::new(1.5, self.colors.accent),
                    )
                } else {
                    (
                        text.color(self.colors.text),
                        self.colors.surface,
                        Stroke::new(1.0, self.colors.border),
                    )
                };

                let btn = egui::Button::new(text)
                    .corner_radius(5.0)
                    .fill(fill)
                    .stroke(stroke);

                if ui.add_sized([btn_w, btn_h], btn).clicked() {
                    self.selected_tab = *tab;
                }
            }
        });

        let recent_hours = self.config.ui.recent_hours;
        let ranges = [
            TimeRange::Recent(recent_hours),
            TimeRange::Yesterday,
            TimeRange::Week,
            TimeRange::Month,
            TimeRange::ThreeMonths,
            TimeRange::Year,
            TimeRange::All,
        ];
        let range_count = ranges.len() as f32;
        let range_spacing = 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = range_spacing;
            let available = ui.available_width();
            let btn_w = (available - range_spacing * (range_count - 1.0)) / range_count;
            let btn_h = 26.0;

            for range in &ranges {
                let is_selected = self.selected_range == *range;
                let text = RichText::new(range.label()).size(12.0);
                let (text, fill, stroke) = if is_selected {
                    (
                        text.strong().color(self.colors.text),
                        lighten(self.colors.surface, 15),
                        Stroke::new(1.0, self.colors.full),
                    )
                } else {
                    (
                        text.color(self.colors.text),
                        self.colors.surface,
                        Stroke::new(1.0, self.colors.border),
                    )
                };

                let btn = egui::Button::new(text)
                    .corner_radius(4.0)
                    .fill(fill)
                    .stroke(stroke);

                if ui.add_sized([btn_w, btn_h], btn).clicked() {
                    self.selected_range = *range;
                    self.needs_reload = true;
                }
            }
        });

        ui.add_space(4.0);

        let chart_frame = egui::Frame::new()
            .inner_margin(Margin::same(8))
            .corner_radius(SECTION_CORNER_RADIUS)
            .fill(self.colors.surface);

        let x_range = self.selected_range.display_range();

        chart_frame.show(ui, |ui| {
            let time_fmt = self.selected_range.time_format();
            let line_width = self.config.ui.chart_line_width;

            match self.selected_tab {
                ChartTab::Capacity => {
                    charts::draw_capacity_chart(
                        ui,
                        &self.records,
                        &self.colors,
                        line_width,
                        time_fmt,
                        x_range,
                    );
                }
                ChartTab::Power => {
                    charts::draw_power_chart(
                        ui,
                        &self.records,
                        &self.colors,
                        line_width,
                        time_fmt,
                        x_range,
                    );
                }
                ChartTab::Voltage => {
                    charts::draw_voltage_chart(
                        ui,
                        &self.records,
                        &self.colors,
                        line_width,
                        time_fmt,
                        x_range,
                    );
                }
                ChartTab::Energy => {
                    charts::draw_energy_chart(
                        ui,
                        &self.records,
                        &self.colors,
                        line_width,
                        time_fmt,
                        x_range,
                    );
                }
                ChartTab::Health => {
                    charts::draw_health_chart(
                        ui,
                        &self.records,
                        &self.colors,
                        line_width,
                        time_fmt,
                        x_range,
                    );
                }
            }
        });
    }
}

impl eframe::App for HibatApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.last_refresh.elapsed() > self.refresh_interval {
            self.needs_reload = true;
        }
        if self.needs_reload {
            self.reload_data();
        }

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = self.colors.background;
        visuals.window_fill = self.colors.background;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.0, Color32::TRANSPARENT);
        visuals.override_text_color = Some(self.colors.text);
        ui.ctx().set_visuals(visuals);

        let mut style = (*ui.ctx().global_style()).clone();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(14.0));
        ui.ctx().set_global_style(style);

        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(220.0)
            .show_inside(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.draw_sidebar(ui);
                            });
                    });
            });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 12,
                    right: 12,
                    top: 8,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    self.draw_main_area(ui);
                });
        });

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(2));
    }
}

const FLASH_DURATION_MS: u128 = 350;

fn flash_color(clicked_at: Option<std::time::Instant>, flash: Color32, base: Color32) -> Color32 {
    let Some(t) = clicked_at else {
        return base;
    };
    let elapsed = t.elapsed().as_millis();
    if elapsed >= FLASH_DURATION_MS {
        return base;
    }
    let t = elapsed as f32 / FLASH_DURATION_MS as f32;
    lerp_color(flash, base, t)
}

fn is_flashing(clicked_at: Option<std::time::Instant>) -> bool {
    clicked_at.is_some_and(|t| t.elapsed().as_millis() < FLASH_DURATION_MS)
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let inv = 1.0 - t;
    Color32::from_rgb(
        (a.r() as f32 * inv + b.r() as f32 * t) as u8,
        (a.g() as f32 * inv + b.g() as f32 * t) as u8,
        (a.b() as f32 * inv + b.b() as f32 * t) as u8,
    )
}

fn lighten(color: Color32, amount: u8) -> Color32 {
    Color32::from_rgb(
        color.r().saturating_add(amount),
        color.g().saturating_add(amount),
        color.b().saturating_add(amount),
    )
}
