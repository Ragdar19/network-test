use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use eframe::egui::{self};
use egui_plot::{AxisHints, Line, Plot, PlotPoints};
use plotters::prelude::*;
use plotters::{
    chart::ChartBuilder,
    style::{IntoFont, WHITE},
};
use std::sync::mpsc;
use std::thread;
use std::{env, fmt::Debug, fs::File, io::Write, path::Path, process::Command, str::from_utf8};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let ping_ip = args[1].clone();
    let iterations = match args[2].parse::<i32>() {
        Ok(i) => i,
        Err(err) => panic!("Error parsing: {err}"),
    };
    println!("Running ping to {ping_ip}");

    // Create a channel for sending ping data
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut i: i32 = 0;
        while i < iterations {
            let ping_value = get_ping(&ping_ip).unwrap();
            if tx.send(ping_value).is_err() {
                break; // Exit if receiver is dropped
            }
            i += 1;
        }
    });

    // export_to_csv(file_path, ping_average_values);

    draw_chart_realtime(rx)
}

fn get_ping(ping_ip: &str) -> Result<PingResult, std::io::Error> {
    let output = Command::new("ping")
        .arg(ping_ip)
        .arg("-c 1")
        .output()?;

    let output_as_str = match from_utf8(&output.stdout) {
        Ok(s) => s,
        Err(err) => panic!("Invalid UTF-8 sequence: {err}"),
    };

    parse_ping(output_as_str)
}

fn parse_ping(ping_output: &str) -> Result<PingResult, std::io::Error> {
    let stats_index = match ping_output.rfind("min/avg/max/stddev") {
        Some(i) => i,
        None => panic!("Sequence not found"),
    };

    let stats_output = &ping_output[stats_index..];

    let mut stats_splitted = stats_output.split(" = ");

    stats_splitted.next(); // Headers

    let stats_values: Vec<&str> = match stats_splitted.next() {
        Some(values) => values.split("/").collect(),
        None => panic!("Stats values not found"),
    };

    match stats_values[1].parse() {
        Ok(average) => return Ok(PingResult::new(average, Utc::now())),
        Err(err) => panic!("Error parsing average value: {err}"),
    }
}

fn export_to_csv(file_path: &str, values: Vec<f64>) {
    let path = Path::new(file_path);
    if path.exists() {
        panic!("File {file_path} already exists.")
    }

    let to_write = values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<String>>()
        .join("\n");

    let mut file = match File::create(file_path) {
        Ok(f) => f,
        Err(err) => panic!("Error creating file: {err}"),
    };

    match file.write_all(to_write.as_bytes()) {
        Ok(it) => it,
        Err(err) => panic!("Error creating file: {err}"),
    };
}

fn draw_chart_png(ping_data: Vec<f64>) -> Result<(), Box<dyn std::error::Error>> {
    // Prepare the drawing area
    let root = BitMapBackend::new("ping_graph.png", (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    // Prepare a chart context
    let mut chart = ChartBuilder::on(&root)
        .caption("Network Ping Monitoring", ("Arial", 30).into_font())
        .margin(5)
        .x_label_area_size(30)
        .y_label_area_size(50)
        .build_cartesian_2d(0..100, 0.0..200.0)?;

    chart.configure_mesh().draw()?;

    chart.draw_series(LineSeries::new(
        ping_data.iter().enumerate().map(|(x, &y)| (x as i32, y)),
        &RED,
    ))?;

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;

    Ok(())
}

fn draw_chart_realtime(
    ping_receiver: mpsc::Receiver<PingResult>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set up eframe options
    let options = eframe::NativeOptions {
        ..Default::default()
    };

    // Run the application
    eframe::run_native(
        "Network Ping Monitor",
        options,
        Box::new(|_cc| Ok(Box::new(PingApp::new(ping_receiver)))),
    )?;

    Ok(())
}

struct PingResult {
    average: f64,
    datetime_recv: DateTime<Utc>
}

impl PingResult {
    fn new(average: f64, datetime_recv: DateTime<Utc>) -> PingResult {
        PingResult { average, datetime_recv }
    }
}

impl Debug for PingResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PingResult")
            .field("average", &self.average)
            .finish()
    }
}

struct PingApp {
    ping_receiver: mpsc::Receiver<PingResult>,
    ping_data: Vec<PingResult>,
    max_points: usize
}

impl PingApp {
    fn new(ping_receiver: mpsc::Receiver<PingResult>) -> Self {
        PingApp {
            ping_receiver,
            ping_data: vec![],
            max_points: 1000
        }
    }
}

impl eframe::App for PingApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Try to receive new ping data without blocking
        while let Ok(ping_value) = self.ping_receiver.try_recv() {
            self.ping_data.push(ping_value);
        }

        // Limit the number of points
        if self.ping_data.len() > self.max_points {
            self.ping_data.remove(0);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let plot = Plot::new("ping_plot")
                .view_aspect(2.0)
                .x_axis_label("Time")
                .y_axis_label("Ping (ms)");
                // .custom_x_axes(vec![AxisHints::new_x().formatter(|x, _range| {
                //     // Convert the x value (seconds since epoch) back to DateTime
                //     let dt = Utc.timestamp_opt(x.value as i64, 0).unwrap();
                //     // Format the time as desired
                //     dt.format("%H:%M:%S").to_string()
                // }).label_spacing(10.0..=20.0)]);

            plot.show(ui, |plot_ui| {
                // Convert ping data to plot points
                let points = PlotPoints::from_ys_f64(&self.ping_data.iter().map(|pr| { pr.average} ).collect::<Vec<f64>>().clone());
                // let points = PlotPoints::new(self.ping_data.iter()
                // .map(|data| {
                //     [
                //         data.datetime_recv.timestamp() as f64,
                //         data.average
                //     ]
                // }).collect());

                // Add the line to the plot
                plot_ui.line(Line::new(points));
            });
        });

        // Request a repaint to ensure continuous updates
        ctx.request_repaint();
    }
}
