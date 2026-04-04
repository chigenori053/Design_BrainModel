use crate::model::{CameraMode, CameraPreset3D};

#[derive(Debug, Clone, Copy)]
pub struct CameraFrame {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}

pub fn frame_for_preset(preset: &CameraPreset3D, time: f64) -> CameraFrame {
    match preset.mode {
        CameraMode::Architectural => CameraFrame {
            yaw: 0.72,
            pitch: 0.48,
            zoom: 18.0,
        },
        CameraMode::RuntimeFlow => CameraFrame {
            yaw: 0.92 + (time as f32 * 0.15).sin() * 0.02,
            pitch: 0.36,
            zoom: 19.5,
        },
        CameraMode::RefactorPreview => CameraFrame {
            yaw: 0.64,
            pitch: 0.52,
            zoom: 17.2,
        },
    }
}
