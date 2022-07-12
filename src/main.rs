mod debug;
use debug::DebugPlugin;

use serde::Deserialize;

use std::fs::File;
use std::io::BufReader;

use bevy_inspector_egui::Inspectable;

use bevy::{
    core::FixedTimestep,
    prelude::*,
    render::camera::ScalingMode,
    sprite::MaterialMesh2dBundle,
    window::{Window, WindowResized},
};

use bevy_rapier2d::prelude::*;

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};

fn main() {
    App::new()
        .insert_resource(WindowDescriptor {
            title: "Tanks!".to_string(),
            resizable: true,
            ..Default::default()
        })
        .add_plugins(DefaultPlugins)
        .add_startup_system(spawn_camera)
        .add_startup_system(setup)
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugin(RapierDebugRenderPlugin::default())
        .add_system_to_stage(PhysicsStages::Writeback, camera_follow)
        .add_system(movement)
        .add_system(window_resized_event)
        .run();
}

fn window_resized_event(
    mut events: EventReader<WindowResized>,
    mut window: ResMut<WindowDescriptor>,
) {
    for event in events.iter() {
        window.width = event.width.try_into().unwrap();
        window.height = event.height.try_into().unwrap();
    }
}

fn camera_follow(
    player_query: Query<(&Player, &Transform)>,
    mut camera_query: Query<&mut Transform, (Without<Player>, With<Camera>)>,
) {
    let (_, transform) = player_query.single();
    let mut camera_transform = camera_query.single_mut();
    camera_transform.translation.x = transform.translation.x;
    camera_transform.translation.y = transform.translation.y;
}

fn spawn_camera(mut commands: Commands) {
    let mut camera = OrthographicCameraBundle::new_2d();
    camera.orthographic_projection.scaling_mode = ScalingMode::WindowSize;
    camera.orthographic_projection.scale = 1.0;
    camera.transform = Transform::from_xyz(0.0, 0.0, 100.0);
    commands.spawn_bundle(camera);
}

#[derive(Component, Inspectable)]
pub struct Player {
    speed: f32,
    radius: f32,
}

fn movement(mut player_query: Query<(&mut Player, &mut Velocity)>, keyboard: Res<Input<KeyCode>>) {
    for (player, mut rb_vels) in player_query.iter_mut() {
        let mut acc = Vec2::new(0.0, 0.0);
        if keyboard.pressed(KeyCode::W) {
            acc.y += 1.0;
        }
        if keyboard.pressed(KeyCode::S) {
            acc.y -= 1.0;
        }
        if keyboard.pressed(KeyCode::A) {
            acc.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::D) {
            acc.x += 1.0;
        }
        if acc.length_squared() > 0.0 {
            acc /= acc.length();
        }
        rb_vels.linvel = acc * player.speed;
    }
}

#[derive(Deserialize)]
struct Map {
    name: String,
    walls: Vec<Vec<i32>>,
    hives: Vec<i32>,
    lives: Vec<Vec<i32>>,
}

fn setup_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let file = File::open("assets/maps/OFFC.txt").expect("No map file found");
    let map: Map = serde_json::from_reader(BufReader::new(file)).unwrap();

    let minx = map.walls.iter().map(|w| w[0]).min().unwrap() as f32;
    let maxx = map.walls.iter().map(|w| w[2]).max().unwrap() as f32;
    let miny = map.walls.iter().map(|w| w[1]).min().unwrap() as f32;
    let maxy = map.walls.iter().map(|w| w[3]).max().unwrap() as f32;
    let origin = Vec3::new(maxx - minx, maxy - miny, 0.0);

    for wall in &map.walls {
        let upleft = Vec3::new(wall[0] as f32, wall[1] as f32, 0.0);
        let downright = Vec3::new(wall[2] as f32, wall[3] as f32, 0.0);
        let center = (upleft + downright - origin) / 2.0;
        let size_big = Vec3::new(
            (wall[2] - wall[0] + 3) as f32,
            (wall[3] - wall[1] + 3) as f32,
            1.0,
        );
        let movecenter = center - Vec3::new(0.0, 0.0, if wall[4] == 2 { 1.0 } else { 0.0 });
        commands.spawn_bundle(MaterialMesh2dBundle {
            mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
            transform: Transform {
                translation: movecenter,
                scale: size_big,
                ..default()
            },
            material: materials.add(ColorMaterial::from(Color::BLACK)),
            ..default()
        });
    }
    for wall in &map.walls {
        let upleft = Vec3::new(wall[0] as f32, wall[1] as f32, 0.0);
        let downright = Vec3::new(wall[2] as f32, wall[3] as f32, 0.0);
        let center = (upleft + downright - origin) / 2.0;
        let size = Vec3::new((wall[2] - wall[0]) as f32, (wall[3] - wall[1]) as f32, 1.0);
        let color = match wall[4] {
            1 => Color::rgba(0.7, 0.2, 0.0, 1.0),
            2 => Color::rgba(0.15, 0.4, 0.03, 1.0),
            3 => Color::rgba(0.4, 0.4, 0.4, 1.0),
            _ => Color::rgba(1.0, 0.4, 0.03, 1.0),
        };
        let movecenter = center - Vec3::new(0.0, 0.0, if wall[4] == 2 { 1.0 } else { 0.0 });
        commands
            .spawn_bundle(MaterialMesh2dBundle {
                mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
                transform: Transform {
                    translation: movecenter,
                    scale: size,
                    ..default()
                },
                material: materials.add(ColorMaterial::from(color)),
                ..default()
            })
            .insert(Collider::cuboid(0.5, 0.5));
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut rapier_config: ResMut<RapierConfiguration>,
) {
    rapier_config.gravity = Vec2::ZERO;

    let color = commands
        .spawn_bundle(MaterialMesh2dBundle {
            mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
            transform: Transform::default().with_scale(Vec3::splat(1.2)),
            material: materials.add(ColorMaterial::from(Color::BLACK)),
            ..default()
        })
        .id();

    commands
        .spawn_bundle(MaterialMesh2dBundle {
            mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
            transform: Transform::default().with_scale(Vec3::splat(13.0)),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        })
        .push_children(&[color])
        .insert(Player {
            speed: 300.0,
            radius: 10.0,
        })
        .insert(RigidBody::Dynamic)
        .insert(Restitution::coefficient(0.0))
        .insert(Collider::ball(1.0))
        .insert(LockedAxes::ROTATION_LOCKED)
        .insert(Damping {
            linear_damping: 0.8,
            angular_damping: 1.0,
        })
        .insert(Velocity::zero());

    setup_map(commands, meshes, materials);
}
