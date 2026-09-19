use eframe::egui;
use std::f32::consts::TAU;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size([150.0, 150.0])
            .with_mouse_passthrough(true)
            .with_taskbar(false)
            .with_position([1750.0, 20.0]), // Top Right Corner
        ..Default::default()
    };

    let app = SiriWidget::new();

    eframe::run_native("Aish Audio Ring", options, Box::new(|_cc| Box::new(app)))
}

struct SiriWidget {
    start_time: Instant,
    amplitude: Arc<Mutex<f32>>,
    visual_amp: f32,
}

impl SiriWidget {
    fn new() -> Self {
        let amplitude = Arc::new(Mutex::new(0.0));
        let amp_clone = Arc::clone(&amplitude);

        thread::spawn(move || {
            if let Ok(socket) = UdpSocket::bind("127.0.0.1:9999") {
                let mut buf = [0; 4];
                loop {
                    if let Ok((size, _)) = socket.recv_from(&mut buf) {
                        if size == 4 {
                            let val = f32::from_ne_bytes(buf);
                            if let Ok(mut a) = amp_clone.lock() {
                                *a = val;
                            }
                        }
                    }
                }
            }
        });

        Self {
            start_time: Instant::now(),
            amplitude,
            visual_amp: 0.0,
        }
    }
}

impl eframe::App for SiriWidget {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let time = self.start_time.elapsed().as_secs_f32();

        let target_amp = *self.amplitude.lock().unwrap();
        self.visual_amp += (target_amp - self.visual_amp) * 0.3;
        let react = (self.visual_amp * 40.0).clamp(0.0, 15.0);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let (rect, _response) =
                    ui.allocate_exact_size(egui::vec2(150.0, 150.0), egui::Sense::hover());

                let painter = ui.painter();
                let center = rect.center();

                let num_points = 120;
                let base_radius = 38.0;
                let mut points = Vec::with_capacity(num_points);

                for i in 0..num_points {
                    let angle = (i as f32 / num_points as f32) * TAU;
                    let ripple1 = (angle * 5.0 - time * 8.0).sin();
                    let ripple2 = (angle * 8.0 + time * 5.0).cos();
                    let wave = ripple1 * 0.6 + ripple2 * 0.4;
                    let current_radius = base_radius + (wave * react);

                    let x = center.x + angle.cos() * current_radius;
                    let y = center.y + angle.sin() * current_radius;

                    points.push(egui::pos2(x, y));
                }

                let path_purple = egui::Shape::closed_line(
                    points.clone(),
                    egui::Stroke::new(
                        9.0,
                        egui::Color32::from_rgba_unmultiplied(200, 45, 255, 120),
                    ),
                );

                let path_cyan = egui::Shape::closed_line(
                    points.clone(),
                    egui::Stroke::new(
                        5.0,
                        egui::Color32::from_rgba_unmultiplied(45, 200, 255, 180),
                    ),
                );

                let path_core =
                    egui::Shape::closed_line(points, egui::Stroke::new(2.0, egui::Color32::WHITE));

                painter.add(path_purple);
                painter.add(path_cyan);
                painter.add(path_core);

                painter.circle_filled(
                    center,
                    base_radius - 1.5,
                    egui::Color32::from_rgba_unmultiplied(12, 12, 16, 255),
                );
            });

        ctx.request_repaint();
    }
}
