use chrono::{DateTime, Local, Utc};
use egui::{self, Color32, Stroke, Ui};
use egui_plot::{uniform_grid_spacer, GridInput, GridMark, Line, Plot};

use crate::config::Config;
use crate::db::{BatteryRecord, BatteryStatus};

fn format_ts(ts: f64, fmt: &str) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts as i64, 0).unwrap_or_default();
    let local: DateTime<Local> = dt.into();
    local.format(fmt).to_string()
}

fn time_grid_spacer(input: GridInput) -> Vec<GridMark> {
    const INTERVALS: &[f64] = &[
        60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0, 7200.0, 14400.0, 21600.0, 43200.0,
        86400.0, 172800.0, 604800.0, 2592000.0,
    ];

    let (min, max) = input.bounds;
    let range = max - min;

    let target_step = range / 6.0;

    let step = INTERVALS
        .iter()
        .copied()
        .find(|&s| s >= target_step)
        .unwrap_or(2592000.0);

    let mut marks = Vec::new();
    let first = (min / step).floor() as i64;
    let last = (max / step).ceil() as i64;
    for i in first..=last {
        let value = i as f64 * step;
        if value >= min && value <= max {
            marks.push(GridMark {
                value,
                step_size: step,
            });
        }
    }

    marks
}

pub struct ThemeColors {
    pub background: Color32,
    pub surface: Color32,
    pub accent: Color32,
    pub border: Color32,

    pub text: Color32,
    pub text_dim: Color32,

    pub charge: Color32,
    pub discharge: Color32,
    pub full: Color32,
    pub health_good: Color32,
    pub health_warn: Color32,
    pub health_bad: Color32,
}

impl ThemeColors {
    pub fn from_config(config: &Config) -> Self {
        Self {
            background: Config::parse_color(&config.theme.background),
            surface: Config::parse_color(&config.theme.surface_color),
            accent: Config::parse_color(&config.theme.accent_color),
            border: Config::parse_color(&config.theme.border_color),
            text: Config::parse_color(&config.theme.text_color),
            text_dim: Config::parse_color(&config.theme.text_dim_color),
            charge: Config::parse_color(&config.theme.charge_color),
            discharge: Config::parse_color(&config.theme.discharge_color),
            full: Config::parse_color(&config.theme.full_color),
            health_good: Config::parse_color(&config.theme.health_good_color),
            health_warn: Config::parse_color(&config.theme.health_warn_color),
            health_bad: Config::parse_color(&config.theme.health_bad_color),
        }
    }

    pub fn status_color(&self, status: BatteryStatus) -> Color32 {
        match status {
            BatteryStatus::Charging => self.charge,
            BatteryStatus::Discharging => self.discharge,
            BatteryStatus::Full => self.full,
            _ => self.text,
        }
    }
}

pub fn draw_capacity_chart(
    ui: &mut Ui,
    records: &[BatteryRecord],
    colors: &ThemeColors,
    line_width: f32,
    time_fmt: &str,
    x_range: (i64, i64),
) {
    if records.is_empty() {
        centered_empty_label(ui, "No data for this period", colors.text_dim);
        return;
    }

    let segments = build_status_segments(records);

    base_plot(
        "capacity_chart",
        ui,
        x_range,
        time_fmt,
        "Capacity %",
        |v| format!("{:.0}%", v),
        uniform_grid_spacer(|_| [10.0, 50.0, 100.0]),
    )
    .include_y(0.0)
    .include_y(100.0)
    .show(ui, |plot_ui| {
        for (status, points) in &segments {
            let color = match status {
                BatteryStatus::Charging => colors.charge,
                _ => colors.charge,
            };
            let line = Line::new(status.to_string(), points.clone())
                .stroke(Stroke::new(line_width, color));
            plot_ui.line(line);
        }
    });
}

pub fn draw_power_chart(
    ui: &mut Ui,
    records: &[BatteryRecord],
    colors: &ThemeColors,
    line_width: f32,
    time_fmt: &str,
    x_range: (i64, i64),
) {
    if records.is_empty() {
        centered_empty_label(ui, "No data for this period", colors.text_dim);
        return;
    }

    let points: Vec<[f64; 2]> = records
        .iter()
        .filter_map(|r| r.power_watts().map(|p| [r.timestamp as f64, p]))
        .collect();

    if points.is_empty() {
        centered_empty_label(ui, "No power data available", colors.text_dim);
        return;
    }

    base_plot(
        "power_chart",
        ui,
        x_range,
        time_fmt,
        "Power (W)",
        |v| format!("{:.2}W", v),
        uniform_grid_spacer(|_| [1.0, 5.0, 10.0]),
    )
    .include_y(0.0)
    .show(ui, |plot_ui| {
        let line = Line::new("Power".to_string(), points)
            .stroke(Stroke::new(line_width, colors.discharge));
        plot_ui.line(line);
    });
}

pub fn draw_voltage_chart(
    ui: &mut Ui,
    records: &[BatteryRecord],
    colors: &ThemeColors,
    line_width: f32,
    time_fmt: &str,
    x_range: (i64, i64),
) {
    if records.is_empty() {
        centered_empty_label(ui, "No data for this period", colors.text_dim);
        return;
    }

    let points: Vec<[f64; 2]> = records
        .iter()
        .filter_map(|r| r.voltage_volts().map(|v| [r.timestamp as f64, v]))
        .collect();

    if points.is_empty() {
        centered_empty_label(ui, "No voltage data available", colors.text_dim);
        return;
    }

    base_plot(
        "voltage_chart",
        ui,
        x_range,
        time_fmt,
        "Voltage (V)",
        |v| format!("{:.3}V", v),
        uniform_grid_spacer(|_| [0.1, 0.5, 1.0]),
    )
    .show(ui, |plot_ui| {
        let line =
            Line::new("Voltage".to_string(), points).stroke(Stroke::new(line_width, colors.full));
        plot_ui.line(line);
    });
}

