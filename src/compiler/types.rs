/// Triangle mesh data ready for Bevy rendering.
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Optional color set via `color()` in the `OpenSCAD` code.
    pub color: Option<[f32; 3]>,
}

/// A rendered orthographic view encoded as base64 PNG.
pub struct ViewImage {
    pub label: String,
    pub base64_png: String,
}

pub enum CompilationResult {
    Success {
        parts: Vec<MeshData>,
        views: Vec<ViewImage>,
        warnings: Vec<String>,
    },
    Error(String),
}
