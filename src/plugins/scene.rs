use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;

pub struct ScenePlugin;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct CadModel;

/// Tag for grid + axes entities that can be toggled.
#[derive(Component)]
pub struct ViewportGizmo;

/// Tag for the directional light that follows the camera orientation.
#[derive(Component)]
pub struct CameraFollowLight;

/// Visibility state for viewport gizmos (axes + grid).
#[derive(Resource)]
pub struct GizmoVisibility {
    pub visible: bool,
}

impl Default for GizmoVisibility {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// Visibility state for part labels (@1, @2, ...).
#[derive(Resource)]
pub struct LabelVisibility {
    pub visible: bool,
}

impl Default for LabelVisibility {
    fn default() -> Self {
        Self { visible: true }
    }
}

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GizmoVisibility>()
            .init_resource::<LabelVisibility>()
            .add_systems(Startup, setup_scene)
            .add_systems(Update, update_camera_follow_light);
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(30.0, 30.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
        MainCamera,
    ));

    // CAD-style lighting: strong ambient + soft camera-relative fill light (no harsh shadows).
    // This reveals surface curvature (holes, tubes, fillets) without obscuring detail.
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 800.0,
    });

    // Key light — soft, shadow-free, will be updated to follow camera each frame
    commands.spawn((
        DirectionalLight {
            illuminance: 4_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.7, 0.5, 0.0)),
        CameraFollowLight,
    ));

    // --- XYZ Axis Lines ---
    let axis_length = 50.0;

    // X axis (red)
    spawn_axis_line(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::ZERO,
        Vec3::X * axis_length,
        Color::srgb(0.9, 0.2, 0.2),
    );
    // Y axis (blue) — Bevy Y-up = OpenSCAD Z-up
    spawn_axis_line(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::ZERO,
        Vec3::Y * axis_length,
        Color::srgb(0.2, 0.4, 0.9),
    );
    // Z axis (green) — Bevy Z = OpenSCAD Y
    spawn_axis_line(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::ZERO,
        Vec3::Z * axis_length,
        Color::srgb(0.2, 0.8, 0.2),
    );

    // --- Grid Lines on XZ plane (ground) ---
    spawn_grid(&mut commands, &mut meshes, &mut materials);
}

fn spawn_axis_line(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    from: Vec3,
    to: Vec3,
    color: Color,
) {
    let mut mesh = Mesh::new(PrimitiveTopology::LineList, default());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[from.x, from.y, from.z], [to.x, to.y, to.z]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 2]);

    let material = materials.add(StandardMaterial {
        base_color: color,
        unlit: true,
        ..default()
    });

    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        ViewportGizmo,
        PickingBehavior::IGNORE,
    ));
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn spawn_grid(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let grid_size = 50.0_f32;
    let grid_step = 10.0_f32;
    let half = grid_size;
    let steps = (grid_size / grid_step) as i32;

    let mut positions: Vec<[f32; 3]> = Vec::new();

    // Lines parallel to X axis (varying Z)
    for i in -steps..=steps {
        let z = i as f32 * grid_step;
        positions.push([-half, 0.0, z]);
        positions.push([half, 0.0, z]);
    }
    // Lines parallel to Z axis (varying X)
    for i in -steps..=steps {
        let x = i as f32 * grid_step;
        positions.push([x, 0.0, -half]);
        positions.push([x, 0.0, half]);
    }

    let vert_count = positions.len();
    let mut mesh = Mesh::new(PrimitiveTopology::LineList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; vert_count]);

    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.35, 0.35, 0.35, 0.4),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        ViewportGizmo,
        PickingBehavior::IGNORE,
    ));
}

/// Keeps the fill light roughly aligned with the camera so geometry is always well-lit.
fn update_camera_follow_light(
    camera_q: Query<&Transform, With<MainCamera>>,
    mut light_q: Query<&mut Transform, (With<CameraFollowLight>, Without<MainCamera>)>,
) {
    let Ok(cam_tf) = camera_q.get_single() else {
        return;
    };
    for mut light_tf in &mut light_q {
        // Point the light in the same direction the camera is looking,
        // offset slightly upward so top surfaces get a bit more light.
        let forward = cam_tf.forward().as_vec3();
        let up = Vec3::Y;
        let dir = (forward + up * 0.3).normalize();
        light_tf.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, dir);
    }
}
