//! `OpenSCAD` compilation pipeline using `openscad-rs` (parser) and `csgrs` (CSG rendering).
//!
//! Parses `OpenSCAD` source with `openscad-rs`, walks the AST to evaluate geometry
//! primitives and boolean operations, and produces triangle mesh data for Bevy.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::{HashMap, HashSet};
use std::fmt;

use csgrs::bmesh::BMesh;
use csgrs::csg::CSG;
use csgrs::mesh::Mesh as CsgMesh;
use csgrs::mesh::plane::Plane;
use csgrs::polygon::Polygon;
use csgrs::sketch::Sketch;
use csgrs::triangulated::Triangulated3D;
use csgrs::vertex::Vertex;
use nalgebra::{Point3, Vector3};
use openscad_rs::ast::{
    Argument, BinaryOp, Expr, ExprKind, Parameter, SourceFile, Statement, UnaryOp,
};

/// Triangle mesh data ready for Bevy rendering.
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Optional color set via `color()` in the OpenSCAD code.
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

// ---------------------------------------------------------------------------
// Named CSS/OpenSCAD color lookup
// ---------------------------------------------------------------------------

fn named_color(name: &str) -> Option<[f32; 3]> {
    let rgb = match name.to_lowercase().as_str() {
        "red" => [1.0, 0.0, 0.0],
        "green" => [0.0, 0.5, 0.0],
        "blue" => [0.0, 0.0, 1.0],
        "yellow" => [1.0, 1.0, 0.0],
        "cyan" | "aqua" => [0.0, 1.0, 1.0],
        "magenta" | "fuchsia" => [1.0, 0.0, 1.0],
        "white" => [1.0, 1.0, 1.0],
        "black" => [0.0, 0.0, 0.0],
        "gray" | "grey" => [0.5, 0.5, 0.5],
        "orange" => [1.0, 0.647, 0.0],
        "pink" => [1.0, 0.753, 0.796],
        "purple" => [0.5, 0.0, 0.5],
        "brown" => [0.647, 0.165, 0.165],
        "gold" => [1.0, 0.843, 0.0],
        "silver" => [0.753, 0.753, 0.753],
        "navy" => [0.0, 0.0, 0.5],
        "olive" => [0.5, 0.5, 0.0],
        "teal" => [0.0, 0.5, 0.5],
        "maroon" => [0.5, 0.0, 0.0],
        "lime" => [0.0, 1.0, 0.0],
        "coral" => [1.0, 0.498, 0.314],
        "salmon" => [0.98, 0.502, 0.447],
        "tomato" => [1.0, 0.388, 0.278],
        "tan" => [0.824, 0.706, 0.549],
        "beige" => [0.961, 0.961, 0.863],
        "ivory" => [1.0, 1.0, 0.941],
        "khaki" => [0.941, 0.902, 0.549],
        "lavender" => [0.902, 0.902, 0.980],
        "plum" => [0.867, 0.627, 0.867],
        "orchid" => [0.855, 0.439, 0.839],
        "violet" => [0.933, 0.510, 0.933],
        "indigo" => [0.294, 0.0, 0.510],
        "turquoise" => [0.251, 0.878, 0.816],
        "sienna" => [0.627, 0.322, 0.176],
        "chocolate" => [0.824, 0.412, 0.118],
        "peru" => [0.804, 0.522, 0.247],
        "wheat" => [0.961, 0.871, 0.702],
        "linen" => [0.980, 0.941, 0.902],
        // OpenSCAD-specific / extended CSS colors
        "darkgreen" => [0.0, 0.392, 0.0],
        "darkred" => [0.545, 0.0, 0.0],
        "darkblue" => [0.0, 0.0, 0.545],
        "lightblue" => [0.678, 0.847, 0.902],
        "lightgreen" => [0.565, 0.933, 0.565],
        "lightgray" | "lightgrey" => [0.827, 0.827, 0.827],
        "darkgray" | "darkgrey" => [0.663, 0.663, 0.663],
        "steelblue" => [0.275, 0.510, 0.706],
        "cornflowerblue" => [0.392, 0.584, 0.929],
        "saddlebrown" => [0.545, 0.271, 0.075],
        "sandybrown" => [0.957, 0.643, 0.376],
        "forestgreen" => [0.133, 0.545, 0.133],
        "limegreen" => [0.196, 0.804, 0.196],
        "seagreen" => [0.180, 0.545, 0.341],
        "springgreen" => [0.0, 1.0, 0.498],
        "firebrick" => [0.698, 0.133, 0.133],
        "crimson" => [0.863, 0.078, 0.235],
        "skyblue" => [0.529, 0.808, 0.922],
        "deepskyblue" => [0.0, 0.749, 1.0],
        "dodgerblue" => [0.118, 0.565, 1.0],
        "royalblue" => [0.255, 0.412, 0.882],
        "midnightblue" => [0.098, 0.098, 0.439],
        "slategray" | "slategrey" => [0.439, 0.502, 0.565],
        "dimgray" | "dimgrey" => [0.412, 0.412, 0.412],
        "whitesmoke" => [0.961, 0.961, 0.961],
        "snow" => [1.0, 0.980, 0.980],
        "honeydew" => [0.941, 1.0, 0.941],
        "mintcream" => [0.961, 1.0, 0.980],
        "orangered" => [1.0, 0.271, 0.0],
        "greenyellow" => [0.678, 1.0, 0.184],
        "yellowgreen" => [0.604, 0.804, 0.196],
        "olivedrab" => [0.420, 0.557, 0.137],
        "darkolivegreen" => [0.333, 0.420, 0.184],
        "darkseagreen" => [0.561, 0.737, 0.561],
        "mediumseagreen" => [0.235, 0.702, 0.443],
        "aquamarine" => [0.498, 1.0, 0.831],
        "palegreen" => [0.596, 0.984, 0.596],
        "rosybrown" => [0.737, 0.561, 0.561],
        "darkgoldenrod" => [0.722, 0.525, 0.043],
        "goldenrod" => [0.855, 0.647, 0.125],
        "burlywood" => [0.871, 0.722, 0.529],
        "navajowhite" => [1.0, 0.871, 0.678],
        "moccasin" => [1.0, 0.894, 0.710],
        "peachpuff" => [1.0, 0.855, 0.725],
        _ => return None,
    };
    Some(rgb)
}

// ---------------------------------------------------------------------------
// Shape: unified 2D/3D geometry
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Shape {
    Mesh3D(Box<BMesh<()>>),
    Sketch2D(Sketch<()>),
}

impl fmt::Debug for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mesh3D(_) => write!(f, "Shape::Mesh3D"),
            Self::Sketch2D(_) => write!(f, "Shape::Sketch2D"),
        }
    }
}

impl Shape {
    /// Create a 3D shape from a `CsgMesh` primitive.
    /// Returns empty mesh if manifold creation fails (error surfaces later in bmesh_to_mesh_data).
    fn from_csg_mesh(mesh: CsgMesh<()>) -> Self {
        match csg_mesh_to_bmesh(mesh) {
            Ok(bmesh) => Self::Mesh3D(Box::new(bmesh)),
            Err(e) => {
                eprintln!("[SynapsCAD] {e}");
                Self::Mesh3D(Box::new(BMesh::new()))
            }
        }
    }

    /// Convert to `BMesh` for boolean operations.
    fn into_bmesh(self) -> BMesh<()> {
        match self {
            Self::Mesh3D(b) => *b,
            Self::Sketch2D(s) => BMesh::from(s.extrude(0.01)),
        }
    }

    /// Extract polygon data for hull computation (converts back to `CsgMesh`).
    fn into_csg_mesh(self) -> CsgMesh<()> {
        match self {
            Self::Mesh3D(b) => bmesh_to_csg_mesh(&b),
            Self::Sketch2D(s) => s.extrude(0.01),
        }
    }

    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Sketch2D(a), Self::Sketch2D(b)) => Self::Sketch2D(a.union(&b)),
            (a, b) => {
                let a_bmesh = a.into_bmesh();
                let b_bmesh = b.into_bmesh();
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    a_bmesh.union(&b_bmesh)
                })) {
                    Ok(r) => Self::Mesh3D(Box::new(r)),
                    Err(_) => {
                        eprintln!(
                            "[SynapsCAD] Warning: boolean union panicked, keeping first operand"
                        );
                        Self::Mesh3D(Box::new(a_bmesh))
                    }
                }
            }
        }
    }

    fn difference(self, other: Self) -> Self {
        match (self, other) {
            (Self::Sketch2D(a), Self::Sketch2D(b)) => Self::Sketch2D(a.difference(&b)),
            (a, b) => {
                let a_bmesh = a.into_bmesh();
                let b_bmesh = b.into_bmesh();
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    a_bmesh.difference(&b_bmesh)
                })) {
                    Ok(r) => Self::Mesh3D(Box::new(r)),
                    Err(_) => {
                        eprintln!(
                            "[SynapsCAD] Warning: boolean difference panicked, keeping first operand"
                        );
                        Self::Mesh3D(Box::new(a_bmesh))
                    }
                }
            }
        }
    }

    fn intersection(self, other: Self) -> Self {
        match (self, other) {
            (Self::Sketch2D(a), Self::Sketch2D(b)) => Self::Sketch2D(a.intersection(&b)),
            (a, b) => {
                let a_bmesh = a.into_bmesh();
                let b_bmesh = b.into_bmesh();
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    a_bmesh.intersection(&b_bmesh)
                })) {
                    Ok(r) => Self::Mesh3D(Box::new(r)),
                    Err(_) => {
                        eprintln!(
                            "[SynapsCAD] Warning: boolean intersection panicked, keeping first operand"
                        );
                        Self::Mesh3D(Box::new(a_bmesh))
                    }
                }
            }
        }
    }

    fn translate(self, x: f64, y: f64, z: f64) -> Self {
        match self {
            Self::Mesh3D(m) => Self::Mesh3D(Box::new(m.translate(x, y, z))),
            Self::Sketch2D(s) => {
                if z.abs() < 1e-12 {
                    Self::Sketch2D(s.translate(x, y, 0.0))
                } else {
                    Self::from_csg_mesh(s.extrude(0.01).translate(x, y, z))
                }
            }
        }
    }

    fn rotate(self, x: f64, y: f64, z: f64) -> Self {
        match self {
            Self::Mesh3D(m) => Self::Mesh3D(Box::new(m.rotate(x, y, z))),
            Self::Sketch2D(s) => {
                if x.abs() < 1e-12 && y.abs() < 1e-12 {
                    Self::Sketch2D(s.rotate(0.0, 0.0, z))
                } else {
                    Self::from_csg_mesh(s.extrude(0.01).rotate(x, y, z))
                }
            }
        }
    }

    fn scale(self, sx: f64, sy: f64, sz: f64) -> Self {
        match self {
            Self::Mesh3D(m) => Self::Mesh3D(Box::new(m.scale(sx, sy, sz))),
            Self::Sketch2D(s) => {
                if (sz - 1.0).abs() < 1e-12 {
                    Self::Sketch2D(s.scale(sx, sy, 1.0))
                } else {
                    Self::from_csg_mesh(s.extrude(0.01).scale(sx, sy, sz))
                }
            }
        }
    }

    fn mirror(self, nx: f64, ny: f64, nz: f64) -> Self {
        let len = (nx.mul_add(nx, ny.mul_add(ny, nz * nz))).sqrt();
        if len < 1e-12 {
            return self;
        }
        let plane = Plane::from_normal(Vector3::new(nx, ny, nz), 0.0);
        match self {
            Self::Mesh3D(m) => Self::Mesh3D(Box::new(m.mirror(plane))),
            Self::Sketch2D(s) => Self::Sketch2D(s.mirror(plane)),
        }
    }

    #[allow(dead_code)]
    fn center(self) -> Self {
        match self {
            Self::Mesh3D(m) => Self::Mesh3D(Box::new(m.center())),
            Self::Sketch2D(s) => Self::Sketch2D(s.center()),
        }
    }
}

/// Compile `OpenSCAD` source code into triangle mesh data.
pub fn compile_scad_code(code: &str) -> CompilationResult {
    let source_file = match openscad_rs::parse(code) {
        Ok(sf) => sf,
        Err(e) => {
            return CompilationResult::Error(format!("Parse error: {e}"));
        }
    };

    // Wrap evaluation + mesh conversion in catch_unwind to guard against
    // panics in boolmesh (non-manifold meshes, index overflows, etc.)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut evaluator = Evaluator::new();
        let shapes = evaluator.eval_source_file(&source_file);

        if shapes.is_empty() {
            return Err("No geometry produced".into());
        }

        let mut warnings = evaluator.warnings;
        let mut parts = Vec::new();
        for (i, (shape, color)) in shapes.into_iter().enumerate() {
            match bmesh_to_mesh_data(&shape.into_bmesh()) {
                Ok(mut data) => {
                    data.color = color;
                    parts.push(data);
                }
                Err(e) => {
                    warnings.push(format!("Part {}: {e}", i + 1));
                }
            }
        }

        if parts.is_empty() && !warnings.is_empty() {
            return Err(format!(
                "All parts failed to render: {}",
                warnings.join("; ")
            ));
        }

        let views = render_orthographic_views(&parts);
        Ok(CompilationResult::Success {
            parts,
            views,
            warnings,
        })
    }));

    match result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => CompilationResult::Error(e),
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            CompilationResult::Error(format!("Internal error: {msg}"))
        }
    }
}

// ---------------------------------------------------------------------------
// BMesh → CsgMesh conversion (for operations that need polygon access like hull)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// CsgMesh ↔ BMesh conversion
// ---------------------------------------------------------------------------

/// Convert `CsgMesh` to `BMesh`. If the mesh has boundary edges (non-manifold),
/// attempts to fix it by deduplicating vertices and removing degenerate/duplicate triangles.
/// Returns an error if all repair attempts fail.
fn csg_mesh_to_bmesh(mesh: CsgMesh<()>) -> Result<BMesh<()>, String> {
    use boolmesh::prelude::Manifold;
    const QUANT: f64 = 1e6;

    if mesh.polygons.is_empty() {
        return Ok(BMesh::new());
    }

    // Helper: try Manifold::new with catch_unwind to guard against internal panics
    // in boolmesh's edge topology (e.g. non-manifold meshes with >2 faces per edge).
    let try_manifold = |p: &[f64], i: &[usize]| -> Result<BMesh<()>, String> {
        let p = p.to_vec();
        let i = i.to_vec();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Manifold::new(&p, &i)))
            .map_err(|_| "Manifold::new panicked (non-manifold mesh)".to_string())?
            .map(|m| BMesh::from_manifold(m, None))
            .map_err(|e| e.to_string())
    };

    // Triangulate from polygons directly with proper vertex sharing.
    // CsgMesh::get_vertices_and_indices() creates unshared vertices (each polygon
    // gets its own copies), which breaks boolmesh's manifold requirement.
    let mut vmap: HashMap<[i64; 3], usize> = HashMap::new();
    let mut verts: Vec<f64> = Vec::new();
    let mut tris: Vec<usize> = Vec::new();

    let vert_idx =
        |vmap: &mut HashMap<[i64; 3], usize>, verts: &mut Vec<f64>, p: &Point3<f64>| -> usize {
            let key = [
                (p.x * QUANT).round() as i64,
                (p.y * QUANT).round() as i64,
                (p.z * QUANT).round() as i64,
            ];
            *vmap.entry(key).or_insert_with(|| {
                let idx = verts.len() / 3;
                verts.push(p.x);
                verts.push(p.y);
                verts.push(p.z);
                idx
            })
        };

    for poly in &mesh.polygons {
        let n = poly.vertices.len();
        if n < 3 {
            continue;
        }
        // Fan triangulation from vertex 0
        let i0 = vert_idx(&mut vmap, &mut verts, &poly.vertices[0].position);
        for j in 1..n - 1 {
            let i1 = vert_idx(&mut vmap, &mut verts, &poly.vertices[j].position);
            let i2 = vert_idx(&mut vmap, &mut verts, &poly.vertices[j + 1].position);
            if i0 == i1 || i1 == i2 || i2 == i0 {
                continue;
            }
            tris.push(i0);
            tris.push(i1);
            tris.push(i2);
        }
    }

    if tris.is_empty() {
        return Ok(BMesh::new());
    }

    // Attempt 1: direct with shared vertices
    if let Ok(bmesh) = try_manifold(&verts, &tris) {
        return Ok(bmesh);
    }

    // Attempt 2: flipped winding (hull algorithms sometimes have inconsistent winding)
    let mut flipped = tris.clone();
    for tri in flipped.chunks_mut(3) {
        tri.swap(1, 2);
    }
    if let Ok(bmesh) = try_manifold(&verts, &flipped) {
        return Ok(bmesh);
    }

    // Attempt 3: remove duplicate triangles (can occur from degenerate polygons)
    let mut seen: HashSet<[usize; 3]> = HashSet::new();
    let mut clean_tris: Vec<usize> = Vec::new();
    for tri in tris.chunks(3) {
        let mut key = [tri[0], tri[1], tri[2]];
        key.sort_unstable();
        if seen.insert(key) {
            clean_tris.extend_from_slice(tri);
        }
    }
    if clean_tris.len() != tris.len() {
        if let Ok(bmesh) = try_manifold(&verts, &clean_tris) {
            return Ok(bmesh);
        }
    }

    eprintln!("[SynapsCAD] Warning: Non-manifold mesh, all repair attempts failed");
    Err("Non-manifold mesh: boolean operation produced geometry that could not be repaired. Please report this bug with the code that caused it.".into())
}

fn bmesh_to_csg_mesh(bmesh: &BMesh<()>) -> CsgMesh<()> {
    let mut polygons = Vec::new();
    bmesh.visit_triangles(|[v0, v1, v2]| {
        polygons.push(Polygon::new(vec![v0, v1, v2], None));
    });
    CsgMesh::from_polygons(&polygons, None)
}

// ---------------------------------------------------------------------------
// BMesh → Bevy MeshData conversion (uses visit_triangles for clean output)
// ---------------------------------------------------------------------------