pub fn draw_health_chart(
    ui: &mut Ui,
    records: &[BatteryRecord],
    colors: &ThemeColors,
    line_width: f32,
    time_fmt: &str,
    x_range: (i64, i64),
) {
    if records.is_empty() {
        centered_empty_label(ui, "No data for this period", colors.text_dim);
        return;
    }

    let points: Vec<[f64; 2]> = records
        .iter()
        .filter_map(|r| r.health_pct().map(|h| [r.timestamp as f64, h]))
        .collect();

    if points.is_empty() {
        centered_empty_label(ui, "No health data available", colors.text_dim);
        return;
    }

    let avg_health = points.iter().map(|p| p[1]).sum::<f64>() / points.len() as f64;
    let color = if avg_health > 80.0 {
        colors.health_good
    } else if avg_health > 50.0 {
        colors.health_warn
    } else {
        colors.health_bad
    };

    base_plot(
        "health_chart",
        ui,
        x_range,
        time_fmt,
        "Health %",
        |v| format!("{:.1}%", v),
        uniform_grid_spacer(|_| [5.0, 10.0, 50.0]),
    )
    .include_y(50.0)
    .include_y(105.0)
    .show(ui, |plot_ui| {
        let line =
            Line::new("Battery Health".to_string(), points).stroke(Stroke::new(line_width, color));
        plot_ui.line(line);
    });
}

pub fn draw_energy_chart(
    ui: &mut Ui,
    records: &[BatteryRecord],
    colors: &ThemeColors,
    line_width: f32,
    time_fmt: &str,
    x_range: (i64, i64),
) {
    if records.is_empty() {
        centered_empty_label(ui, "No data for this period", colors.text_dim);
        return;
    }

    let energy_points: Vec<[f64; 2]> = records
        .iter()
        .filter_map(|r| r.energy_wh().map(|e| [r.timestamp as f64, e]))
        .collect();

    let full_points: Vec<[f64; 2]> = records
        .iter()
        .filter_map(|r| r.energy_full_wh().map(|e| [r.timestamp as f64, e]))
        .collect();

    if energy_points.is_empty() {
        centered_empty_label(ui, "No energy data available", colors.text_dim);
        return;
    }

    base_plot(
        "energy_chart",
        ui,
        x_range,
        time_fmt,
        "Energy (Wh)",
        |v| format!("{:.2}Wh", v),
        uniform_grid_spacer(|_| [5.0, 10.0, 50.0]),
    )
    .include_y(0.0)
    .show(ui, |plot_ui| {
        let line = Line::new("Energy Now".to_string(), energy_points)
            .stroke(Stroke::new(line_width, colors.charge));
        plot_ui.line(line);

        if !full_points.is_empty() {
            let full_line = Line::new("Energy Full".to_string(), full_points)
                .stroke(Stroke::new(line_width * 0.5, colors.full));
            plot_ui.line(full_line);
        }
    });
}

fn base_plot(
    id: &str,
    ui: &Ui,
    x_range: (i64, i64),
    time_fmt: &str,
    y_label: &str,
    y_fmt: fn(f64) -> String,
    y_spacer: Box<dyn Fn(egui_plot::GridInput) -> Vec<GridMark>>,
) -> Plot<'static> {
    let tfmt = time_fmt.to_string();
    let tfmt2 = time_fmt.to_string();

    Plot::new(id.to_string())
        .height(ui.available_height())
        .include_x(x_range.0 as f64)
        .include_x(x_range.1 as f64)
        .x_grid_spacer(time_grid_spacer)
        .y_grid_spacer(y_spacer)
        .y_axis_label(egui::RichText::new(y_label.to_string()).size(15.0))
        .x_axis_formatter(move |mark, _range| format_ts(mark.value, &tfmt))
        .y_axis_formatter(move |mark, _range| y_fmt(mark.value))
        .label_formatter(move |_name, value| {
            format!("{}\n{}", format_ts(value.x, &tfmt2), y_fmt(value.y))
        })
}

fn centered_empty_label(ui: &mut Ui, text: &str, color: Color32) {
    let available = ui.available_size();
    ui.allocate_ui_with_layout(
        available,
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
        |ui| {
            ui.label(egui::RichText::new(text).size(16.0).color(color));
        },
    );
}

fn build_status_segments(records: &[BatteryRecord]) -> Vec<(BatteryStatus, Vec<[f64; 2]>)> {
    if records.is_empty() {
        return vec![];
    }

    let mut segments: Vec<(BatteryStatus, Vec<[f64; 2]>)> = Vec::new();
    let mut current_status = records[0].status;
    let mut current_points: Vec<[f64; 2]> = vec![];

    for record in records {
        if record.status != current_status {
            let point = [record.timestamp as f64, record.capacity_pct()];
            current_points.push(point);
            segments.push((current_status, std::mem::take(&mut current_points)));
            current_status = record.status;
            current_points.push(point);
        } else {
            current_points.push([record.timestamp as f64, record.capacity_pct()]);
        }
    }

    if !current_points.is_empty() {
        segments.push((current_status, current_points));
    }

    segments
}
