use egui::{Pos2, Rect};

use crate::camera::CameraFrame;
use crate::model::Vec3;

#[derive(Debug, Clone)]
pub struct ScreenProjector {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    rect: Rect,
    camera: CameraFrame,
}

impl ScreenProjector {
    pub fn new(points: &[Vec3], rect: Rect, camera: CameraFrame) -> Self {
        let projected = points
            .iter()
            .map(|point| project_world(*point, camera))
            .collect::<Vec<_>>();
        let min_x = projected.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let max_x = projected.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let min_y = projected.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let max_y = projected.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        Self {
            min_x,
            max_x,
            min_y,
            max_y,
            rect,
            camera,
        }
    }

    pub fn project(&self, point: Vec3) -> Pos2 {
        let (sx, sy) = project_world(point, self.camera);
        let span_x = (self.max_x - self.min_x).max(1.0);
        let span_y = (self.max_y - self.min_y).max(1.0);
        let nx = (sx - self.min_x) / span_x;
        let ny = (sy - self.min_y) / span_y;
        let margin = 52.0;
        Pos2::new(
            self.rect.left() + margin + nx * (self.rect.width() - margin * 2.0).max(1.0),
            self.rect.top() + margin + ny * (self.rect.height() - margin * 2.0).max(1.0),
        )
    }
}

fn project_world(point: Vec3, camera: CameraFrame) -> (f32, f32) {
    let (sin_yaw, cos_yaw) = camera.yaw.sin_cos();
    let rx = point.x * cos_yaw - point.z * sin_yaw;
    let rz = point.x * sin_yaw + point.z * cos_yaw;

    let (sin_pitch, cos_pitch) = camera.pitch.sin_cos();
    let ry = point.y * cos_pitch - rz * sin_pitch;
    let depth = point.y * sin_pitch + rz * cos_pitch;

    let perspective = 1.0 / (1.0 + depth.max(-100.0) * 0.0018);
    (
        rx * camera.zoom * perspective,
        -ry * camera.zoom * perspective,
    )
}