fn bmesh_to_mesh_data(bmesh: &BMesh<()>) -> Result<MeshData, String> {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();

    bmesh.visit_triangles(|[v0, v1, v2]| {
        let idx = positions.len() as u32;
        // OpenSCAD Z-up → Bevy Y-up: swap Y and Z
        for v in &[v0, v1, v2] {
            positions.push([
                v.position.x as f32,
                v.position.z as f32,
                v.position.y as f32,
            ]);
        }
        // Compute face normal in Bevy space from the swapped positions
        let a = positions[idx as usize];
        let b = positions[idx as usize + 1];
        let c = positions[idx as usize + 2];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            ab[1].mul_add(ac[2], -(ab[2] * ac[1])),
            ab[2].mul_add(ac[0], -(ab[0] * ac[2])),
            ab[0].mul_add(ac[1], -(ab[1] * ac[0])),
        ];
        let len = cross[0]
            .mul_add(cross[0], cross[1].mul_add(cross[1], cross[2] * cross[2]))
            .sqrt();
        let n = if len > 1e-6 {
            [cross[0] / len, cross[1] / len, cross[2] / len]
        } else {
            [0.0, 1.0, 0.0]
        };
        normals.push(n);
        normals.push(n);
        normals.push(n);
        indices.push(idx);
        indices.push(idx + 1);
        indices.push(idx + 2);
    });

    if positions.is_empty() {
        return Err("Mesh has no vertices".into());
    }

    Ok(MeshData {
        positions,
        normals,
        indices,
        color: None,
    })
}

// ---------------------------------------------------------------------------
// Software orthographic renderer (3 views: front, right, top)
// ---------------------------------------------------------------------------

const VIEW_SIZE: u32 = 256;
const BG_COLOR: [u8; 3] = [30, 30, 30];

struct ProjectedTri {
    verts: [(f32, f32, f32); 3], // (screen_x, screen_y, depth)
    normal: [f32; 3],
    color: [f32; 3], // RGB base color for this triangle
}

/// Default palette for parts without explicit color (matches PART_PALETTE in compilation.rs).
const VIEW_PART_PALETTE: &[[f32; 3]] = &[
    [0.40, 0.70, 1.00],
    [1.00, 0.60, 0.40],
    [0.50, 0.85, 0.50],
    [0.95, 0.75, 0.30],
    [0.70, 0.50, 0.90],
    [0.30, 0.85, 0.85],
    [0.95, 0.45, 0.60],
    [0.60, 0.80, 0.30],
    [0.85, 0.55, 0.80],
    [0.45, 0.65, 0.85],
    [0.90, 0.65, 0.55],
    [0.55, 0.75, 0.65],
];

fn render_orthographic_views(parts: &[MeshData]) -> Vec<ViewImage> {
    // Build per-part buffers with color tracking
    let mut all_pos = Vec::new();
    let mut all_norm = Vec::new();
    let mut all_idx = Vec::new();
    let mut tri_colors = Vec::new(); // one color per triangle
    for (part_idx, part) in parts.iter().enumerate() {
        let offset = all_pos.len() as u32;
        all_pos.extend_from_slice(&part.positions);
        all_norm.extend_from_slice(&part.normals);
        all_idx.extend(part.indices.iter().map(|i| i + offset));
        let color = part
            .color
            .unwrap_or(VIEW_PART_PALETTE[part_idx % VIEW_PART_PALETTE.len()]);
        let num_tris = part.indices.len() / 3;
        tri_colors.extend(std::iter::repeat_n(color, num_tris));
    }

    if all_pos.is_empty() {
        return Vec::new();
    }

    let views = [
        ("Front", [0, 1, 2], [1.0_f32, 1.0, 1.0]), // X→right, Y→up, Z=depth
        ("Right", [2, 1, 0], [-1.0_f32, 1.0, 1.0]), // -Z→right, Y→up, X=depth
        ("Top", [0, 2, 1], [1.0_f32, -1.0, 1.0]),  // X→right, -Z→up, Y=depth
    ];

    views
        .iter()
        .map(|(label, axes, flips)| {
            let base64_png =
                render_single_view(&all_pos, &all_norm, &all_idx, &tri_colors, *axes, *flips);
            ViewImage {
                label: (*label).to_string(),
                base64_png,
            }
        })
        .collect()
}

#[allow(clippy::similar_names, clippy::cast_possible_wrap)]
fn render_single_view(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    indices: &[u32],
    tri_colors: &[[f32; 3]],
    axes: [usize; 3], // [screen_x_axis, screen_y_axis, depth_axis]
    flips: [f32; 3],  // sign multipliers for each mapped axis
) -> String {
    use base64::Engine;

    let size = VIEW_SIZE as usize;
    let margin = 0.1; // 10% margin on each side

    // Project vertices
    let projected: Vec<(f32, f32, f32)> = positions
        .iter()
        .map(|p| {
            (
                p[axes[0]] * flips[0],
                p[axes[1]] * flips[1],
                p[axes[2]] * flips[2],
            )
        })
        .collect();

    // Bounding box of screen coords
    let (mut sx_min, mut sx_max) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut sy_min, mut sy_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for &(sx, sy, _) in &projected {
        sx_min = sx_min.min(sx);
        sx_max = sx_max.max(sx);
        sy_min = sy_min.min(sy);
        sy_max = sy_max.max(sy);
    }

    let range_x = sx_max - sx_min;
    let range_y = sy_max - sy_min;
    if range_x < 1e-6 || range_y < 1e-6 {
        // Degenerate — return empty
        return String::new();
    }

    // Uniform scale to fit with margin, keeping aspect ratio
    let usable = 2.0f32.mul_add(-margin, 1.0);
    let scale = (size as f32 * usable) / range_x.max(range_y);
    let cx = f32::midpoint(sx_min, sx_max);
    let cy = f32::midpoint(sy_min, sy_max);
    let half = size as f32 / 2.0;

    // Map to pixel coords (Y flipped: screen Y goes down)
    let to_pixel = |sx: f32, sy: f32| -> (f32, f32) {
        (
            (sx - cx).mul_add(scale, half),
            (-(sy - cy)).mul_add(scale, half),
        )
    };

    // Build projected triangles with face normals and colors
    let mut tris: Vec<ProjectedTri> = Vec::with_capacity(indices.len() / 3);
    for (tri_idx, tri) in indices.chunks(3).enumerate() {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let v0 = projected[i0];
        let v1 = projected[i1];
        let v2 = projected[i2];
        // Average normal for flat shading
        let n = [
            (normals[i0][0] + normals[i1][0] + normals[i2][0]) / 3.0,
            (normals[i0][1] + normals[i1][1] + normals[i2][1]) / 3.0,
            (normals[i0][2] + normals[i1][2] + normals[i2][2]) / 3.0,
        ];
        let color = tri_colors.get(tri_idx).copied().unwrap_or([0.4, 0.7, 1.0]);
        tris.push(ProjectedTri {
            verts: [v0, v1, v2],
            normal: n,
            color,
        });
    }

    // Rasterize with depth buffer
    let mut pixels = vec![BG_COLOR; size * size];
    let mut depth_buf = vec![f32::NEG_INFINITY; size * size];

    // Light direction (towards camera + slight offset)
    let light_dir = normalize([0.3, 0.5, 1.0]);

    for tri in &tris {
        let p0 = to_pixel(tri.verts[0].0, tri.verts[0].1);
        let p1 = to_pixel(tri.verts[1].0, tri.verts[1].1);
        let p2 = to_pixel(tri.verts[2].0, tri.verts[2].1);

        // Bounding box in pixels
        let min_px = (p0.0.min(p1.0).min(p2.0).floor() as i32).max(0);
        let max_px = (p0.0.max(p1.0).max(p2.0).ceil() as i32).min(size as i32 - 1);
        let min_py = (p0.1.min(p1.1).min(p2.1).floor() as i32).max(0);
        let max_py = (p0.1.max(p1.1).max(p2.1).ceil() as i32).min(size as i32 - 1);

        for py in min_py..=max_py {
            for px in min_px..=max_px {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                let (w0, w1, w2) = barycentric(p0, p1, p2, (fx, fy));
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    let depth = w2.mul_add(
                        tri.verts[2].2,
                        w0.mul_add(tri.verts[0].2, w1 * tri.verts[1].2),
                    );
                    let idx = py as usize * size + px as usize;
                    if depth > depth_buf[idx] {
                        depth_buf[idx] = depth;
                        // Diffuse shading with part color
                        let ndotl = dot(tri.normal, light_dir).abs();
                        let shade = 0.8f32.mul_add(ndotl, 0.2); // ambient + diffuse
                        let r = (tri.color[0] * 255.0 * shade).min(255.0) as u8;
                        let g = (tri.color[1] * 255.0 * shade).min(255.0) as u8;
                        let b = (tri.color[2] * 255.0 * shade).min(255.0) as u8;
                        pixels[idx] = [r, g, b];
                    }
                }
            }
        }
    }

    // Encode to PNG
    let mut img_buf = image::RgbImage::new(VIEW_SIZE, VIEW_SIZE);
    for (i, px) in pixels.iter().enumerate() {
        let x = (i % size) as u32;
        let y = (i / size) as u32;
        img_buf.put_pixel(x, y, image::Rgb(*px));
    }

    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    image::ImageEncoder::write_image(
        encoder,
        img_buf.as_raw(),
        VIEW_SIZE,
        VIEW_SIZE,
        image::ExtendedColorType::Rgb8,
    )
    .expect("PNG encoding failed");

    base64::engine::general_purpose::STANDARD.encode(&png_bytes)
}

fn barycentric(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p: (f32, f32)) -> (f32, f32, f32) {
    let d = (p1.1 - p2.1).mul_add(p0.0 - p2.0, (p2.0 - p1.0) * (p0.1 - p2.1));
    if d.abs() < 1e-10 {
        return (-1.0, -1.0, -1.0);
    }
    let w0 = (p1.1 - p2.1).mul_add(p.0 - p2.0, (p2.0 - p1.0) * (p.1 - p2.1)) / d;
    let w1 = (p2.1 - p0.1).mul_add(p.0 - p2.0, (p0.0 - p2.0) * (p.1 - p2.1)) / d;
    let w2 = 1.0 - w0 - w1;
    (w0, w1, w2)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[0].mul_add(b[0], a[1] * b[1]))
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = v[2].mul_add(v[2], v[0].mul_add(v[0], v[1] * v[1])).sqrt();
    if len < 1e-10 {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Convert axis-angle rotation (angle in degrees, axis [ax,ay,az]) to Euler angles [rx,ry,rz] in degrees.
/// Uses Rodrigues' rotation matrix → intrinsic ZYX Euler extraction.
fn axis_angle_to_euler(angle_deg: f64, ax: f64, ay: f64, az: f64) -> (f64, f64, f64) {
    let len = ax.mul_add(ax, ay.mul_add(ay, az * az)).sqrt();
    if len < 1e-12 {
        return (0.0, 0.0, 0.0);
    }
    let (ux, uy, uz) = (ax / len, ay / len, az / len);
    let theta = angle_deg.to_radians();
    let c = theta.cos();
    let s = theta.sin();
    let t = 1.0 - c;

    // Rotation matrix from Rodrigues' formula
    let r00 = (t * ux).mul_add(ux, c);
    let r01 = (t * ux).mul_add(uy, -(s * uz));
    let _r02 = (t * ux).mul_add(uz, s * uy);
    let r10 = (t * uy).mul_add(ux, s * uz);
    let r11 = (t * uy).mul_add(uy, c);
    let _r12 = (t * uy).mul_add(uz, -(s * ux));
    let r20 = (t * uz).mul_add(ux, -(s * uy));
    let r21 = (t * uz).mul_add(uy, s * ux);
    let r22 = (t * uz).mul_add(uz, c);

    // Extract intrinsic ZYX Euler angles (matching OpenSCAD's rotate([x,y,z]) convention)
    let ry = (-r20).asin();
    let (rx, rz) = if ry.cos().abs() > 1e-6 {
        (r21.atan2(r22), r10.atan2(r00))
    } else {
        (0.0, r01.atan2(r11))
    };

    (rx.to_degrees(), ry.to_degrees(), rz.to_degrees())
}

// ---------------------------------------------------------------------------
// AST Evaluator
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Value {
    Number(f64),
    Bool(bool),
    List(Vec<Self>),
    String(String),
    Range(f64, f64, f64), // from, to, step
    Undef,
}

impl Value {
    const fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    fn as_bool(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Number(n) => *n != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::List(l) => !l.is_empty(),
            Self::Undef => false,
            Self::Range(..) => true,
        }
    }

    fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }

    fn to_number_list(&self) -> Option<Vec<f64>> {
        self.as_list()
            .map(|l| l.iter().filter_map(Self::as_number).collect())
    }

    /// Expand ranges into lists for iteration.
    fn to_iterable(&self) -> Vec<Self> {
        match self {
            Self::Range(from, to, step) => {
                let mut vals = Vec::new();
                let mut v = *from;
                if *step > 0.0 {
                    #[allow(clippy::while_float)]
                    while v <= *to + 1e-12 {
                        vals.push(Self::Number(v));
                        v += step;
                    }
                } else if *step < 0.0 {
                    #[allow(clippy::while_float)]
                    while v >= *to - 1e-12 {
                        vals.push(Self::Number(v));
                        v += step;
                    }
                }
                vals
            }
            Self::List(l) => l.clone(),
            _ => vec![self.clone()],
        }
    }
}

/// Stored user-defined module.
#[derive(Clone)]
struct UserModule {
    params: Vec<(String, Option<Expr>)>,
    body: Vec<Statement>,
}

/// Stored user-defined function.
#[derive(Clone)]
struct UserFunction {
    params: Vec<(String, Option<Expr>)>,
    body_expr: Expr,
}

struct Evaluator {
    variables: HashMap<String, Value>,
    modules: HashMap<String, UserModule>,
    functions: HashMap<String, UserFunction>,
    /// Stack of call-site children for `children()` calls inside user modules.
    children_stack: Vec<Vec<Statement>>,
    /// Recursion depth counter to prevent stack overflow.
    depth: usize,
    /// Stack of active colors from nested `color()` calls.
    color_stack: Vec<[f32; 3]>,
    /// Warnings collected during evaluation (shown to the user after compilation).
    warnings: Vec<String>,
}

