#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalityKind {
    Text,
    Image,
    Audio,
    Structured,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageBuffer {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate_hz: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModalityInput {
    Text(String),
    Image(ImageBuffer),
    Audio(AudioBuffer),
    Structured(serde_json::Value),
}

impl Default for ModalityInput {
    fn default() -> Self {
        Self::Structured(serde_json::Value::Null)
    }
}

impl ModalityInput {
    pub fn kind(&self) -> ModalityKind {
        match self {
            Self::Text(_) => ModalityKind::Text,
            Self::Image(_) => ModalityKind::Image,
            Self::Audio(_) => ModalityKind::Audio,
            Self::Structured(_) => ModalityKind::Structured,
        }
    }

    pub fn accepted_modalities() -> [ModalityKind; 4] {
        [
            ModalityKind::Text,
            ModalityKind::Image,
            ModalityKind::Audio,
            ModalityKind::Structured,
        ]
    }
}

impl ModalityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Structured => "structured",
        }
    }
}
