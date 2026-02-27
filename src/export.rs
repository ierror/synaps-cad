use crate::plugins::compilation::StlMeshData;
use std::io::Write;
use std::path::Path;

/// Supported export formats.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Stl,
    Obj,
    ThreeMf,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Stl => "stl",
            Self::Obj => "obj",
            Self::ThreeMf => "3mf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stl => "STL",
            Self::Obj => "OBJ (with colors)",
            Self::ThreeMf => "3MF (with colors)",
        }
    }
}

pub const ALL_FORMATS: &[ExportFormat] =
    &[ExportFormat::ThreeMf, ExportFormat::Obj, ExportFormat::Stl];

/// Export parts to the given path in the specified format.
pub fn export_parts(
    parts: &[StlMeshData],
    path: &Path,
    format: ExportFormat,
) -> Result<(), String> {
    match format {
        ExportFormat::Stl => export_stl(parts, path),
        ExportFormat::Obj => export_obj(parts, path),
        ExportFormat::ThreeMf => export_3mf(parts, path),
    }
}

/// Binary STL export (no colors).
fn export_stl(parts: &[StlMeshData], path: &Path) -> Result<(), String> {
    let mut file =
        std::fs::File::create(path).map_err(|e| format!("Failed to create file: {e}"))?;

    // Count total triangles
    let total_triangles: u32 = parts.iter().map(|p| (p.indices.len() / 3) as u32).sum();

    // 80-byte header
    let mut header = [0u8; 80];
    let label = b"SynapsCAD STL Export";
    header[..label.len()].copy_from_slice(label);
    file.write_all(&header).map_err(|e| e.to_string())?;
    file.write_all(&total_triangles.to_le_bytes())
        .map_err(|e| e.to_string())?;

    for part in parts {
        for tri in part.indices.chunks(3) {
            let (v0, v1, v2) = (
                part.positions[tri[0] as usize],
                part.positions[tri[1] as usize],
                part.positions[tri[2] as usize],
            );
            // Compute face normal
            let u = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let v = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let normal = if len > 0.0 {
                [n[0] / len, n[1] / len, n[2] / len]
            } else {
                [0.0, 0.0, 1.0]
            };

            // Normal (3 × f32)
            for &c in &normal {
                file.write_all(&c.to_le_bytes())
                    .map_err(|e| e.to_string())?;
            }
            // Vertices (3 × 3 × f32)
            for vtx in &[v0, v1, v2] {
                for &c in vtx {
                    file.write_all(&c.to_le_bytes())
                        .map_err(|e| e.to_string())?;
                }
            }
            // Attribute byte count
            file.write_all(&0u16.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// OBJ + MTL export (with per-part colors).
fn export_obj(parts: &[StlMeshData], path: &Path) -> Result<(), String> {
    let mtl_path = path.with_extension("mtl");
    let mtl_filename = mtl_path.file_name().unwrap_or_default().to_string_lossy();

    let mut obj =
        std::fs::File::create(path).map_err(|e| format!("Failed to create OBJ file: {e}"))?;
    let mut mtl =
        std::fs::File::create(&mtl_path).map_err(|e| format!("Failed to create MTL file: {e}"))?;

    writeln!(obj, "# SynapsCAD OBJ Export").map_err(|e| e.to_string())?;
    writeln!(obj, "mtllib {mtl_filename}").map_err(|e| e.to_string())?;
    writeln!(mtl, "# SynapsCAD MTL Export").map_err(|e| e.to_string())?;

    let mut vertex_offset = 1usize; // OBJ indices are 1-based

    for (i, part) in parts.iter().enumerate() {
        let mat_name = format!("part_{}", i + 1);
        let color = part.color.unwrap_or([0.7, 0.7, 0.7]);

        // Write material
        writeln!(mtl, "newmtl {mat_name}").map_err(|e| e.to_string())?;
        writeln!(mtl, "Kd {} {} {}", color[0], color[1], color[2]).map_err(|e| e.to_string())?;
        writeln!(mtl, "Ka 0.1 0.1 0.1").map_err(|e| e.to_string())?;
        writeln!(mtl, "Ks 0.3 0.3 0.3").map_err(|e| e.to_string())?;
        writeln!(mtl, "Ns 100.0").map_err(|e| e.to_string())?;
        writeln!(mtl, "d 1.0").map_err(|e| e.to_string())?;

        // Write object group
        writeln!(obj, "o part_{}", i + 1).map_err(|e| e.to_string())?;
        writeln!(obj, "usemtl {mat_name}").map_err(|e| e.to_string())?;

        // Write vertices
        for pos in &part.positions {
            writeln!(obj, "v {} {} {}", pos[0], pos[1], pos[2]).map_err(|e| e.to_string())?;
        }

        // Write normals
        for n in &part.normals {
            writeln!(obj, "vn {} {} {}", n[0], n[1], n[2]).map_err(|e| e.to_string())?;
        }

        // Write faces
        for tri in part.indices.chunks(3) {
            let (a, b, c) = (
                tri[0] as usize + vertex_offset,
                tri[1] as usize + vertex_offset,
                tri[2] as usize + vertex_offset,
            );
            writeln!(obj, "f {a}//{a} {b}//{b} {c}//{c}").map_err(|e| e.to_string())?;
        }

        vertex_offset += part.positions.len();
    }

    Ok(())
}

/// 3MF export (with per-part colors via `ColorGroup`).
fn export_3mf(parts: &[StlMeshData], path: &Path) -> Result<(), String> {
    use lib3mf::{BuildItem, Mesh, Model, Object, Triangle, Vertex};

    let mut model = Model::new();
    let color_group_id = 1usize;

    // Collect unique colors and build a ColorGroup
    let mut colors: Vec<(u8, u8, u8, u8)> = Vec::new();
    let mut part_color_indices: Vec<Option<usize>> = Vec::new();

    for part in parts {
        if let Some(c) = part.color {
            let rgba = (
                (c[0] * 255.0).round() as u8,
                (c[1] * 255.0).round() as u8,
                (c[2] * 255.0).round() as u8,
                255u8,
            );
            let idx = colors
                .iter()
                .position(|&existing| existing == rgba)
                .unwrap_or_else(|| {
                    colors.push(rgba);
                    colors.len() - 1
                });
            part_color_indices.push(Some(idx));
        } else {
            part_color_indices.push(None);
        }
    }

    // Add color group if we have any colors
    if !colors.is_empty() {
        let cg = lib3mf::ColorGroup {
            id: color_group_id,
            colors: colors.clone(),
            parse_order: 0,
        };
        model.resources.color_groups.push(cg);
    }

    for (i, part) in parts.iter().enumerate() {
        let object_id = i + 2; // start at 2 (color group is 1)
        let mut mesh = Mesh::new();

        for pos in &part.positions {
            mesh.vertices.push(Vertex::new(
                f64::from(pos[0]),
                f64::from(pos[1]),
                f64::from(pos[2]),
            ));
        }

        for tri_indices in part.indices.chunks(3) {
            let mut tri = Triangle::new(
                tri_indices[0] as usize,
                tri_indices[1] as usize,
                tri_indices[2] as usize,
            );
            // Assign color to triangle
            if let Some(Some(color_idx)) = part_color_indices.get(i) {
                tri.pid = Some(color_group_id);
                tri.pindex = Some(*color_idx);
            }
            mesh.triangles.push(tri);
        }

        let mut object = Object::new(object_id);
        object.name = Some(format!("Part {}", i + 1));
        object.mesh = Some(mesh);
        model.resources.objects.push(object);
        model.build.items.push(BuildItem::new(object_id));
    }

    model
        .write_to_file(path)
        .map_err(|e| format!("Failed to write 3MF: {e}"))?;

    Ok(())
}