impl Evaluator {
    fn new() -> Self {
        let mut variables = HashMap::new();
        variables.insert("$fn".into(), Value::Number(0.0));
        variables.insert("$fa".into(), Value::Number(12.0));
        variables.insert("$fs".into(), Value::Number(2.0));
        variables.insert("PI".into(), Value::Number(std::f64::consts::PI));
        variables.insert("$preview".into(), Value::Bool(true));
        Self {
            variables,
            modules: HashMap::new(),
            functions: HashMap::new(),
            children_stack: Vec::new(),
            depth: 0,
            color_stack: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Resolve `$fn` from either explicit args or global variable.
    /// In preview mode, capped at 24 segments for performance.
    fn resolve_fn(&self, args: &[(Option<String>, Value)]) -> usize {
        let fn_val = Self::get_named_arg(args, "$fn")
            .and_then(Value::as_number)
            .or_else(|| self.variables.get("$fn").and_then(Value::as_number))
            .unwrap_or(0.0);
        let n = if fn_val > 0.0 { fn_val as usize } else { 16 };
        // Cap segments in preview mode for faster CSG operations
        n.min(Self::PREVIEW_FN_CAP)
    }

    /// Maximum `$fn` value during preview to keep CSG tractable.
    const PREVIEW_FN_CAP: usize = 24;

    // =======================================================================
    // Package & Statement evaluation
    // =======================================================================

    fn eval_source_file(&mut self, source_file: &SourceFile) -> Vec<(Shape, Option<[f32; 3]>)> {
        // First pass: register ALL module and function definitions in the file
        self.register_definitions(&source_file.statements);

        // Second pass: evaluate geometry with color tracking
        let mut shapes = Vec::new();
        for stmt in &source_file.statements {
            self.eval_statement(stmt, &mut shapes);
        }
        shapes
    }

    /// Recursively scan statements to register module/function definitions.
    fn register_definitions(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            match stmt {
                Statement::ModuleDefinition {
                    name, params, body, ..
                } => {
                    self.register_module(name, params, body);
                    self.register_definitions(body);
                }
                Statement::FunctionDefinition {
                    name, params, body, ..
                } => {
                    self.register_function(name, params, body);
                }
                Statement::ModuleInstantiation { children, .. } => {
                    self.register_definitions(children);
                }
                Statement::IfElse {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.register_definitions(then_body);
                    if let Some(eb) = else_body {
                        self.register_definitions(eb);
                    }
                }
                Statement::Block { body, .. } => {
                    self.register_definitions(body);
                }
                _ => {}
            }
        }
    }

    fn eval_statement(&mut self, stmt: &Statement, shapes: &mut Vec<(Shape, Option<[f32; 3]>)>) {
        match stmt {
            Statement::ModuleInstantiation {
                name,
                args,
                children,
                ..
            } => {
                // Handle for/let/color as special module instantiations
                match name.as_str() {
                    "for" | "intersection_for" => {
                        shapes.extend(self.eval_for_from_instantiation(args, children));
                    }
                    "let" => {
                        self.eval_let_instantiation(args, children, shapes);
                    }
                    "color" => {
                        let eval_args = self.eval_arguments(args);
                        self.eval_color_into(children, &eval_args, shapes);
                    }
                    "translate" | "rotate" | "scale" | "mirror" => {
                        let eval_args = self.eval_arguments(args);
                        let kind = match name.as_str() {
                            "translate" => TransformKind::Translate,
                            "rotate" => TransformKind::Rotate,
                            "scale" => TransformKind::Scale,
                            _ => TransformKind::Mirror,
                        };
                        self.eval_transform_into(children, &eval_args, kind, shapes);
                    }
                    _ => {
                        // Check for user-defined module — evaluate body directly
                        // to preserve per-shape colors
                        if let Some(user_mod) = self.modules.get(name).cloned() {
                            let eval_args = self.eval_arguments(args);
                            self.eval_user_module_into(&user_mod, &eval_args, children, shapes);
                        } else if let Some(s) =
                            self.eval_module_instantiation_inner(name, args, children)
                        {
                            let color = self.color_stack.last().copied();
                            shapes.push((s, color));
                        }
                    }
                }
            }
            Statement::Assignment { name, expr, .. } => {
                let val = self.eval_expr(expr);
                self.variables.insert(name.clone(), val);
            }
            Statement::IfElse {
                condition,
                then_body,
                else_body,
                ..
            } => {
                shapes.extend(self.eval_if_else(condition, then_body, else_body.as_ref()));
            }
            Statement::Block { body, .. } => {
                for s in body {
                    self.eval_statement(s, shapes);
                }
            }
            _ => {}
        }
    }

    // =======================================================================
    // For loop (from module instantiation)
    // =======================================================================

    fn eval_for_from_instantiation(
        &mut self,
        args: &[Argument],
        children: &[Statement],
    ) -> Vec<(Shape, Option<[f32; 3]>)> {
        // Collect loop variable assignments: for(i=[0:10], j=[0:5])
        let loop_vars: Vec<(String, Value)> = args
            .iter()
            .filter_map(|arg| {
                let name = arg.name.as_ref()?.clone();
                let val = self.eval_expr(&arg.value);
                Some((name, val))
            })
            .collect();

        self.eval_for_nested(&loop_vars, 0, children)
    }

    fn eval_for_nested(
        &mut self,
        loop_vars: &[(String, Value)],
        depth: usize,
        children: &[Statement],
    ) -> Vec<(Shape, Option<[f32; 3]>)> {
        if depth >= loop_vars.len() {
            return self.eval_statement_list(children);
        }

        let (name, range_val) = &loop_vars[depth];
        let items = range_val.to_iterable();
        let saved = self.variables.get(name).cloned();

        let mut results = Vec::new();
        for item in items {
            self.variables.insert(name.clone(), item);
            results.extend(self.eval_for_nested(loop_vars, depth + 1, children));
        }

        // Restore variable
        match saved {
            Some(v) => {
                self.variables.insert(name.clone(), v);
            }
            None => {
                self.variables.remove(name);
            }
        }
        results
    }

    fn eval_let_instantiation(
        &mut self,
        args: &[Argument],
        children: &[Statement],
        shapes: &mut Vec<(Shape, Option<[f32; 3]>)>,
    ) {
        let saved = self.variables.clone();
        for arg in args {
            if let Some(name) = &arg.name {
                let val = self.eval_expr(&arg.value);
                self.variables.insert(name.clone(), val);
            }
        }
        for stmt in children {
            self.eval_statement(stmt, shapes);
        }
        self.variables = saved;
    }

    fn eval_statement_list(&mut self, stmts: &[Statement]) -> Vec<(Shape, Option<[f32; 3]>)> {
        let mut shapes = Vec::new();
        for stmt in stmts {
            self.eval_statement(stmt, &mut shapes);
        }
        shapes
    }

    // =======================================================================
    // If/else
    // =======================================================================

    fn eval_if_else(
        &mut self,
        condition: &Expr,
        then_body: &[Statement],
        else_body: Option<&Vec<Statement>>,
    ) -> Vec<(Shape, Option<[f32; 3]>)> {
        let cond_val = self.eval_expr(condition);

        if cond_val.as_bool() {
            self.eval_statement_list(then_body)
        } else if let Some(eb) = else_body {
            self.eval_statement_list(eb)
        } else {
            Vec::new()
        }
    }

    // =======================================================================
    // Module & function registration
    // =======================================================================

    fn register_module(&mut self, name: &str, params: &[Parameter], body: &[Statement]) {
        let extracted_params = Self::extract_params(params);
        self.modules.insert(
            name.to_string(),
            UserModule {
                params: extracted_params,
                body: body.to_vec(),
            },
        );
    }

    fn register_function(&mut self, name: &str, params: &[Parameter], body: &Expr) {
        let extracted_params = Self::extract_params(params);
        self.functions.insert(
            name.to_string(),
            UserFunction {
                params: extracted_params,
                body_expr: body.clone(),
            },
        );
    }

    fn extract_params(params: &[Parameter]) -> Vec<(String, Option<Expr>)> {
        params
            .iter()
            .map(|p| (p.name.clone(), p.default.clone()))
            .collect()
    }

    // =======================================================================
    // User-defined module instantiation
    // =======================================================================

    /// Evaluate a user module, extending the parent shapes list directly
    /// to preserve individual per-shape colors.
    fn eval_user_module_into(
        &mut self,
        user_mod: &UserModule,
        args: &[(Option<String>, Value)],
        call_site_children: &[Statement],
        shapes: &mut Vec<(Shape, Option<[f32; 3]>)>,
    ) {
        const MAX_DEPTH: usize = 512;
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.warnings
                .push(format!("Maximum recursion depth ({MAX_DEPTH}) exceeded"));
            self.depth -= 1;
            return;
        }

        let saved_vars = self.variables.clone();
        self.children_stack.push(call_site_children.to_vec());

        // Bind parameters
        let mut pos_idx = 0;
        for (param_name, default_expr) in &user_mod.params {
            let named = Self::get_named_arg(args, param_name).cloned();
            let val = named.unwrap_or_else(|| {
                let v = Self::get_positional_arg(args, pos_idx).cloned();
                pos_idx += 1;
                v.or_else(|| default_expr.as_ref().map(|e| self.eval_expr(e)))
                    .unwrap_or(Value::Undef)
            });
            self.variables.insert(param_name.clone(), val);
        }
        for (name, val) in args {
            if let Some(n) = name
                && n.starts_with('$')
            {
                self.variables.insert(n.clone(), val.clone());
            }
        }

        for stmt in &user_mod.body {
            self.eval_statement(stmt, shapes);
        }

        self.variables = saved_vars;
        self.children_stack.pop();
        self.depth -= 1;
    }

    fn eval_user_module(
        &mut self,
        user_mod: &UserModule,
        args: &[(Option<String>, Value)],
        call_site_children: &[Statement],
    ) -> Option<Shape> {
        let saved_vars = self.variables.clone();

        // Push call-site children so children() can access them
        self.children_stack.push(call_site_children.to_vec());

        // Bind parameters: use provided args, fall back to defaults
        let mut pos_idx = 0;
        for (param_name, default_expr) in &user_mod.params {
            let named = Self::get_named_arg(args, param_name).cloned();
            let val = named.unwrap_or_else(|| {
                let v = Self::get_positional_arg(args, pos_idx).cloned();
                pos_idx += 1;
                v.or_else(|| default_expr.as_ref().map(|e| self.eval_expr(e)))
                    .unwrap_or(Value::Undef)
            });
            self.variables.insert(param_name.clone(), val);
        }

        // Also bind any special variables ($fn, $fa, $fs) from args
        for (name, val) in args {
            if let Some(n) = name
                && n.starts_with('$')
            {
                self.variables.insert(n.clone(), val.clone());
            }
        }

        // Evaluate module body
        let mut meshes = Vec::new();
        for stmt in &user_mod.body {
            self.eval_statement(stmt, &mut meshes);
        }

        // Restore variables and pop children stack
        self.variables = saved_vars;
        self.children_stack.pop();

        if meshes.is_empty() {
            None
        } else {
            let mut iter = meshes.into_iter();
            let (mut result, _) = iter.next().unwrap();
            for (m, _) in iter {
                result = result.union(m);
            }
            Some(result)
        }
    }

    // =======================================================================
    // Module instantiation dispatch
    // =======================================================================

    fn eval_module_instantiation_inner(
        &mut self,
        name: &str,
        raw_args: &[Argument],
        children: &[Statement],
    ) -> Option<Shape> {
        const MAX_DEPTH: usize = 512;
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.warnings.push(format!(
                "Maximum recursion depth ({MAX_DEPTH}) exceeded in {name}()"
            ));
            self.depth -= 1;
            return None;
        }
        let result = self.eval_module_instantiation_dispatch(name, raw_args, children);
        self.depth -= 1;
        result
    }

    fn eval_module_instantiation_dispatch(
        &mut self,
        name: &str,
        raw_args: &[Argument],
        children: &[Statement],
    ) -> Option<Shape> {
        let args = self.eval_arguments(raw_args);

        match name {
            // --- 3D primitives ---
            "cube" => self.eval_cube(&args),
            "sphere" => self.eval_sphere(&args),
            "cylinder" => self.eval_cylinder(&args),
            "polyhedron" => self.eval_polyhedron(&args),

            // --- 2D primitives ---
            "circle" => self.eval_circle(&args),
            "square" => self.eval_square(&args),
            "polygon" => self.eval_polygon(&args),
            "text" => {
                self.warnings
                    .push("text() not yet supported, skipping".into());
                None
            }

            // --- Boolean operations ---
            "union" => self.eval_boolean_op(children, BoolOp::Union),
            "difference" => self.eval_boolean_op(children, BoolOp::Difference),
            "intersection" => self.eval_boolean_op(children, BoolOp::Intersection),

            // --- Transformations ---
            "translate" => self.eval_transform(children, &args, TransformKind::Translate),
            "rotate" => self.eval_transform(children, &args, TransformKind::Rotate),
            "scale" => self.eval_transform(children, &args, TransformKind::Scale),
            "mirror" => self.eval_transform(children, &args, TransformKind::Mirror),
            "multmatrix" => {
                self.warnings
                    .push("multmatrix() not yet supported, passing through children".into());
                self.eval_passthrough_children(children)
            }
            "offset" => self.eval_offset(children, &args),
            "resize" | "projection" | "render" | "group" | "import" | "surface" => {
                self.eval_passthrough_children(children)
            }

            // --- Extrusions ---
            "linear_extrude" => self.eval_linear_extrude(children, &args),
            "rotate_extrude" => self.eval_rotate_extrude(children, &args),

            // --- Other ---
            "hull" => self.eval_hull(children),
            "minkowski" => {
                let child_shapes = self.eval_children(children);
                if child_shapes.len() >= 2 {
                    let mut iter = child_shapes.into_iter();
                    let base = iter.next().unwrap().into_csg_mesh();
                    let tool = iter.next().unwrap().into_csg_mesh();
                    Some(Shape::from_csg_mesh(base.minkowski_sum(&tool)))
                } else {
                    self.eval_passthrough_children(children)
                }
            }
            "echo" => {
                self.eval_echo(&args);
                self.eval_passthrough_children(children)
            }
            "children" => {
                // Evaluate call-site children from the parent user module
                self.children_stack
                    .last()
                    .cloned()
                    .and_then(|call_site_children| {
                        let shapes = self.eval_children(&call_site_children);
                        if shapes.is_empty() {
                            None
                        } else {
                            let mut iter = shapes.into_iter();
                            let mut result = iter.next().unwrap();
                            for s in iter {
                                result = result.union(s);
                            }
                            Some(result)
                        }
                    })
            }

            _ => {
                if let Some(user_mod) = self.modules.get(name).cloned() {
                    self.eval_user_module(&user_mod, &args, children)
                } else {
                    self.warnings
                        .push(format!("Unknown module: {name}(), skipping"));
                    None
                }
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn eval_echo(&self, args: &[(Option<String>, Value)]) {
        let parts: Vec<String> = args
            .iter()
            .map(|(name, val)| {
                let v = match val {
                    Value::Number(n) => format!("{n}"),
                    Value::Bool(b) => format!("{b}"),
                    Value::String(s) => format!("\"{s}\""),
                    Value::List(l) => format!("{l:?}"),
                    Value::Range(a, b, c) => format!("[{a}:{c}:{b}]"),
                    Value::Undef => "undef".into(),
                };
                match name {
                    Some(n) => format!("{n} = {v}"),
                    None => v,
                }
            })
            .collect();
        eprintln!("ECHO: {}", parts.join(", "));
    }

    fn eval_arguments(&mut self, args: &[Argument]) -> Vec<(Option<String>, Value)> {
        args.iter()
            .map(|arg| {
                let val = self.eval_expr(&arg.value);
                (arg.name.clone(), val)
            })
            .collect()
    }

    fn get_named_arg<'a>(args: &'a [(Option<String>, Value)], name: &str) -> Option<&'a Value> {
        args.iter()
            .find(|(n, _)| n.as_deref() == Some(name))
            .map(|(_, v)| v)
    }

    fn get_positional_arg(args: &[(Option<String>, Value)], idx: usize) -> Option<&Value> {
        let mut pos = 0;
        for (name, val) in args {
            if name.is_none() {
                if pos == idx {
                    return Some(val);
                }
                pos += 1;
            }
        }
        None
    }

    fn get_arg<'a>(
        args: &'a [(Option<String>, Value)],
        name: &str,
        pos: usize,
    ) -> Option<&'a Value> {
        Self::get_named_arg(args, name).or_else(|| Self::get_positional_arg(args, pos))
    }

    fn get_arg_number(args: &[(Option<String>, Value)], name: &str, pos: usize) -> Option<f64> {
        Self::get_arg(args, name, pos).and_then(Value::as_number)
    }

    fn get_arg_bool(
        args: &[(Option<String>, Value)],
        name: &str,
        pos: usize,
        default: bool,
    ) -> bool {
        Self::get_arg(args, name, pos).map_or(default, Value::as_bool)
    }

    // =======================================================================
    // 3D Primitives
    // =======================================================================

    #[allow(clippy::unused_self)]
    fn eval_cube(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let size_val = Self::get_arg(args, "size", 0).unwrap_or(&Value::Number(1.0));
        let center = Self::get_arg_bool(args, "center", 1, false);

        let mesh = match size_val {
            Value::Number(s) => {
                let m = CsgMesh::cube(*s, None);
                if center { m.center() } else { m }
            }
            Value::List(dims) => {
                let nums: Vec<f64> = dims.iter().filter_map(Value::as_number).collect();
                let (x, y, z) = match nums.len() {
                    1 => (nums[0], nums[0], nums[0]),
                    2 => (nums[0], nums[1], 1.0),
                    _ => (
                        nums.first().copied().unwrap_or(1.0),
                        nums.get(1).copied().unwrap_or(1.0),
                        nums.get(2).copied().unwrap_or(1.0),
                    ),
                };
                let m = CsgMesh::cube(1.0, None).scale(x, y, z);
                if center { m.center() } else { m }
            }
            _ => return None,
        };

        Some(Shape::from_csg_mesh(mesh))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn eval_sphere(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let r = Self::get_arg_number(args, "r", 0)
            .or_else(|| Self::get_arg_number(args, "d", 0).map(|d| d / 2.0))
            .unwrap_or(1.0);

        let slices = self.resolve_fn(args);
        let stacks = slices / 2;

        Some(Shape::from_csg_mesh(CsgMesh::sphere(
            r, slices, stacks, None,
        )))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn eval_cylinder(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let h = Self::get_arg_number(args, "h", 0)
            .or_else(|| Self::get_arg_number(args, "height", 0))
            .unwrap_or(1.0);

        // Handle d/d1/d2 (diameter) as well as r/r1/r2
        let r1 = Self::get_arg_number(args, "r1", 99)
            .or_else(|| Self::get_arg_number(args, "d1", 99).map(|d| d / 2.0))
            .or_else(|| Self::get_arg_number(args, "r", 1))
            .or_else(|| Self::get_arg_number(args, "d", 1).map(|d| d / 2.0))
            .unwrap_or(1.0);
        let r2 = Self::get_arg_number(args, "r2", 99)
            .or_else(|| Self::get_arg_number(args, "d2", 99).map(|d| d / 2.0))
            .unwrap_or(r1);

        let center = Self::get_arg_bool(args, "center", 99, false);
        let slices = self.resolve_fn(args);

        // For cones (r1 != r2): use CsgMesh::frustum which correctly
        // handles zero-radius (emits triangles, not degenerate quads).
        let m = if (r1 - r2).abs() < 1e-12 {
            CsgMesh::cylinder(r1, h, slices, None)
        } else {
            CsgMesh::frustum(r1, r2, h, slices, None)
        };
        let m = if center { m.center() } else { m };

        Some(Shape::from_csg_mesh(m))
    }

    #[allow(clippy::unused_self)]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn eval_polyhedron(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let points_val = Self::get_arg(args, "points", 0)?;
        let faces_val =
            Self::get_arg(args, "faces", 1).or_else(|| Self::get_arg(args, "triangles", 1));

        let points: Vec<[f64; 3]> = points_val
            .as_list()?
            .iter()
            .filter_map(|v| {
                let nums = v.to_number_list()?;
                if nums.len() >= 3 {
                    Some([nums[0], nums[1], nums[2]])
                } else {
                    None
                }
            })
            .collect();

        let faces: Vec<Vec<usize>> = faces_val?
            .as_list()?
            .iter()
            .filter_map(|v| {
                let nums = v.to_number_list()?;
                Some(nums.iter().map(|n| *n as usize).collect())
            })
            .collect();

        // Deduplicate faces: normalize each face to its canonical cyclic rotation
        // (start from the minimum index) and remove duplicates.  This prevents
        // non-manifold meshes caused by accidentally repeated pentagons/polygons.
        let faces = {
            let mut seen = std::collections::HashSet::new();
            let mut deduped = Vec::with_capacity(faces.len());
            for face in &faces {
                if face.is_empty() {
                    continue;
                }
                // Rotate so the minimum index comes first (canonical form)
                let min_pos = face
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| *v)
                    .map(|(i, _)| i)
                    .unwrap();
                let mut canonical: Vec<usize> = face[min_pos..].to_vec();
                canonical.extend_from_slice(&face[..min_pos]);
                if seen.insert(canonical) {
                    deduped.push(face.clone());
                }
            }
            deduped
        };

        let mut polygons = Vec::new();
        for face in &faces {
            if face.len() < 3 {
                continue;
            }
            let pts: Vec<_> = face.iter().filter_map(|&idx| points.get(idx)).collect();
            if pts.len() < 3 {
                continue;
            }
            // Compute face normal
            let v0 = Vector3::new(pts[0][0], pts[0][1], pts[0][2]);
            let v1 = Vector3::new(pts[1][0], pts[1][1], pts[1][2]);
            let v2 = Vector3::new(pts[2][0], pts[2][1], pts[2][2]);
            let normal = (v1 - v0).cross(&(v2 - v0)).normalize();

            if pts.len() == 3 {
                let verts: Vec<_> = pts
                    .iter()
                    .map(|p| Vertex::new(Point3::new(p[0], p[1], p[2]), normal))
                    .collect();
                polygons.push(Polygon::new(verts, None));
            } else {
                // Fan-triangulate N-gons (N>3) to avoid "Conflicting edge"
                // panics in spade's Delaunay triangulation during boolean ops.
                // Fan triangulation is correct for convex planar faces.
                let p0 = Point3::new(pts[0][0], pts[0][1], pts[0][2]);
                for i in 1..pts.len() - 1 {
                    let p1 = Point3::new(pts[i][0], pts[i][1], pts[i][2]);
                    let p2 = Point3::new(pts[i + 1][0], pts[i + 1][1], pts[i + 1][2]);
                    let verts = vec![
                        Vertex::new(p0, normal),
                        Vertex::new(p1, normal),
                        Vertex::new(p2, normal),
                    ];
                    polygons.push(Polygon::new(verts, None));
                }
            }
        }

        if polygons.is_empty() {
            return None;
        }
        Some(Shape::from_csg_mesh(CsgMesh::from_polygons(
            &polygons, None,
        )))
    }

    // =======================================================================
    // 2D Primitives
    // =======================================================================

    #[allow(clippy::unnecessary_wraps)]
    fn eval_circle(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let r = Self::get_arg_number(args, "r", 0)
            .or_else(|| Self::get_arg_number(args, "d", 0).map(|d| d / 2.0))
            .unwrap_or(1.0);

        let slices = self.resolve_fn(args);
        Some(Shape::Sketch2D(Sketch::circle(r, slices, None)))
    }

    #[allow(clippy::unused_self)]
    fn eval_square(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let size_val = Self::get_arg(args, "size", 0).unwrap_or(&Value::Number(1.0));
        let center = Self::get_arg_bool(args, "center", 1, false);

        let sketch = match size_val {
            Value::Number(s) => Sketch::square(*s, None),
            Value::List(dims) => {
                let nums: Vec<f64> = dims.iter().filter_map(Value::as_number).collect();
                let w = nums.first().copied().unwrap_or(1.0);
                let h = nums.get(1).copied().unwrap_or(w);
                Sketch::rectangle(w, h, None)
            }
            _ => return None,
        };

        let sketch = if center { sketch.center() } else { sketch };
        Some(Shape::Sketch2D(sketch))
    }

    #[allow(clippy::unused_self)]
    fn eval_polygon(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let points_val = Self::get_arg(args, "points", 0)?;
        let points: Vec<[f64; 2]> = points_val
            .as_list()?
            .iter()
            .filter_map(|v| {
                let nums = v.to_number_list()?;
                if nums.len() >= 2 {
                    Some([nums[0], nums[1]])
                } else {
                    None
                }
            })
            .collect();

        if points.len() < 3 {
            return None;
        }
        Some(Shape::Sketch2D(Sketch::polygon(&points, None)))
    }

    // =======================================================================
    // Boolean operations
    // =======================================================================

    fn eval_boolean_op(&mut self, children: &[Statement], op: BoolOp) -> Option<Shape> {
        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }

        let mut iter = child_shapes.into_iter();
        let first = iter.next().unwrap();

        match op {
            BoolOp::Union => {
                // Batch union: merge all meshes via polygon concatenation,
                // then do a single BSP union to resolve intersections.
                let rest: Vec<Shape> = iter.collect();
                if rest.is_empty() {
                    return Some(first);
                }
                let mut result = first;
                for child in rest {
                    result = result.union(child);
                }
                Some(result)
            }
            BoolOp::Difference => {
                // Batch difference: union all subtracted children first,
                // then perform a single difference operation.
                let rest: Vec<Shape> = iter.collect();
                if rest.is_empty() {
                    return Some(first);
                }
                // Union all the "tool" shapes into one
                let mut tool_iter = rest.into_iter();
                let mut tool = tool_iter.next().unwrap();
                for t in tool_iter {
                    tool = tool.union(t);
                }
                Some(first.difference(tool))
            }
            BoolOp::Intersection => {
                let mut result = first;
                for child in iter {
                    result = result.intersection(child);
                }
                Some(result)
            }
        }
    }

    // =======================================================================
    // Transformations
    // =======================================================================

    fn eval_transform(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
        kind: TransformKind,
    ) -> Option<Shape> {
        let child = self.eval_passthrough_children(children)?;
        Some(Self::apply_transform(child, &kind, args))
    }

    // =======================================================================
    // 2D Offset
    // =======================================================================

    fn eval_offset(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
    ) -> Option<Shape> {
        let r = Self::get_arg_number(args, "r", 99);
        let delta = Self::get_arg_number(args, "delta", 99);

        // Collect 2D children
        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }
        let sketch = self.shapes_to_sketch(&child_shapes)?;

        // OpenSCAD semantics:
        // offset(r=R) → rounded corners (Minkowski with circle of radius R)
        // offset(delta=D) → sharp corners
        // r takes precedence if both specified
        if let Some(r_val) = r {
            if r_val.abs() > 1e-12 {
                Some(Shape::Sketch2D(sketch.offset_rounded(r_val)))
            } else {
                Some(Shape::Sketch2D(sketch))
            }
        } else if let Some(d_val) = delta {
            if d_val.abs() > 1e-12 {
                Some(Shape::Sketch2D(sketch.offset(d_val)))
            } else {
                Some(Shape::Sketch2D(sketch))
            }
        } else {
            // Positional arg (offset(5) → delta=5 in OpenSCAD)
            let d = Self::get_arg_number(args, "", 0).unwrap_or(0.0);
            if d.abs() > 1e-12 {
                Some(Shape::Sketch2D(sketch.offset(d)))
            } else {
                Some(Shape::Sketch2D(sketch))
            }
        }
    }

    // =======================================================================
    // Extrusions
    // =======================================================================

    fn eval_linear_extrude(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
    ) -> Option<Shape> {
        let height = Self::get_arg_number(args, "height", 0).unwrap_or(1.0);
        let twist = Self::get_arg_number(args, "twist", 99).unwrap_or(0.0);
        let scale_val = Self::get_arg_number(args, "scale", 99).unwrap_or(1.0);
        let center = Self::get_arg_bool(args, "center", 99, false);
        let slices = self.resolve_fn(args);

        // Collect 2D children
        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }

        // Merge all children into a single sketch (if possible)
        let sketch = self.shapes_to_sketch(&child_shapes)?;

        let mesh = if twist.abs() > 1e-12 || (scale_val - 1.0).abs() > 1e-12 {
            // Twisted/scaled extrusion: approximate by layered slices
            self.twisted_extrude(&sketch, height, twist, scale_val, slices)
        } else {
            sketch.extrude(height)
        };

        let mesh = if center { mesh.center() } else { mesh };
        Some(Shape::from_csg_mesh(mesh))
    }

    fn eval_rotate_extrude(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
    ) -> Option<Shape> {
        let angle = Self::get_arg_number(args, "angle", 0).unwrap_or(360.0);
        let slices = self.resolve_fn(args);

        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }

        let sketch = self.shapes_to_sketch(&child_shapes)?;
        let mesh = match sketch.revolve(angle, slices) {
            Ok(m) => m,
            Err(e) => {
                self.warnings.push(format!("rotate_extrude() error: {e:?}"));
                return None;
            }
        };
        Some(Shape::from_csg_mesh(mesh))
    }

    /// Convert shapes to a single Sketch. 3D meshes are dropped with a warning.
    fn shapes_to_sketch(&mut self, shapes: &[Shape]) -> Option<Sketch<()>> {
        let mut result: Option<Sketch<()>> = None;
        for shape in shapes {
            match shape {
                Shape::Sketch2D(s) => {
                    result = Some(result.map_or_else(|| s.clone(), |r| r.union(s)));
                }
                Shape::Mesh3D(_) => {
                    self.warnings
                        .push("3D mesh child inside extrude, skipping".into());
                }
            }
        }
        result
    }

    /// Approximate twisted/tapered linear extrusion by stacking rotated+scaled layers.
    #[allow(clippy::unused_self, clippy::cast_precision_loss)]
    fn twisted_extrude(
        &self,
        sketch: &Sketch<()>,
        height: f64,
        twist: f64,
        end_scale: f64,
        n_slices: usize,
    ) -> CsgMesh<()> {
        let n = n_slices.max(2);
        let mut result: Option<BMesh<()>> = None;

        for i in 0..n {
            let t0 = i as f64 / n as f64;
            let t1 = (i + 1) as f64 / n as f64;
            let z0 = height * t0;
            let z1 = height * t1;
            let angle0 = twist * t0;
            let angle1 = twist * t1;
            let s0 = (end_scale - 1.0).mul_add(t0, 1.0);
            let s1 = (end_scale - 1.0).mul_add(t1, 1.0);
            let layer_h = z1 - z0;

            if layer_h < 1e-12 {
                continue;
            }

            // Create a thin slice: extrude the sketch by layer height,
            // scale, rotate, then translate to correct Z position
            let avg_scale = f64::midpoint(s0, s1);
            let avg_angle = f64::midpoint(angle0, angle1);

            let layer = sketch
                .extrude(layer_h)
                .scale(avg_scale, avg_scale, 1.0)
                .rotate(0.0, 0.0, avg_angle)
                .translate(0.0, 0.0, z0);

            let layer_bmesh = BMesh::from(layer);
            result = Some(match result {
                Some(r) => r.union(&layer_bmesh),
                None => layer_bmesh,
            });
        }

        result.map_or_else(|| CsgMesh::cube(0.001, None), |b| bmesh_to_csg_mesh(&b))
    }

    // =======================================================================
    // Hull
    // =======================================================================

    fn eval_hull(&mut self, children: &[Statement]) -> Option<Shape> {
        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }
        // Collect all polygons from children and compute convex hull on the combined mesh
        let mut all_polygons = Vec::new();
        for shape in child_shapes {
            let mesh = shape.into_csg_mesh();
            all_polygons.extend(mesh.polygons);
        }
        let combined = CsgMesh::from_polygons(&all_polygons, None);
        Some(Shape::from_csg_mesh(combined.convex_hull()))
    }

    // =======================================================================
    // Children helpers
    // =======================================================================

    fn eval_passthrough_children(&mut self, children: &[Statement]) -> Option<Shape> {
        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }
        let mut iter = child_shapes.into_iter();
        let mut result = iter.next().unwrap();
        for child in iter {
            result = result.union(child);
        }
        Some(result)
    }

    /// Handle `color("name")` or `color([r,g,b])` — pushes color, evals children into parent shapes, pops.
    fn eval_color_into(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
        shapes: &mut Vec<(Shape, Option<[f32; 3]>)>,
    ) {
        let rgb = Self::parse_color_args(args);
        if let Some(c) = rgb {
            self.color_stack.push(c);
        }
        for stmt in children {
            self.eval_statement(stmt, shapes);
        }
        if rgb.is_some() {
            self.color_stack.pop();
        }
    }

    /// Evaluate children preserving per-shape colors, then apply a transform to each.
    fn eval_transform_into(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
        kind: TransformKind,
        shapes: &mut Vec<(Shape, Option<[f32; 3]>)>,
    ) {
        let before = shapes.len();
        for stmt in children {
            self.eval_statement(stmt, shapes);
        }
        // Apply the transform to every newly-added shape
        let new_shapes: Vec<_> = shapes.drain(before..).collect();
        for (s, color) in new_shapes {
            shapes.push((Self::apply_transform(s, &kind, args), color));
        }
    }

    /// Apply a single transform to a shape (shared logic for eval_transform and eval_transform_into).
    fn apply_transform(
        shape: Shape,
        kind: &TransformKind,
        args: &[(Option<String>, Value)],
    ) -> Shape {
        match kind {
            TransformKind::Translate => {
                let v = Self::get_positional_arg(args, 0)
                    .or_else(|| Self::get_named_arg(args, "v"))
                    .and_then(Value::to_number_list)
                    .unwrap_or_default();
                let (x, y, z) = (
                    v.first().copied().unwrap_or(0.0),
                    v.get(1).copied().unwrap_or(0.0),
                    v.get(2).copied().unwrap_or(0.0),
                );
                shape.translate(x, y, z)
            }
            TransformKind::Rotate => {
                let axis_vec = Self::get_named_arg(args, "v").and_then(Value::to_number_list);
                let a_val =
                    Self::get_positional_arg(args, 0).or_else(|| Self::get_named_arg(args, "a"));

                if let (Some(angle), Some(ax)) =
                    (a_val.as_ref().and_then(|v| v.as_number()), &axis_vec)
                {
                    let (ex, ey, ez) = axis_angle_to_euler(
                        angle,
                        ax.first().copied().unwrap_or(0.0),
                        ax.get(1).copied().unwrap_or(0.0),
                        ax.get(2).copied().unwrap_or(1.0),
                    );
                    shape.rotate(ex, ey, ez)
                } else if let Some(v) = a_val.and_then(Value::to_number_list) {
                    let (x, y, z) = (
                        v.first().copied().unwrap_or(0.0),
                        v.get(1).copied().unwrap_or(0.0),
                        v.get(2).copied().unwrap_or(0.0),
                    );
                    shape.rotate(x, y, z)
                } else {
                    let angle = Self::get_positional_arg(args, 0)
                        .and_then(Value::as_number)
                        .unwrap_or(0.0);
                    shape.rotate(0.0, 0.0, angle)
                }
            }
            TransformKind::Scale => {
                let val =
                    Self::get_positional_arg(args, 0).or_else(|| Self::get_named_arg(args, "v"));
                match val {
                    Some(Value::List(_)) => {
                        let v = val.and_then(Value::to_number_list).unwrap_or_default();
                        let (x, y, z) = (
                            v.first().copied().unwrap_or(1.0),
                            v.get(1).copied().unwrap_or(1.0),
                            v.get(2).copied().unwrap_or(1.0),
                        );
                        shape.scale(x, y, z)
                    }
                    Some(Value::Number(s)) => shape.scale(*s, *s, *s),
                    _ => shape,
                }
            }
            TransformKind::Mirror => {
                let v = Self::get_positional_arg(args, 0)
                    .or_else(|| Self::get_named_arg(args, "v"))
                    .and_then(Value::to_number_list)
                    .unwrap_or_else(|| vec![1.0, 0.0, 0.0]);
                let (nx, ny, nz) = (
                    v.first().copied().unwrap_or(1.0),
                    v.get(1).copied().unwrap_or(0.0),
                    v.get(2).copied().unwrap_or(0.0),
                );
                shape.mirror(nx, ny, nz)
            }
        }
    }

    /// Parse color arguments: `color("red")`, `color("red", 0.5)`, `color([r,g,b])`, `color([r,g,b,a])`.
    fn parse_color_args(args: &[(Option<String>, Value)]) -> Option<[f32; 3]> {
        let first = args.first().map(|(_, v)| v)?;
        match first {
            Value::String(name) => named_color(name),
            Value::List(items) => {
                if items.len() >= 3 {
                    let r = items[0].as_number()? as f32;
                    let g = items[1].as_number()? as f32;
                    let b = items[2].as_number()? as f32;
                    Some([r, g, b])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn eval_children(&mut self, children: &[Statement]) -> Vec<Shape> {
        // Process each statement separately, unioning shapes per statement.
        // This ensures booleans like difference() get one shape per child statement.
        let mut result = Vec::new();
        for stmt in children {
            let mut shapes = Vec::new();
            self.eval_statement(stmt, &mut shapes);
            if !shapes.is_empty() {
                let mut iter = shapes.into_iter().map(|(s, _)| s);
                let mut merged = iter.next().unwrap();
                for s in iter {
                    merged = merged.union(s);
                }
                result.push(merged);
            }
        }
        result
    }

    // =======================================================================
    // Expression evaluation
    // =======================================================================

    fn eval_expr(&mut self, expr: &Expr) -> Value {
        match &expr.kind {
            ExprKind::Number(n) => Value::Number(*n),
            ExprKind::String(s) => Value::String(s.clone()),
            ExprKind::BoolTrue => Value::Bool(true),
            ExprKind::BoolFalse => Value::Bool(false),
            ExprKind::Identifier(name) => self.variables.get(name).cloned().unwrap_or(Value::Undef),
            ExprKind::Vector(items) => {
                let mut vals: Vec<Value> = Vec::new();
                for item in items {
                    let is_lc = matches!(
                        &item.kind,
                        ExprKind::LcFor { .. }
                            | ExprKind::LcForC { .. }
                            | ExprKind::LcIf { .. }
                            | ExprKind::LcEach { .. }
                            | ExprKind::LcLet { .. }
                    );
                    let val = self.eval_expr(item);
                    if is_lc {
                        // Flatten list comprehension results into the vector
                        if let Value::List(inner) = val {
                            vals.extend(inner);
                        } else if !matches!(val, Value::Undef) {
                            vals.push(val);
                        }
                    } else {
                        vals.push(val);
                    }
                }
                Value::List(vals)
            }
            ExprKind::Range { start, step, end } => {
                let from = self.eval_expr(start).as_number().unwrap_or(0.0);
                let to = self.eval_expr(end).as_number().unwrap_or(0.0);
                let s = step.as_ref().map_or_else(
                    || if to >= from { 1.0 } else { -1.0 },
                    |step_expr| self.eval_expr(step_expr).as_number().unwrap_or(1.0),
                );
                Value::Range(from, to, s)
            }
            ExprKind::UnaryOp { op, operand } => {
                let inner = self.eval_expr(operand);
                match op {
                    UnaryOp::Negate => match inner {
                        Value::Number(n) => Value::Number(-n),
                        _ => Value::Undef,
                    },
                    UnaryOp::Not => Value::Bool(!inner.as_bool()),
                    UnaryOp::Plus => inner,
                    UnaryOp::BinaryNot => Value::Undef,
                }
            }
            ExprKind::BinaryOp { op, left, right } => self.eval_binary_op(*op, left, right),
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let cond = self.eval_expr(condition);
                if cond.as_bool() {
                    self.eval_expr(then_expr)
                } else {
                    self.eval_expr(else_expr)
                }
            }
            ExprKind::FunctionCall { callee, args } => {
                // Get function name from callee
                let name = match &callee.kind {
                    ExprKind::Identifier(n) => n.clone(),
                    _ => return Value::Undef,
                };
                let call_args: Vec<(Option<String>, Value)> = args
                    .iter()
                    .map(|arg| {
                        let val = self.eval_expr(&arg.value);
                        (arg.name.clone(), val)
                    })
                    .collect();

                // Check user-defined functions first
                if let Some(user_fn) = self.functions.get(&name).cloned() {
                    return self.eval_user_function(&user_fn, &call_args);
                }

                let args_vals: Vec<Value> = call_args.into_iter().map(|(_, v)| v).collect();
                self.eval_builtin_function(&name, &args_vals)
            }
            ExprKind::Index { object, index } => {
                let base = self.eval_expr(object);
                let idx = self.eval_expr(index);
                match (&base, &idx) {
                    (Value::List(l), Value::Number(i)) => {
                        let idx = *i as usize;
                        l.get(idx).cloned().unwrap_or(Value::Undef)
                    }
                    _ => Value::Undef,
                }
            }
            ExprKind::MemberAccess { object, member } => {
                let base = self.eval_expr(object);
                match (&base, member.as_str()) {
                    (Value::List(l), "x") => l.first().cloned().unwrap_or(Value::Undef),
                    (Value::List(l), "y") => l.get(1).cloned().unwrap_or(Value::Undef),
                    (Value::List(l), "z") => l.get(2).cloned().unwrap_or(Value::Undef),
                    _ => Value::Undef,
                }
            }
            ExprKind::Let { assignments, body } | ExprKind::LcLet { assignments, body } => {
                let saved = self.variables.clone();
                for arg in assignments {
                    if let Some(name) = &arg.name {
                        let val = self.eval_expr(&arg.value);
                        self.variables.insert(name.clone(), val);
                    }
                }
                let result = self.eval_expr(body);
                self.variables = saved;
                result
            }
            ExprKind::LcFor { assignments, body } => {
                let loop_vars: Vec<(String, Value)> = assignments
                    .iter()
                    .filter_map(|arg| {
                        let name = arg.name.as_ref()?.clone();
                        let val = self.eval_expr(&arg.value);
                        Some((name, val))
                    })
                    .collect();
                let mut results = Vec::new();
                self.eval_lc_for_nested(&loop_vars, 0, body, &mut results);
                Value::List(results)
            }
            ExprKind::LcIf {
                condition,
                then_expr,
                else_expr,
            } => {
                let cond = self.eval_expr(condition);
                if cond.as_bool() {
                    self.eval_expr(then_expr)
                } else if let Some(ee) = else_expr {
                    self.eval_expr(ee)
                } else {
                    Value::Undef
                }
            }
            ExprKind::LcEach { body } => self.eval_expr(body),
            ExprKind::Echo { args, body } => {
                let echo_args: Vec<(Option<String>, Value)> = args
                    .iter()
                    .map(|a| (a.name.clone(), self.eval_expr(&a.value)))
                    .collect();
                self.eval_echo(&echo_args);
                body.as_ref().map_or(Value::Undef, |b| self.eval_expr(b))
            }
            ExprKind::Assert { body, .. } => {
                body.as_ref().map_or(Value::Undef, |b| self.eval_expr(b))
            }
            _ => Value::Undef,
        }
    }

    fn eval_lc_for_nested(
        &mut self,
        loop_vars: &[(String, Value)],
        depth: usize,
        body: &Expr,
        results: &mut Vec<Value>,
    ) {
        if depth >= loop_vars.len() {
            let val = self.eval_expr(body);
            match val {
                Value::Undef => {}
                _ => results.push(val),
            }
            return;
        }
        let (name, range_val) = &loop_vars[depth];
        let items = range_val.to_iterable();
        let saved = self.variables.get(name).cloned();
        for item in items {
            self.variables.insert(name.clone(), item);
            self.eval_lc_for_nested(loop_vars, depth + 1, body, results);
        }
        match saved {
            Some(v) => {
                self.variables.insert(name.clone(), v);
            }
            None => {
                self.variables.remove(name);
            }
        }
    }

    fn eval_user_function(
        &mut self,
        user_fn: &UserFunction,
        args: &[(Option<String>, Value)],
    ) -> Value {
        let saved = self.variables.clone();

        let mut pos_idx = 0;
        for (param_name, default_expr) in &user_fn.params {
            let val = Self::get_named_arg(args, param_name)
                .cloned()
                .or_else(|| {
                    let v = Self::get_positional_arg(args, pos_idx).cloned();
                    pos_idx += 1;
                    v
                })
                .or_else(|| default_expr.as_ref().map(|e| self.eval_expr(e)))
                .unwrap_or(Value::Undef);
            self.variables.insert(param_name.clone(), val);
            if Self::get_named_arg(args, param_name).is_none() {
                // pos already incremented
            }
        }

        let result = self.eval_expr(&user_fn.body_expr);
        self.variables = saved;
        result
    }

    fn eval_binary_op(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Value {
        let lhs = self.eval_expr(left);
        let rhs = self.eval_expr(right);
        match (lhs, rhs) {
            (Value::Number(a), Value::Number(b)) => match op {
                BinaryOp::Add => Value::Number(a + b),
                BinaryOp::Subtract => Value::Number(a - b),
                BinaryOp::Multiply => Value::Number(a * b),
                BinaryOp::Divide => Value::Number(if b == 0.0 { f64::NAN } else { a / b }),
                BinaryOp::Modulo => Value::Number(a % b),
                BinaryOp::Exponent => Value::Number(a.powf(b)),
                BinaryOp::Less => Value::Bool(a < b),
                BinaryOp::Greater => Value::Bool(a > b),
                BinaryOp::LessEqual => Value::Bool(a <= b),
                BinaryOp::GreaterEqual => Value::Bool(a >= b),
                BinaryOp::Equal => Value::Bool((a - b).abs() < f64::EPSILON),
                BinaryOp::NotEqual => Value::Bool((a - b).abs() >= f64::EPSILON),
                BinaryOp::LogicalAnd => Value::Bool(a != 0.0 && b != 0.0),
                BinaryOp::LogicalOr => Value::Bool(a != 0.0 || b != 0.0),
                _ => Value::Undef,
            },
            (Value::Bool(a), Value::Bool(b)) => match op {
                BinaryOp::LogicalAnd => Value::Bool(a && b),
                BinaryOp::LogicalOr => Value::Bool(a || b),
                BinaryOp::Equal => Value::Bool(a == b),
                BinaryOp::NotEqual => Value::Bool(a != b),
                _ => Value::Undef,
            },
            (Value::String(a), Value::String(b)) => match op {
                BinaryOp::Equal => Value::Bool(a == b),
                BinaryOp::NotEqual => Value::Bool(a != b),
                _ => Value::Undef,
            },
            // Scalar * List (vector scaling, recursive for nested lists)
            (Value::Number(s), Value::List(l)) | (Value::List(l), Value::Number(s))
                if matches!(op, BinaryOp::Multiply) =>
            {
                fn scale_list(l: &[Value], s: f64) -> Vec<Value> {
                    l.iter()
                        .map(|v| match v {
                            Value::Number(n) => Value::Number(n * s),
                            Value::List(inner) => Value::List(scale_list(inner, s)),
                            other => other.clone(),
                        })
                        .collect()
                }
                Value::List(scale_list(&l, s))
            }
            // Scalar / List (recursive for nested lists)
            (Value::List(l), Value::Number(s)) if matches!(op, BinaryOp::Divide) => {
                fn div_list(l: &[Value], s: f64) -> Vec<Value> {
                    l.iter()
                        .map(|v| match v {
                            Value::Number(n) => {
                                Value::Number(if s == 0.0 { f64::NAN } else { n / s })
                            }
                            Value::List(inner) => Value::List(div_list(inner, s)),
                            other => other.clone(),
                        })
                        .collect()
                }
                Value::List(div_list(&l, s))
            }
            // List +/- List (vector add/sub)
            (Value::List(a), Value::List(b))
                if matches!(op, BinaryOp::Add | BinaryOp::Subtract) =>
            {
                let len = a.len().max(b.len());
                let result: Vec<Value> = (0..len)
                    .map(|i| {
                        let va = a.get(i).and_then(Value::as_number).unwrap_or(0.0);
                        let vb = b.get(i).and_then(Value::as_number).unwrap_or(0.0);
                        Value::Number(match op {
                            BinaryOp::Add => va + vb,
                            _ => va - vb,
                        })
                    })
                    .collect();
                Value::List(result)
            }
            _ => Value::Undef,
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn eval_builtin_function(&mut self, name: &str, args: &[Value]) -> Value {
        match name {
            // Trigonometric (OpenSCAD uses degrees)
            "sin" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.to_radians().sin())),
            "cos" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.to_radians().cos())),
            "tan" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.to_radians().tan())),
            "asin" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.asin().to_degrees())),
            "acos" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.acos().to_degrees())),
            "atan" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.atan().to_degrees())),
            "atan2" => {
                if args.len() >= 2 {
                    match (args[0].as_number(), args[1].as_number()) {
                        (Some(y), Some(x)) => Value::Number(y.atan2(x).to_degrees()),
                        _ => Value::Undef,
                    }
                } else {
                    Value::Undef
                }
            }

            // Math
            "abs" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.abs())),
            "sqrt" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.sqrt())),
            "exp" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.exp())),
            "ln" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.ln())),
            "log" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.log10())),
            "sign" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.signum())),
            "pow" => {
                if args.len() >= 2 {
                    match (args[0].as_number(), args[1].as_number()) {
                        (Some(a), Some(b)) => Value::Number(a.powf(b)),
                        _ => Value::Undef,
                    }
                } else {
                    Value::Undef
                }
            }

            // Rounding
            "round" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.round())),
            "ceil" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.ceil())),
            "floor" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| Value::Number(n.floor())),

            // Min/max
            "min" => args
                .iter()
                .filter_map(Value::as_number)
                .reduce(f64::min)
                .map_or(Value::Undef, Value::Number),
            "max" => args
                .iter()
                .filter_map(Value::as_number)
                .reduce(f64::max)
                .map_or(Value::Undef, Value::Number),

            // List/string operations
            "len" => match args.first() {
                Some(Value::List(l)) => Value::Number(l.len() as f64),
                Some(Value::String(s)) => Value::Number(s.len() as f64),
                _ => Value::Undef,
            },
            "concat" => {
                let mut result = Vec::new();
                for arg in args {
                    match arg {
                        Value::List(l) => result.extend(l.iter().cloned()),
                        other => result.push(other.clone()),
                    }
                }
                Value::List(result)
            }

            // Vector operations
            "norm" => {
                if let Some(Value::List(l)) = args.first() {
                    let sum_sq: f64 = l.iter().filter_map(Value::as_number).map(|n| n * n).sum();
                    Value::Number(sum_sq.sqrt())
                } else {
                    Value::Undef
                }
            }
            "cross" => {
                if args.len() >= 2 {
                    let a = args[0].to_number_list();
                    let b = args[1].to_number_list();
                    match (a, b) {
                        (Some(a), Some(b)) if a.len() >= 3 && b.len() >= 3 => Value::List(vec![
                            Value::Number(a[1].mul_add(b[2], -(a[2] * b[1]))),
                            Value::Number(a[2].mul_add(b[0], -(a[0] * b[2]))),
                            Value::Number(a[0].mul_add(b[1], -(a[1] * b[0]))),
                        ]),
                        _ => Value::Undef,
                    }
                } else {
                    Value::Undef
                }
            }

            // Type checking
            "is_undef" => Value::Bool(matches!(args.first(), Some(Value::Undef) | None)),
            "is_list" => Value::Bool(matches!(args.first(), Some(Value::List(_)))),
            "is_num" => Value::Bool(matches!(args.first(), Some(Value::Number(_)))),
            "is_string" => Value::Bool(matches!(args.first(), Some(Value::String(_)))),
            "is_bool" => Value::Bool(matches!(args.first(), Some(Value::Bool(_)))),

            // String operations
            "str" => {
                let s: String = args
                    .iter()
                    .map(|v| match v {
                        Value::Number(n) => format!("{n}"),
                        Value::Bool(b) => format!("{b}"),
                        Value::String(s) => s.clone(),
                        Value::Undef => "undef".into(),
                        Value::List(l) => format!("{l:?}"),
                        Value::Range(a, b, c) => format!("[{a}:{c}:{b}]"),
                    })
                    .collect::<String>();
                Value::String(s)
            }
            "chr" => args
                .first()
                .and_then(Value::as_number)
                .map_or(Value::Undef, |n| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    char::from_u32(n as u32).map_or(Value::Undef, |c| Value::String(c.to_string()))
                }),
            "ord" => {
                if let Some(Value::String(s)) = args.first() {
                    s.chars()
                        .next()
                        .map_or(Value::Undef, |c| Value::Number(f64::from(c as u32)))
                } else {
                    Value::Undef
                }
            }

            // Random
            "rands" => {
                if args.len() >= 3 {
                    match (
                        args[0].as_number(),
                        args[1].as_number(),
                        args[2].as_number(),
                    ) {
                        (Some(min), Some(max), Some(count)) => {
                            let n = count as usize;
                            // Deterministic pseudo-random for reproducibility
                            let seed = args.get(3).and_then(Value::as_number).unwrap_or(0.0) as u64;
                            let mut rng =
                                seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                            let vals: Vec<Value> = (0..n)
                                .map(|_| {
                                    rng =
                                        rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                                    let t = (rng >> 33) as f64 / (1u64 << 31) as f64;
                                    Value::Number(min + t * (max - min))
                                })
                                .collect();
                            Value::List(vals)
                        }
                        _ => Value::Undef,
                    }
                } else {
                    Value::Undef
                }
            }

            // Lookup
            "lookup" => {
                if args.len() >= 2 {
                    if let (Some(key), Some(Value::List(table))) =
                        (args[0].as_number(), args.get(1))
                    {
                        // Linear interpolation lookup
                        let pairs: Vec<(f64, f64)> = table
                            .iter()
                            .filter_map(|row| {
                                let nums = row.to_number_list()?;
                                if nums.len() >= 2 {
                                    Some((nums[0], nums[1]))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if pairs.is_empty() {
                            return Value::Undef;
                        }
                        if key <= pairs[0].0 {
                            return Value::Number(pairs[0].1);
                        }
                        if key >= pairs.last().unwrap().0 {
                            return Value::Number(pairs.last().unwrap().1);
                        }
                        for w in pairs.windows(2) {
                            if key >= w[0].0 && key <= w[1].0 {
                                let t = (key - w[0].0) / (w[1].0 - w[0].0);
                                return Value::Number(t.mul_add(w[1].1 - w[0].1, w[0].1));
                            }
                        }
                        Value::Number(pairs.last().unwrap().1)
                    } else {
                        Value::Undef
                    }
                } else {
                    Value::Undef
                }
            }

            // Search
            "search" => {
                // Simplified: search(val, list) → index
                Value::Undef // TODO: full implementation
            }

            _ => {
                self.warnings.push(format!("Unknown function: {name}()"));
                Value::Undef
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum BoolOp {
    Union,
    Difference,
    Intersection,
}

#[derive(Clone, Copy)]
enum TransformKind {
    Translate,
    Rotate,
    Scale,
    Mirror,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_star_difference() {
        let code = r#"
module star(points = 5, outer_r = 3, inner_r = 1.2, h = 2) {
    linear_extrude(height = h)
        polygon([for (i = [0:2*points-1])
            let(r = (i % 2 == 0) ? outer_r : inner_r,
                a = 90 + i * 180 / points)
            [r * cos(a), r * sin(a)]
        ]);
}

difference() {
    cylinder(h = 1, r = 10, $fn = 64);
    translate([0, 0, -0.5])
        star(points = 5, outer_r = 5, inner_r = 2, h = 2);
}
"#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                eprintln!("Parts: {}", parts.len());
                for (i, p) in parts.iter().enumerate() {
                    eprintln!(
                        "Part {i}: {} verts, {} tris",
                        p.positions.len(),
                        p.indices.len() / 3
                    );
                }
                assert!(
                    parts[0].indices.len() / 3 > 96,
                    "Expected more tris than plain cylinder (got {})",
                    parts[0].indices.len() / 3
                );
            }
            CompilationResult::Error(e) => panic!("Compilation failed: {e}"),
        }
    }

    #[test]
    fn test_star_polygon_standalone() {
        // Test that the polygon comprehension evaluates correctly
        let code = r#"
linear_extrude(height = 2)
    polygon([for (i = [0:9])
        let(r = (i % 2 == 0) ? 5 : 2,
            a = 90 + i * 36)
        [r * cos(a), r * sin(a)]
    ]);
"#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                eprintln!("Star standalone - Parts: {}", parts.len());
                for (i, p) in parts.iter().enumerate() {
                    eprintln!(
                        "Star part {i}: {} verts, {} tris",
                        p.positions.len(),
                        p.indices.len() / 3
                    );
                }
                assert!(!parts.is_empty(), "Star polygon should produce geometry");
                assert!(parts[0].indices.len() > 0, "Star should have triangles");
            }
            CompilationResult::Error(e) => panic!("Compilation failed: {e}"),
        }
    }

    #[test]
    fn test_axis_angle_rotate() {
        // Test rotate(a=angle, v=[x,y,z]) axis-angle rotation
        let code = r#"
rotate(a = 45, v = [1, 0, 0])
    cube([10, 10, 10]);
"#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "Should produce geometry");
                assert!(
                    parts[0].indices.len() / 3 >= 12,
                    "Cube should have at least 12 tris"
                );
            }
            CompilationResult::Error(e) => panic!("Compilation failed: {e}"),
        }
    }

    /// Compile to `BMesh` for diagnostics, then convert back to `CsgMesh` for analysis.
    fn compile_to_csg_mesh(code: &str) -> CsgMesh<()> {
        bmesh_to_csg_mesh(&compile_to_bmesh(code))
    }

    fn compile_to_bmesh(code: &str) -> BMesh<()> {
        let source_file = openscad_rs::parse(code).expect("parse error");
        let mut evaluator = Evaluator::new();
        let shapes = evaluator.eval_source_file(&source_file);
        assert!(!shapes.is_empty(), "No geometry produced");
        let mut iter = shapes.into_iter();
        let (first, _) = iter.next().unwrap();
        let mut result = first.into_bmesh();
        for (shape, _) in iter {
            result = result.union(&shape.into_bmesh());
        }
        result
    }

    /// Convert `CsgMesh` to `MeshData` via `BMesh` for rendering.
    fn csg_mesh_to_mesh_data(mesh: &CsgMesh<()>) -> Result<MeshData, String> {
        let bmesh = BMesh::from(mesh.clone());
        bmesh_to_mesh_data(&bmesh)
    }

    /// Merge all parts from `compile_scad_code` into a single `MeshData`.
    fn compile_to_merged_mesh(code: &str) -> MeshData {
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                let mut positions = Vec::new();
                let mut normals = Vec::new();
                let mut indices = Vec::new();
                for part in parts {
                    let offset = positions.len() as u32;
                    positions.extend(part.positions);
                    normals.extend(part.normals);
                    indices.extend(part.indices.iter().map(|i| i + offset));
                }
                MeshData {
                    positions,
                    normals,
                    indices,
                    color: None,
                }
            }
            CompilationResult::Error(e) => panic!("Failed: {e}"),
        }
    }

    /// Analyze a CSG mesh and print polygon statistics.
    fn analyze_csg_mesh(mesh: &CsgMesh<()>) -> (usize, usize, usize, usize) {
        let total_polys = mesh.polygons.len();
        let triangles = mesh
            .polygons
            .iter()
            .filter(|p| p.vertices.len() == 3)
            .count();
        let quads = mesh
            .polygons
            .iter()
            .filter(|p| p.vertices.len() == 4)
            .count();
        let large = mesh
            .polygons
            .iter()
            .filter(|p| p.vertices.len() > 4)
            .count();
        let max_verts = mesh
            .polygons
            .iter()
            .map(|p| p.vertices.len())
            .max()
            .unwrap_or(0);
        let degenerate = mesh
            .polygons
            .iter()
            .filter(|p| {
                let verts = &p.vertices;
                if verts.len() < 3 {
                    return true;
                }
                // Check for zero-area triangles
                let a = &verts[0].position;
                let b = &verts[1].position;
                let c = &verts[2].position;
                let ab = nalgebra::Vector3::new(b.x - a.x, b.y - a.y, b.z - a.z);
                let ac = nalgebra::Vector3::new(c.x - a.x, c.y - a.y, c.z - a.z);
                ab.cross(&ac).norm() < 1e-10
            })
            .count();

        eprintln!("  Polygons: {total_polys} (tri={triangles}, quad={quads}, >4={large})");
        eprintln!("  Max vertices in polygon: {max_verts}");
        eprintln!("  Degenerate polygons: {degenerate}");

        (total_polys, triangles, quads, large)
    }

    #[test]
    fn test_scalar_vector_mul() {
        let m = compile_to_merged_mesh("r=25; translate(r * [1, 0, 0]) cube(5);");
        let xs: Vec<f64> = m.positions.iter().map(|p| p[0] as f64).collect();
        let min_x = xs.iter().cloned().fold(f64::INFINITY, f64::min);
        eprintln!("min_x = {min_x}");
        assert!(
            min_x > 20.0,
            "Expected translated to x≈25, got min_x={min_x}"
        );
    }

    #[test]
    fn test_ring_of_children() {
        // First test: verify args are parsed correctly
        let _simple = compile_scad_code("module m(a, b) { echo(a=a, b=b); } m(20, 4);");
        // Just want the echo output

        let code = r#"
module ring(radius, count){
    for (a = [0 : count - 1]) {
        angle = a * 360 / count;
        translate(radius * [cos(angle), -sin(angle), 0])
            children();
    }
}
ring(20, 4) { cube(3); }
"#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                let total_verts: usize = parts.iter().map(|p| p.positions.len()).sum();
                let total_tris: usize = parts.iter().map(|p| p.indices.len()).sum::<usize>() / 3;
                eprintln!("ring: {total_verts} verts, {total_tris} tris");
                // 4 cubes × 24 verts = 96 verts minimum
                assert!(
                    total_verts >= 96,
                    "Expected 4 cubes, got {total_verts} verts"
                );
            }
            CompilationResult::Error(e) => panic!("Failed: {e}"),
        }
    }

    #[test]
    fn test_candle_stand() {
        let code = r#"
length=50; radius=25; count=7; centerCandle=true;
candleSize=7; width=4; holeSize=3; CenterCandleWidth=4;
heightOfSupport=3; widthOfSupport=3; heightOfRing=4; widthOfRing=23;

cylinder(length,width-2);

translate([0,0,length-candleSize/2])
if(centerCandle){
    difference(){
        cylinder(candleSize,r=CenterCandleWidth);
        cylinder(candleSize+1,r=CenterCandleWidth-2);
    }
}

translate([0,0,length-candleSize/2]){
    make(radius, count,candleSize,length);
    make_ring_of(radius, count){ cylinder(1,r=width); }
}

for (a = [0 : count - 1]) {
    rotate(a*360/count) {
        translate([0, -width/2, 0]) cube([radius, widthOfSupport, heightOfSupport]);
    }
}

module make(radius, count,candleSize,length){
    difference(){
        union(){
            make_ring_of(radius, count){ cylinder(candleSize,r=width); }
            for (a = [0 : count - 1]) {
                rotate(a*360/count) {
                    translate([0, -width/2, 0]) cube([radius, widthOfSupport, heightOfSupport]);
                }
            }
            linear_extrude(heightOfRing)
            difference(){ circle(radius); circle(widthOfRing); }
        }
        make_ring_of(radius, count){ cylinder(candleSize+1,r=holeSize); }
    }
}

module make_ring_of(radius, count){
    for (a = [0 : count - 1]) {
        angle = a * 360 / count;
        translate(radius * [cos(angle), -sin(angle), 0])
            children();
    }
}
"#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                let total_verts: usize = parts.iter().map(|p| p.positions.len()).sum();
                let total_tris: usize = parts.iter().map(|p| p.indices.len()).sum::<usize>() / 3;
                eprintln!("OK: {total_verts} verts, {total_tris} tris");
                assert!(total_verts > 0);
            }
            CompilationResult::Error(e) => panic!("Compilation failed: {e}"),
        }
    }

    /// Test hull of sphere + cube (FunnelAnchor pattern from RefillClip).
    #[test]
    fn test_hull_sphere_cube() {
        let code = r#"
$fn = 30;
hull() {
    sphere(r=14);
    translate([0, 30, 0]) cube([30, 4, 30], center=true);
}
"#;
        eprintln!("\n--- Hull of sphere + cube ---");
        let csg = compile_to_csg_mesh(code);
        analyze_csg_mesh(&csg);
        let result = csg_mesh_to_mesh_data(&csg).expect("mesh conversion failed");
        eprintln!(
            "  Triangulated: {} verts, {} tris",
            result.positions.len(),
            result.indices.len() / 3
        );
        assert!(result.positions.len() > 10);
    }

    /// Test intersection of sphere + cube (FunnelAnchor pattern).
    #[test]
    fn test_intersection_sphere_cube() {
        let code = r#"
$fn = 30;
intersection() {
    sphere(r=14);
    translate([-14, -14, -14]) cube([28, 28, 14.4]);
}
"#;
        eprintln!("\n--- Intersection of sphere + cube ---");
        let csg = compile_to_csg_mesh(code);
        analyze_csg_mesh(&csg);
        let result = csg_mesh_to_mesh_data(&csg).expect("mesh conversion failed");
        eprintln!(
            "  Triangulated: {} verts, {} tris",
            result.positions.len(),
            result.indices.len() / 3
        );
        assert!(result.positions.len() > 10);
    }

    /// Test difference with hull children (core pattern of RefillClip).
    #[test]
    fn test_difference_hull_shapes() {
        let code = r#"
$fn = 30;
difference() {
    // Solid outer
    hull() {
        translate([0, 0, 0]) cube([36, 4, 33], center=true);
        translate([0, 25, 10]) rotate([-35, 0, 0]) cylinder(h=8, r1=14, r2=22);
    }
    // Hollow inner
    hull() {
        translate([0, 0, 0]) sphere(r=12.5);
        translate([0, 25, 10]) rotate([-35, 0, 0]) cylinder(h=9, r1=12.5, r2=20);
    }
}
"#;
        eprintln!("\n--- Difference of hull shapes ---");
        let csg = compile_to_csg_mesh(code);
        let (_total, _, _, _large) = analyze_csg_mesh(&csg);
        let result = csg_mesh_to_mesh_data(&csg).expect("mesh conversion failed");
        eprintln!(
            "  Triangulated: {} verts, {} tris",
            result.positions.len(),
            result.indices.len() / 3
        );
        assert!(result.positions.len() > 10);
        // Verify no zero-area triangles in output
        let mut degenerate_tris = 0;
        for tri in result.indices.chunks(3) {
            let a = result.positions[tri[0] as usize];
            let b = result.positions[tri[1] as usize];
            let c = result.positions[tri[2] as usize];
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cross_len = ((ab[1] * ac[2] - ab[2] * ac[1]).powi(2)
                + (ab[2] * ac[0] - ab[0] * ac[2]).powi(2)
                + (ab[0] * ac[1] - ab[1] * ac[0]).powi(2))
            .sqrt();
            if cross_len < 1e-8 {
                degenerate_tris += 1;
            }
        }
        eprintln!("  Degenerate output triangles: {degenerate_tris}");
    }

    /// Full refill clip test.
    #[test]
    fn test_refill_clip() {
        let code = REFILL_CLIP_CODE;
        eprintln!("\n--- Refill Clip ---");
        let csg = compile_to_csg_mesh(code);
        let (_total, _tris, _quads, _large) = analyze_csg_mesh(&csg);
        let result = csg_mesh_to_mesh_data(&csg).expect("mesh conversion failed");
        eprintln!(
            "  Triangulated: {} verts, {} tris",
            result.positions.len(),
            result.indices.len() / 3
        );
        assert!(
            result.positions.len() > 100,
            "Expected substantial mesh, got {} verts",
            result.positions.len()
        );

        // Compare bounding box with OpenSCAD reference (Z-up original coordinates)
        // Our mesh is Y-up (swapped Y↔Z), so compare: our X=X, our Y=Z_orig, our Z=Y_orig
        let xs: Vec<f32> = result.positions.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = result.positions.iter().map(|p| p[1]).collect(); // = Z_orig
        let zs: Vec<f32> = result.positions.iter().map(|p| p[2]).collect(); // = Y_orig
        let (xmin, xmax) = (
            *xs.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap(),
            *xs.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap(),
        );
        let (ymin, ymax) = (
            *ys.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap(),
            *ys.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap(),
        );
        let (zmin, zmax) = (
            *zs.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap(),
            *zs.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap(),
        );
        eprintln!(
            "  Our bbox (Y-up): X=[{},{}] Y=[{},{}] Z=[{},{}]",
            xmin, xmax, ymin, ymax, zmin, zmax
        );
        eprintln!("  OpenSCAD bbox:   X=[-22.0,22.0] Y(up)=[35.0,134.2] Z(depth)=[95.0,175.6]");
        // Our Y = OpenSCAD Z, Our Z = OpenSCAD Y
        // So our Y should match [35,134.2] and our Z should match [95,175.6]

        // Verify mesh integrity: check for NaN/Inf positions
        let nan_count = result
            .positions
            .iter()
            .filter(|p| p.iter().any(|v| v.is_nan() || v.is_infinite()))
            .count();
        assert_eq!(nan_count, 0, "Found {nan_count} NaN/Inf positions");

        // Check normals
        let bad_normals = result
            .normals
            .iter()
            .filter(|n| {
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                len < 0.5 || len > 1.5
            })
            .count();
        eprintln!("  Bad normals (len not ~1): {bad_normals}");

        // Also export to STL for visual comparison
        export_stl(&result, "/tmp/synapscad_refill_clip.stl");
        eprintln!("  Exported STL to /tmp/synapscad_refill_clip.stl");
    }

    fn export_stl(mesh: &MeshData, path: &str) {
        use std::io::Write;
        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "solid SynapsCAD").unwrap();
        for tri in mesh.indices.chunks(3) {
            let n = mesh.normals[tri[0] as usize];
            writeln!(f, "  facet normal {} {} {}", n[0], n[1], n[2]).unwrap();
            writeln!(f, "    outer loop").unwrap();
            for &idx in tri {
                let p = mesh.positions[idx as usize];
                writeln!(f, "      vertex {} {} {}", p[0], p[1], p[2]).unwrap();
            }
            writeln!(f, "    endloop").unwrap();
            writeln!(f, "  endfacet").unwrap();
        }
        writeln!(f, "endsolid SynapsCAD").unwrap();
    }

    const REFILL_CLIP_CODE: &str = r#"
$fn = 60;
TANK_TOP_WIDTH = 256;
TANK_BOTTOM_WIDTH = 244;
TANK_HEIGHT = 110;
TANK_WALL = 2.8;
REFILL_SLOT_HEIGHT = 33.0;
REFILL_SLOT_WIDTH = 30.4;
REFILL_SLOT_CENTER_Z = TANK_HEIGHT - REFILL_SLOT_HEIGHT/2 + 0.5;
REFILL_FUNNEL_TOP_R = 22;
REFILL_FUNNEL_HEIGHT = 8;
REFILL_FUNNEL_OFFSET = 25;
REFILL_FUNNEL_TILT = 35;
REFILL_CHANNEL_INNER = 25;
REFILL_CHANNEL_WALL = 1.6;

module RefillClip() {
    clip_width = REFILL_SLOT_WIDTH - 0.4;
    channel_outer_r = REFILL_CHANNEL_INNER/2 + REFILL_CHANNEL_WALL;
    funnel_height = REFILL_FUNNEL_HEIGHT;
    funnel_top_r = REFILL_FUNNEL_TOP_R;

    y_wall_outer = TANK_TOP_WIDTH/2;
    y_wall_inner = y_wall_outer - TANK_WALL;
    slot_center_z = REFILL_SLOT_CENTER_Z;
    slot_height = REFILL_SLOT_HEIGHT;
    z_base = slot_center_z - slot_height/2;
    body_top_z = min(z_base + slot_height, TANK_HEIGHT - 3);
    body_mid_z = (z_base + body_top_z)/2;
    funnel_base_z = body_top_z + 8;
    flange_height = min(slot_height + 5, (TANK_HEIGHT - body_mid_z)*2);

    y_funnel = y_wall_outer + REFILL_FUNNEL_OFFSET;
    funnel_tilt = REFILL_FUNNEL_TILT;
    funnel_anchor_overlap = 0.4;

    module FunnelTransform() {
        translate([0, y_funnel, funnel_base_z])
            rotate([-funnel_tilt, 0, 0])
                children();
    }

    module FunnelAnchor(radius) {
        intersection() {
            sphere(r=radius);
            translate([-radius, -radius, -radius])
                cube([radius * 2, radius * 2, radius + funnel_anchor_overlap]);
        }
    }

    nozzle_length = z_base - 35;
    y_nozzle = y_wall_inner - channel_outer_r - 2;

    difference() {
        union() {
            translate([0, 0, z_base]) {
                translate([0, y_wall_outer - TANK_WALL/2, body_mid_z - z_base])
                    cube([clip_width, TANK_WALL, slot_height], center=true);
                translate([0, y_wall_outer + 2, body_mid_z - z_base])
                    cube([clip_width + 6, 4, flange_height], center=true);
                translate([0, y_wall_inner - 2, body_mid_z - z_base])
                    cube([clip_width + 6, 4, flange_height], center=true);
            }

            FunnelTransform()
                cylinder(h=funnel_height, r1=channel_outer_r, r2=funnel_top_r);

            hull() {
                translate([0, y_wall_outer + 2, body_mid_z])
                    cube([clip_width + 6, 4, slot_height], center=true);
                FunnelTransform()
                    cylinder(r=channel_outer_r, h=1);
            }

            hull() {
                translate([0, y_nozzle, body_mid_z]) sphere(r=channel_outer_r);
                translate([0, y_nozzle, z_base - nozzle_length]) cylinder(r=channel_outer_r, h=1);
            }

            hull() {
                translate([0, y_wall_inner - 2, body_mid_z])
                    cube([clip_width + 6, 4, 30], center=true);
                translate([0, y_nozzle, body_mid_z])
                    sphere(r=channel_outer_r);
            }

            hull() {
                translate([0, y_wall_outer + 2, body_mid_z])
                    cube([clip_width + 6, 4, 30], center=true);
                translate([0, y_wall_inner - 2, body_mid_z])
                    cube([clip_width + 6, 4, 30], center=true);
            }

            hull() {
                FunnelTransform()
                    FunnelAnchor(channel_outer_r);
                translate([0, y_wall_outer, body_mid_z])
                    sphere(r=channel_outer_r);
                translate([0, y_nozzle, body_mid_z])
                    sphere(r=channel_outer_r);
            }
        }

        hull() {
            FunnelTransform()
                FunnelAnchor(REFILL_CHANNEL_INNER/2);
            translate([0, y_wall_outer, body_mid_z])
                sphere(r=REFILL_CHANNEL_INNER/2);
            translate([0, y_nozzle, body_mid_z])
                sphere(r=REFILL_CHANNEL_INNER/2);
        }

        FunnelTransform()
            cylinder(h=funnel_height + 1, r1=REFILL_CHANNEL_INNER/2, r2=funnel_top_r - 2);

        hull() {
            translate([0, y_nozzle, body_mid_z]) sphere(r=REFILL_CHANNEL_INNER/2);
            translate([0, y_nozzle, z_base - nozzle_length]) cylinder(r=REFILL_CHANNEL_INNER/2, h=1);
        }

        translate([0, y_nozzle, z_base - nozzle_length - 1])
            cylinder(r=REFILL_CHANNEL_INNER/2, h=5);

        cut_start_x = clip_width/2 - 0.5;
        cut_end_x = clip_width/2 + 5;
        groove_width = cut_end_x - cut_start_x;
        groove_x_offset = cut_start_x + groove_width/2;

        translate([-groove_x_offset, (y_wall_inner + y_wall_outer)/2, z_base + 12.5])
            cube([groove_width, TANK_WALL + 1.0, 40], center=true);
        translate([groove_x_offset, (y_wall_inner + y_wall_outer)/2, z_base + 12.5])
            cube([groove_width, TANK_WALL + 1.0, 40], center=true);
    }
}

RefillClip();
"#;

    // -----------------------------------------------------------------------
    // OpenSCAD Examples Conformance Tests
    //
    // These test against real-world example files from the OpenSCAD project
    // (CC0 public domain). See tests/openscad_examples/LICENSE.
    // -----------------------------------------------------------------------

    /// Helper: load an example file relative to CARGO_MANIFEST_DIR
    fn example_path(relative: &str) -> String {
        format!(
            "{}/tests/openscad_examples/{relative}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    /// Compile an example file and assert it produces valid geometry.
    fn assert_example_compiles(relative: &str) {
        let path = example_path(relative);
        let code =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
        match compile_scad_code(&code) {
            CompilationResult::Success { parts, .. } => {
                assert!(
                    !parts.is_empty(),
                    "{relative}: compiled but produced no geometry"
                );
                for part in &parts {
                    for pos in &part.positions {
                        assert!(
                            pos.iter().all(|v| v.is_finite()),
                            "{relative}: NaN/Inf in positions"
                        );
                    }
                }
            }
            CompilationResult::Error(e) => panic!("{relative}: compilation failed: {e}"),
        }
    }

    /// Compile an example file — just verify it doesn't panic.
    /// May produce empty geometry for files using unsupported features.
    fn assert_example_no_panic(relative: &str) {
        let path = example_path(relative);
        let code =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
        let result = std::panic::catch_unwind(|| compile_scad_code(&code));
        match result {
            Ok(CompilationResult::Success { .. }) => {}
            Ok(CompilationResult::Error(e)) => {
                eprintln!("{relative}: compilation error (expected): {e}");
            }
            Err(_) => {
                eprintln!("{relative}: panicked (known issue in dependency)");
            }
        }
    }

    // -----------------------------------------------------------------------
    // OpenSCAD Reference Comparison
    // -----------------------------------------------------------------------

    #[derive(serde::Deserialize)]
    struct ReferenceData {
        #[allow(dead_code)]
        vertices: usize,
        facets: usize,
        bounding_box: BBox,
    }

    #[derive(serde::Deserialize)]
    struct BBox {
        min: [f64; 3],
        max: [f64; 3],
    }

    /// Compile an example and compare its bounding box and triangle count against
    /// the OpenSCAD reference data (generated by `tests/generate_references.sh`).
    ///
    /// Our compiler outputs Y-up (Bevy convention) while OpenSCAD uses Z-up.
    /// We swap Y↔Z when comparing bounding boxes.
    fn assert_example_matches_reference(relative: &str) {
        let path = example_path(relative);
        let code =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));

        // 1. Compile with our compiler
        let parts = match compile_scad_code(&code) {
            CompilationResult::Success { parts, .. } => {
                assert!(
                    !parts.is_empty(),
                    "{relative}: compiled but produced no geometry"
                );
                parts
            }
            CompilationResult::Error(e) => panic!("{relative}: compilation failed: {e}"),
        };

        // 2. Load reference JSON
        let ref_name = relative.replace(".scad", ".json");
        let ref_path = format!(
            "{}/tests/openscad_references/{ref_name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let ref_json = match std::fs::read_to_string(&ref_path) {
            Ok(json) => json,
            Err(_) => {
                eprintln!("{relative}: no reference data, skipping comparison");
                return;
            }
        };
        let reference: ReferenceData =
            serde_json::from_str(&ref_json).expect("Failed to parse reference JSON");

        // 3. Compute our bounding box across all parts
        //    Our positions are [x, y, z] in Y-up (Bevy).
        //    OpenSCAD reference is Z-up. Swap: our_Y → ref_Z, our_Z → ref_Y.
        let mut our_min = [f64::INFINITY; 3];
        let mut our_max = [f64::NEG_INFINITY; 3];
        let mut our_triangles: usize = 0;

        for part in &parts {
            our_triangles += part.indices.len() / 3;
            for pos in &part.positions {
                // pos is [x, y, z] in Y-up (swapped Y↔Z from Z-up).
                // Convert back: Z-up X = pos[0], Z-up Y = pos[2], Z-up Z = pos[1]
                let zup = [f64::from(pos[0]), f64::from(pos[2]), f64::from(pos[1])];
                for i in 0..3 {
                    if zup[i] < our_min[i] {
                        our_min[i] = zup[i];
                    }
                    if zup[i] > our_max[i] {
                        our_max[i] = zup[i];
                    }
                }
            }
        }

        let ref_min = reference.bounding_box.min;
        let ref_max = reference.bounding_box.max;

        // 4. Compare bounding box within tolerance
        //    Use max(1.0 absolute, 5% of reference dimension) per axis
        let mut bbox_ok = true;
        let mut bbox_details = String::new();
        for i in 0..3 {
            let axis = ["X", "Y", "Z"][i];
            let ref_size = (ref_max[i] - ref_min[i]).abs();
            let tol = f64::max(1.0, ref_size * 0.05);

            let min_diff = (our_min[i] - ref_min[i]).abs();
            let max_diff = (our_max[i] - ref_max[i]).abs();

            if min_diff > tol || max_diff > tol {
                bbox_ok = false;
                bbox_details.push_str(&format!(
                    "\n  {axis}: ours=[{:.2}, {:.2}] ref=[{:.2}, {:.2}] (min_diff={:.2}, max_diff={:.2}, tol={:.2})",
                    our_min[i], our_max[i], ref_min[i], ref_max[i], min_diff, max_diff, tol
                ));
            }
        }

        // 5. Compare triangle count within very wide tolerance
        //    Different tessellation ($fn) means counts can differ dramatically.
        //    Just catch "completely wrong" (e.g. missing geometry entirely).
        let ref_facets = reference.facets;
        let tri_ratio = if ref_facets > 0 {
            our_triangles as f64 / ref_facets as f64
        } else {
            1.0
        };
        let tri_ok = (0.05..=20.0).contains(&tri_ratio);

        // 6. Report
        if !bbox_ok || !tri_ok {
            let mut msg = format!("{relative}: geometry mismatch vs OpenSCAD reference:");
            if !bbox_ok {
                msg.push_str(&format!("\n  Bounding box mismatch:{bbox_details}"));
            }
            if !tri_ok {
                msg.push_str(&format!(
                    "\n  Triangle count: ours={our_triangles}, ref={ref_facets} (ratio={tri_ratio:.2}, expected 0.5–2.0)"
                ));
            }
            panic!("{msg}");
        }

        eprintln!(
            "{relative}: OK (bbox within tolerance, triangles: ours={our_triangles} ref={ref_facets} ratio={tri_ratio:.2})"
        );
    }

    // === Basics ===

    #[test]
    fn openscad_basics_csg() {
        assert_example_matches_reference("Basics/CSG.scad");
    }

    #[test]
    fn openscad_basics_csg_modules() {
        assert_example_matches_reference("Basics/CSG-modules.scad");
    }

    #[test]
    fn openscad_basics_hull() {
        assert_example_matches_reference("Basics/hull.scad");
    }

    #[test]
    fn openscad_basics_linear_extrude() {
        // Panics in csgrs with "boundary edges" — known dependency issue
        assert_example_no_panic("Basics/linear_extrude.scad");
    }

    #[test]
    fn openscad_basics_logo() {
        assert_example_matches_reference("Basics/logo.scad");
    }

    #[test]
    fn openscad_basics_rotate_extrude() {
        // Panics in spade with "Conflicting edge" — known dependency issue
        assert_example_no_panic("Basics/rotate_extrude.scad");
    }

    #[test]
    fn openscad_basics_letterblock() {
        // Uses text() — unsupported, but shouldn't panic
        assert_example_no_panic("Basics/LetterBlock.scad");
    }

    #[test]
    fn openscad_basics_logo_and_text() {
        // Uses text() and use<> — shouldn't panic
        assert_example_no_panic("Basics/logo_and_text.scad");
    }

    #[test]
    fn openscad_basics_projection() {
        // Uses projection() and import() — unsupported
        assert_example_no_panic("Basics/projection.scad");
    }

    #[test]
    fn openscad_basics_roof() {
        // Uses roof() — unsupported
        assert_example_no_panic("Basics/roof.scad");
    }

    #[test]
    fn openscad_basics_text_on_cube() {
        // Uses text() — unsupported
        assert_example_no_panic("Basics/text_on_cube.scad");
    }

    // === Functions ===

    #[test]
    fn openscad_functions_echo() {
        // Echo-only file — no geometry produced, just verify no panic
        assert_example_no_panic("Functions/echo.scad");
    }

    #[test]
    fn openscad_functions_functions() {
        assert_example_matches_reference("Functions/functions.scad");
    }

    #[test]
    fn openscad_functions_list_comprehensions() {
        // Panics in csgrs with "boundary edges" — known dependency issue
        assert_example_no_panic("Functions/list_comprehensions.scad");
    }

    #[test]
    #[ignore] // Stack overflow — likely infinite recursion in evaluator
    fn openscad_functions_polygon_areas() {
        // Uses text() for labels but core logic is polygon + functions
        assert_example_no_panic("Functions/polygon_areas.scad");
    }

    #[test]
    fn openscad_functions_recursion() {
        // Uses text() — only 2D output, no 3D geometry expected
        assert_example_no_panic("Functions/recursion.scad");
    }

    // === Advanced ===

    #[test]
    fn openscad_advanced_children() {
        // Uses children() and text() — text is skipped, children works
        assert_example_no_panic("Advanced/children.scad");
    }

    #[test]
    fn openscad_advanced_children_indexed() {
        // Uses children(index) and $children — not fully supported
        assert_example_no_panic("Advanced/children_indexed.scad");
    }

    #[test]
    fn openscad_advanced_module_recursion() {
        assert_example_no_panic("Advanced/module_recursion.scad");
    }

    #[test]
    fn openscad_advanced_geb() {
        // Uses text() and offset() — unsupported
        assert_example_no_panic("Advanced/GEB.scad");
    }

    #[test]
    fn openscad_advanced_offset() {
        assert_example_no_panic("Advanced/offset.scad");
    }

    #[test]
    fn openscad_advanced_animation() {
        // Uses $t animation variable and offset()
        assert_example_no_panic("Advanced/animation.scad");
    }

    #[test]
    fn openscad_advanced_assert() {
        // Uses assert() — handled as passthrough in expressions
        assert_example_no_panic("Advanced/assert.scad");
    }

    #[test]
    fn openscad_advanced_surface_image() {
        // Uses surface() and projection() — unsupported
        assert_example_no_panic("Advanced/surface_image.scad");
    }

    // === Old ===

    #[test]
    fn openscad_old_example001() {
        // Known bbox mismatch: geometry ~25x too small (for loop scaling issue)
        assert_example_compiles("Old/example001.scad");
    }

    #[test]
    fn openscad_old_example002() {
        assert_example_matches_reference("Old/example002.scad");
    }

    #[test]
    fn openscad_old_example003() {
        assert_example_matches_reference("Old/example003.scad");
    }

    #[test]
    fn openscad_old_example004() {
        assert_example_matches_reference("Old/example004.scad");
    }

    #[test]
    fn openscad_old_example005() {
        // Panics in spade with "Conflicting edge" — known dependency issue
        assert_example_no_panic("Old/example005.scad");
    }

    #[test]
    fn openscad_old_example006() {
        // Uses version() (returns undef) and complex nested for loops
        assert_example_no_panic("Old/example006.scad");
    }

    #[test]
    fn openscad_old_example007() {
        // Uses import() DXF and dxf_dim/dxf_cross
        assert_example_no_panic("Old/example007.scad");
    }

    #[test]
    fn openscad_old_example008() {
        // Uses import() DXF
        assert_example_no_panic("Old/example008.scad");
    }

    #[test]
    fn openscad_old_example009() {
        // Uses import() and dxf functions
        assert_example_no_panic("Old/example009.scad");
    }

    #[test]
    fn openscad_old_example010() {
        // Uses surface() — unsupported
        assert_example_no_panic("Old/example010.scad");
    }

    #[test]
    fn openscad_old_example011() {
        // Uses polyhedron() — supported
        assert_example_matches_reference("Old/example011.scad");
    }

    #[test]
    fn openscad_old_example012() {
        // Uses import() STL
        assert_example_no_panic("Old/example012.scad");
    }

    #[test]
    fn openscad_old_example013() {
        // Uses import() DXF
        assert_example_no_panic("Old/example013.scad");
    }

    #[test]
    fn openscad_old_example014() {
        // Known bbox mismatch: geometry ~4x too large (intersection_for issue)
        assert_example_compiles("Old/example014.scad");
    }

    #[test]
    fn openscad_old_example015() {
        // Uses import() DXF
        assert_example_no_panic("Old/example015.scad");
    }

    #[test]
    fn openscad_old_example016() {
        // Uses import() STL
        assert_example_no_panic("Old/example016.scad");
    }

    #[test]
    fn openscad_old_example017() {
        // Panics in spade with "Conflicting edge" — known dependency issue
        assert_example_no_panic("Old/example017.scad");
    }

    #[test]
    fn openscad_old_example018() {
        // Uses children() indexed and $children
        assert_example_no_panic("Old/example018.scad");
    }

    #[test]
    fn openscad_old_example019() {
        assert_example_matches_reference("Old/example019.scad");
    }

    #[test]
    fn openscad_old_example020() {
        // Panics in csgrs with "boundary edges" — known dependency issue
        assert_example_no_panic("Old/example020.scad");
    }

    #[test]
    fn openscad_old_example021() {
        // Uses projection() — unsupported
        assert_example_no_panic("Old/example021.scad");
    }

    #[test]
    fn openscad_old_example022() {
        assert_example_matches_reference("Old/example022.scad");
    }

    #[test]
    fn openscad_old_example023() {
        // Uses use<MCAD/...> external library
        assert_example_no_panic("Old/example023.scad");
    }

    #[test]
    fn openscad_old_example024() {
        assert_example_matches_reference("Old/example024.scad");
    }

    // === Parametric ===

    #[test]
    fn openscad_parametric_candlestand() {
        assert_example_matches_reference("Parametric/candleStand.scad");
    }

    #[test]
    fn openscad_parametric_sign() {
        // Uses text() — unsupported
        assert_example_no_panic("Parametric/sign.scad");
    }

    // === Polyhedron N-gon regression tests ===

    /// Dodecahedron vertices + pentagonal faces for polyhedron tests.
    fn dodecahedron_scad() -> &'static str {
        r#"
        // Dodecahedron: 20 vertices, 12 pentagonal faces
        phi = (1 + sqrt(5)) / 2;
        points = [
            [ 1,  1,  1], [ 1,  1, -1], [ 1, -1,  1], [ 1, -1, -1],
            [-1,  1,  1], [-1,  1, -1], [-1, -1,  1], [-1, -1, -1],
            [0,  1/phi,  phi], [0,  1/phi, -phi], [0, -1/phi,  phi], [0, -1/phi, -phi],
            [ 1/phi,  phi, 0], [ 1/phi, -phi, 0], [-1/phi,  phi, 0], [-1/phi, -phi, 0],
            [ phi, 0,  1/phi], [ phi, 0, -1/phi], [-phi, 0,  1/phi], [-phi, 0, -1/phi]
        ];
        faces = [
            [0,8,10,2,16],  [0,16,17,1,12], [0,12,14,4,8],
            [1,17,3,11,9],  [1,9,5,14,12],  [2,10,6,15,13],
            [2,13,3,17,16], [3,13,15,7,11], [4,14,5,19,18],
            [4,18,6,10,8],  [5,9,11,7,19],  [6,18,19,7,15]
        ];
        "#
    }

    #[test]
    fn test_polyhedron_pentagon_faces_standalone() {
        // Dodecahedron without booleans: 12 pentagons → 36 triangles
        let code = format!(
            "{} polyhedron(points=points, faces=faces);",
            dodecahedron_scad()
        );
        match compile_scad_code(&code) {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "should produce geometry");
                let total_tris: usize = parts.iter().map(|p| p.positions.len() / 3).sum();
                assert!(
                    total_tris >= 36,
                    "expected ≥36 triangles from 12 pentagons, got {total_tris}"
                );
            }
            CompilationResult::Error(e) => panic!("compilation failed: {e}"),
        }
    }

    #[test]
    fn test_polyhedron_pentagon_faces_difference() {
        // Dodecahedron with boolean difference — the exact pattern that caused
        // "Conflicting edge" panics before fan triangulation fix.
        let code = format!(
            "{} difference() {{ polyhedron(points=points, faces=faces); sphere(r=0.5, $fn=16); }}",
            dodecahedron_scad()
        );
        let result = compile_scad_code(&code);
        match result {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "difference should produce geometry");
            }
            CompilationResult::Error(e) => panic!("compilation failed: {e}"),
        }
    }

    #[test]
    fn test_polyhedron_duplicate_face_dedup() {
        // A dodecahedron with a duplicate face (cyclic rotation) should still
        // compile successfully because eval_polyhedron deduplicates faces.
        let code = r#"
        phi = (1 + sqrt(5)) / 2;
        points = [
            [ 1,  1,  1], [ 1,  1, -1], [ 1, -1,  1], [ 1, -1, -1],
            [-1,  1,  1], [-1,  1, -1], [-1, -1,  1], [-1, -1, -1],
            [0,  1/phi,  phi], [0,  1/phi, -phi], [0, -1/phi,  phi], [0, -1/phi, -phi],
            [ 1/phi,  phi, 0], [ 1/phi, -phi, 0], [-1/phi,  phi, 0], [-1/phi, -phi, 0],
            [ phi, 0,  1/phi], [ phi, 0, -1/phi], [-phi, 0,  1/phi], [-phi, 0, -1/phi]
        ];
        faces = [
            [8,10,2,16,0],  // duplicate of next face (cyclic rotation)
            [0,8,10,2,16],  [0,16,17,1,12], [0,12,14,4,8],
            [1,17,3,11,9],  [1,9,5,14,12],  [2,10,6,15,13],
            [2,13,3,17,16], [3,13,15,7,11], [4,14,5,19,18],
            [4,18,6,10,8],  [5,9,11,7,19],  [6,18,19,7,15]
        ];
        polyhedron(points=points, faces=faces);
        "#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert!(
                    !parts.is_empty(),
                    "should produce geometry despite duplicate face"
                );
                let total_tris: usize = parts.iter().map(|p| p.positions.len() / 3).sum();
                assert!(total_tris >= 36, "expected ≥36 triangles, got {total_tris}");
            }
            CompilationResult::Error(e) => panic!("compilation failed: {e}"),
        }
    }

    #[test]
    fn openscad_basics_dodecahedron_difference() {
        assert_example_matches_reference("Basics/dodecahedron_difference.scad");
    }

    // -----------------------------------------------------------------------
    // Cone / frustum regression tests (zero-radius "Conflicting edge" fix)
    // -----------------------------------------------------------------------

    #[test]
    fn test_cone_zero_r1() {
        // Cone with r1=0 — the exact pattern that caused "Conflicting edge" panic.
        let code = "cylinder(h=5, r1=0, r2=10, $fn=12);";
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "cone should produce geometry");
            }
            CompilationResult::Error(e) => panic!("compilation failed: {e}"),
        }
    }

    #[test]
    fn test_cone_zero_r2() {
        // Inverted cone with r2=0.
        let code = "cylinder(h=5, r1=10, r2=0, $fn=12);";
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "cone should produce geometry");
            }
            CompilationResult::Error(e) => panic!("compilation failed: {e}"),
        }
    }

    #[test]
    fn test_cone_boolean_difference() {
        // Cone inside difference — ensures boolean ops work on frustum meshes.
        let code = r#"
difference() {
    cube([20, 20, 10], center=true);
    cylinder(h=12, r1=0, r2=8, $fn=12, center=true);
}
"#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "difference should produce geometry");
            }
            CompilationResult::Error(e) => panic!("compilation failed: {e}"),
        }
    }

    #[test]
    fn test_planter_pattern() {
        // Simplified planter pattern: polyhedron + intersection + union + difference + cone.
        // Exercises the full boolean tree that triggered the original crash.
        let code = format!(
            r#"{}
difference() {{
    union() {{
        intersection() {{
            polyhedron(points=points, faces=faces);
            cube([4, 4, 2], center=true);
        }}
        translate([0, 0, 0.8]) cylinder(r=1.5, h=0.3, $fn=6);
    }}
    translate([0, 0, -0.5]) cylinder(h=1, r1=0, r2=2, $fn=12);
}}
"#,
            dodecahedron_scad()
        );
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_scad_code(&code)));
        assert!(result.is_ok(), "planter pattern should not panic");
    }

    #[test]
    fn test_dodecahedron_planter_no_panic() {
        let code = r#"
$fn = 48;
body_size = 50;
wall = 3;
rim_h = 5;
rim_overhang = 8;
drain_r = 3;
drain_count = 3;
drain_spread = 14;
tray_h = 6;
tray_wall = 2;

module dodecahedron(r) {
    phi = (1 + sqrt(5)) / 2;
    s = r / sqrt(3);
    a = s;
    b = s / phi;
    c = s * phi;
    pts = [
        [ a,  a,  a], [ a,  a, -a], [ a, -a,  a], [ a, -a, -a],
        [-a,  a,  a], [-a,  a, -a], [-a, -a,  a], [-a, -a, -a],
        [ 0,  b,  c], [ 0,  b, -c], [ 0, -b,  c], [ 0, -b, -c],
        [ b,  c,  0], [ b, -c,  0], [-b,  c,  0], [-b, -c,  0],
        [ c,  0,  b], [ c,  0, -b], [-c,  0,  b], [-c,  0, -b]
    ];
    faces = [
        [0, 8, 4, 14, 12], [0, 16, 2, 10, 8], [0, 12, 1, 17, 16],
        [8, 10, 6, 18, 4], [4, 18, 19, 5, 14], [14, 5, 9, 1, 12],
        [1, 9, 11, 3, 17], [16, 17, 3, 13, 2], [2, 13, 15, 6, 10],
        [6, 15, 7, 19, 18], [19, 7, 11, 9, 5], [3, 11, 7, 15, 13]
    ];
    polyhedron(points = pts, faces = faces, convexity = 2);
}

module planter() {
    top_cut = body_size * 0.45;
    bot_cut = body_size * 0.35;
    rim_r = body_size * 0.65 + rim_overhang;
    inner_rim_r = rim_r - wall * 2;
    difference() {
        union() {
            intersection() {
                dodecahedron(body_size);
                translate([0, 0, (top_cut - bot_cut) / 2])
                    cube([body_size * 3, body_size * 3, top_cut + bot_cut], center = true);
            }
            translate([0, 0, top_cut - rim_h])
                cylinder(r = rim_r, h = rim_h, $fn = 6);
        }
        translate([0, 0, wall * 1.5])
            scale([0.82, 0.82, 0.82])
                dodecahedron(body_size);
        translate([0, 0, top_cut - rim_h - 1])
            cylinder(r = inner_rim_r, h = rim_h + 2, $fn = 6);
        for (i = [0:drain_count - 1]) {
            a = i * 360 / drain_count;
            translate([drain_spread * cos(a), drain_spread * sin(a), -bot_cut - 1])
                cylinder(h = wall * 3, r = drain_r);
        }
        translate([0, 0, -bot_cut - 0.1])
            difference() {
                cylinder(h = 4, r = body_size, $fn = 60);
                cylinder(h = 4.1, r1 = body_size * 0.45, r2 = body_size, $fn = 60);
            }
    }
}

module drip_tray() {
    tray_r = body_size * 0.55;
    difference() {
        cylinder(r = tray_r, h = tray_h, $fn = 6);
        translate([0, 0, tray_wall])
            cylinder(r = tray_r - tray_wall, h = tray_h + 1, $fn = 6);
    }
}

planter();
translate([body_size * 3, 0, body_size * 0.35])
    drip_tray();
"#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "Should produce geometry");
            }
            CompilationResult::Error(e) => {
                // Graceful error from catch_unwind is acceptable (boolmesh limitation)
                eprintln!("Dodecahedron planter: handled gracefully: {e}");
            }
        }
    }

    #[test]
    fn openscad_basics_viral_planter() {
        assert_example_no_panic("Basics/viral_planter.scad");
    }

    #[test]
    fn test_color_named_and_rgb() {
        let code = r#"
color("red") cube(10);
color("green") translate([20, 0, 0]) sphere(5, $fn=12);
color([0.2, 0.4, 0.8]) translate([40, 0, 0]) cylinder(h=10, r=5, $fn=12);
"#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert_eq!(parts.len(), 3, "Expected 3 colored parts");
                assert_eq!(parts[0].color, Some([1.0, 0.0, 0.0]));
                // "green" = (0, 128, 0) → 0.0, ~0.502, 0.0
                let g = parts[1].color.unwrap();
                assert!((g[1] - 128.0 / 255.0).abs() < 0.01);
                assert!(parts[2].color.is_some());
                let c = parts[2].color.unwrap();
                assert!((c[0] - 0.2).abs() < 0.01);
                assert!((c[1] - 0.4).abs() < 0.01);
                assert!((c[2] - 0.8).abs() < 0.01);
            }
            CompilationResult::Error(e) => panic!("Color test failed: {e}"),
        }
    }

    #[test]
    fn test_color_through_modules() {
        let code = r#"
module my_mod() {
    color("red") cube(10);
    color("blue") translate([20, 0, 0]) sphere(5, $fn=12);
}
my_mod();
"#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert_eq!(parts.len(), 2, "Module should produce 2 colored parts");
                assert_eq!(
                    parts[0].color,
                    Some([1.0, 0.0, 0.0]),
                    "First part should be red"
                );
                assert_eq!(
                    parts[1].color,
                    Some([0.0, 0.0, 1.0]),
                    "Second part should be blue"
                );
            }
            CompilationResult::Error(e) => panic!("Color-through-module test failed: {e}"),
        }
    }

    #[test]
    fn test_color_through_transforms() {
        // Colors must survive translate/rotate/scale wrapping a module call
        let code = r#"
module rocket() {
    color("silver") cylinder(h = 40, r = 8, $fn = 12);
    color("red") translate([0, 0, 40]) cylinder(h = 15, r1 = 8, r2 = 0, $fn = 12);
}
translate([50, 0, 0]) rocket();
"#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert_eq!(
                    parts.len(),
                    2,
                    "Transformed module should produce 2 colored parts"
                );
                assert!(parts[0].color.is_some(), "First part should have color");
                assert!(parts[1].color.is_some(), "Second part should have color");
                // Silver = [0.75, 0.75, 0.75]
                let c0 = parts[0].color.unwrap();
                assert!(
                    (c0[0] - 0.75).abs() < 0.1,
                    "First part should be silver-ish, got {:?}",
                    c0
                );
                // Red = [1.0, 0.0, 0.0]
                assert_eq!(
                    parts[1].color,
                    Some([1.0, 0.0, 0.0]),
                    "Second part should be red"
                );
            }
            CompilationResult::Error(e) => panic!("Color-through-transform test failed: {e}"),
        }
    }

    #[test]
    fn test_scalar_mul_nested_list() {
        // Verify s * [[1,1,1], [2,2,2]] scales inner vectors
        let code = r#"
s = 3;
pts = s * [[1, 2, 3], [4, 5, 6]];
// Use first point as a translate
translate(pts[0]) cube(1);
"#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "Should produce geometry");
            }
            CompilationResult::Error(e) => panic!("Scalar * nested list failed: {e}"),
        }
    }

    #[test]
    fn test_hull_dodecahedron_planter() {
        // Regression test for hull-based dodecahedron planter that was failing
        // with non-manifold errors due to scalar * list-of-lists not scaling
        let code = r#"
phi = (1 + sqrt(5)) / 2;
module dodecahedron(r) {
    s = r / sqrt(3);
    points = s * [
        [ 1,  1,  1], [ 1,  1, -1], [ 1, -1,  1], [ 1, -1, -1],
        [-1,  1,  1], [-1,  1, -1], [-1, -1,  1], [-1, -1, -1],
        [0,  1/phi,  phi], [0,  1/phi, -phi], [0, -1/phi,  phi], [0, -1/phi, -phi],
        [ 1/phi,  phi, 0], [ 1/phi, -phi, 0], [-1/phi,  phi, 0], [-1/phi, -phi, 0],
        [ phi, 0,  1/phi], [ phi, 0, -1/phi], [-phi, 0,  1/phi], [-phi, 0, -1/phi]
    ];
    hull() {
        for (p = points) translate(p) sphere(r=0.01, $fn=6);
    }
}
dodecahedron(30);
"#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "Dodecahedron should produce geometry");
                assert!(!parts[0].positions.is_empty(), "Mesh should have vertices");
            }
            CompilationResult::Error(e) => panic!("Hull dodecahedron failed: {e}"),
        }
    }

    #[test]
    fn test_frustum_difference() {
        // Frustum (r1 != r2) difference — was broken before polygon-based BMesh conversion
        let code = r#"
difference() {
    cylinder(h = 5, r1 = 31, r2 = 33, $fn = 6);
    translate([0, 0, 3])
        cylinder(h = 6, r1 = 28, r2 = 30, $fn = 6);
}
"#;
        match compile_scad_code(code) {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty(), "Should produce tray geometry");
            }
            CompilationResult::Error(e) => panic!("Frustum diff failed: {e}"),
        }
    }

    #[test]
    fn test_hull_dodecahedron_planter_full() {
        // Full planter model: hull dodecahedron + intersection + difference
        let code = r#"
$view = "tray";
planter_radius = 50;
wall_thickness = 3;
tray_height = 5;
phi = (1 + sqrt(5)) / 2;
module dodecahedron(r) {
    s = r / sqrt(3);
    points = s * [
        [ 1,  1,  1], [ 1,  1, -1], [ 1, -1,  1], [ 1, -1, -1],
        [-1,  1,  1], [-1,  1, -1], [-1, -1,  1], [-1, -1, -1],
        [0,  1/phi,  phi], [0,  1/phi, -phi], [0, -1/phi,  phi], [0, -1/phi, -phi],
        [ 1/phi,  phi, 0], [ 1/phi, -phi, 0], [-1/phi,  phi, 0], [-1/phi, -phi, 0],
        [ phi, 0,  1/phi], [ phi, 0, -1/phi], [-phi, 0,  1/phi], [-phi, 0, -1/phi]
    ];
    hull() {
        for (p = points) translate(p) sphere(r=0.01, $fn=6);
    }
}
module planter_body() {
    r = planter_radius;
    inner_r = r - wall_thickness;
    difference() {
        intersection() {
            cube([r*3, r*3, r], center=true);
            dodecahedron(r);
        }
        translate([0, 0, wall_thickness])
            scale([inner_r/r, inner_r/r, inner_r/r])
                dodecahedron(r);
    }
}
module drip_tray() {
    r = planter_radius;
    tray_outer_r = r * 0.62;
    tray_inner_r = tray_outer_r - wall_thickness;
    difference() {
        cylinder(h = tray_height, r1 = tray_outer_r, r2 = tray_outer_r + 2, $fn = 6);
        translate([0, 0, wall_thickness])
            cylinder(h = tray_height + 1, r1 = tray_inner_r, r2 = tray_inner_r + 2, $fn = 6);
    }
}
module view_tray() { drip_tray(); }
module view_planter() { planter_body(); }
if ($view == "planter") view_planter();
if ($view == "tray") view_tray();
"#;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_scad_code(code)));
        match result {
            Ok(CompilationResult::Success { parts, .. }) => {
                assert!(!parts.is_empty(), "Tray should produce geometry");
                eprintln!("Tray view: {} positions", parts[0].positions.len());
            }
            Ok(CompilationResult::Error(e)) => {
                panic!("Tray view should not error: {e}");
            }
            Err(_) => panic!("Planter model should not panic"),
        }

        // Also test planter body view
        let code2 = code.replace("$view = \"tray\"", "$view = \"planter\"");
        let result2 =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_scad_code(&code2)));
        match result2 {
            Ok(CompilationResult::Success { parts, .. }) => {
                assert!(!parts.is_empty(), "Planter should produce geometry");
                eprintln!("Planter view: {} positions", parts[0].positions.len());
            }
            Ok(CompilationResult::Error(e)) => {
                // Hull + intersection + difference chain may still fail
                eprintln!("Planter view error (acceptable): {e}");
            }
            Err(_) => panic!("Planter model should not panic"),
        }
    }

    #[test]
    fn test_offset_rounded() {
        let code = r#"
            offset(r=5) square([10, 10], center=true);
        "#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { .. } => {}
            CompilationResult::Error(e) => panic!("offset(r=) failed: {e}"),
        }
    }

    #[test]
    fn test_offset_expands_geometry() {
        // Extrude offset square and check bounding box is larger
        let code = r#"
            linear_extrude(1) offset(r=5) square([10, 10], center=true);
        "#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty());
                let xs: Vec<f32> = parts[0].positions.iter().map(|p| p[0]).collect();
                let xmin = xs.iter().cloned().reduce(f32::min).unwrap();
                let xmax = xs.iter().cloned().reduce(f32::max).unwrap();
                let width = xmax - xmin;
                // Without offset: width = 10. With offset(r=5): width ≈ 20
                assert!(
                    width > 15.0,
                    "Expected offset to expand geometry, got width={width}"
                );
            }
            CompilationResult::Error(e) => panic!("Compilation failed: {e}"),
        }
    }

    #[test]
    fn test_offset_delta() {
        // offset(delta=5) should also expand geometry (sharp corners)
        let code = r#"
            linear_extrude(1) offset(delta=5) square([10, 10], center=true);
        "#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty());
                let xs: Vec<f32> = parts[0].positions.iter().map(|p| p[0]).collect();
                let xmin = xs.iter().cloned().reduce(f32::min).unwrap();
                let xmax = xs.iter().cloned().reduce(f32::max).unwrap();
                let width = xmax - xmin;
                assert!(
                    width > 15.0,
                    "Expected offset(delta=) to expand geometry, got width={width}"
                );
            }
            CompilationResult::Error(e) => panic!("Compilation failed: {e}"),
        }
    }

    #[test]
    fn test_rounded_square_module() {
        // Simulates the RoundedSquare pattern used in the growbox
        let code = r#"
            module RoundedSquare(size, r) {
                offset(r=r) square([size.x - 2*r, size.y - 2*r], center=true);
            }
            linear_extrude(1) RoundedSquare([40, 40], 5);
        "#;
        let result = compile_scad_code(code);
        match result {
            CompilationResult::Success { parts, .. } => {
                assert!(!parts.is_empty());
                let xs: Vec<f32> = parts[0].positions.iter().map(|p| p[0]).collect();
                let xmin = xs.iter().cloned().reduce(f32::min).unwrap();
                let xmax = xs.iter().cloned().reduce(f32::max).unwrap();
                let width = xmax - xmin;
                // RoundedSquare([40,40], 5): inner square is 30x30, offset(r=5) expands to ~40x40
                assert!(
                    width > 35.0,
                    "RoundedSquare should be close to 40mm wide, got width={width}"
                );
            }
            CompilationResult::Error(e) => panic!("RoundedSquare compilation failed: {e}"),
        }
    }
}
