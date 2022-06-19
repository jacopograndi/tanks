mod debug;
use debug::DebugPlugin;

use serde::Deserialize;

use std::fs::File;
use std::io::BufReader;

use bevy_inspector_egui::Inspectable;

use bevy::{prelude::*, sprite::MaterialMesh2dBundle, render::camera::ScalingMode};

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_startup_system(spawn_camera)
        .add_startup_system(setup)
        .add_system(movement.label("movement"))
        .add_system(camera_follow.after("movement"))
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .run();
}

fn spawn_camera(mut commands: Commands) {
    let mut camera = OrthographicCameraBundle::new_2d();
    camera.orthographic_projection.scaling_mode = ScalingMode::None;
    camera.transform = Transform::from_xyz(0.0, 0.0, 100.0);
    commands.spawn_bundle(camera);
}

fn camera_follow(
    player_query: Query<(&Transform, With<Player>)>,
    mut camera_query: Query<&mut Transform, (Without<Player>, With<Camera>)>,
) {
    let (player_transform, _) = player_query.single();
    let mut camera_transform = camera_query.single_mut();
    camera_transform.translation.x = player_transform.translation.x;
    camera_transform.translation.y = player_transform.translation.y;
}

#[derive(Component, Inspectable)]
pub struct Player;

fn movement (
    mut player_query: Query<(&Player, &mut Transform)>,
    keyboard: Res<Input<KeyCode>>,
    time: Res<Time>,
) {
    let (_player, mut transform) = player_query.single_mut();

    if keyboard.pressed(KeyCode::W) {
        transform.translation.y += 1.0 * time.delta_seconds();
    }
    if keyboard.pressed(KeyCode::S) {
        transform.translation.y -= 1.0 * time.delta_seconds();
    }
    if keyboard.pressed(KeyCode::A) {
        transform.translation.x -= 1.0 * time.delta_seconds();
    }
    if keyboard.pressed(KeyCode::D) {
        transform.translation.x += 1.0 * time.delta_seconds();
    }
}

fn spawn_rect(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    color: Color,
    pos: Vec3,
    size: Vec3
) {
    commands.spawn_bundle(MaterialMesh2dBundle {
        mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
        transform: Transform {
            translation: pos,
            scale: size,
            ..default()
        },
        material: materials.add(ColorMaterial::from(color)),
        ..default()
    });
}

#[derive(Deserialize)]
struct Map {
   name: String,
   walls: Vec<Vec<i32>>,
   hives: Vec<i32>,
   lives: Vec<Vec<i32>>
}

fn setup_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let file = File::open("assets/maps/MAZE.txt").expect("No map file found");
    let map : Map = serde_json::from_reader(BufReader::new(file)).unwrap();

    for wall in &map.walls {
        let upleft = Vec3::new(wall[0] as f32, wall[1] as f32, 0.0);
        let downright = Vec3::new(wall[2] as f32, wall[3] as f32, 0.0);
        let center = (upleft + downright) / 2.0 * 0.002;
        let size_big = Vec3::new(
            (wall[2] - wall[0] +10) as f32, 
            (wall[3] - wall[1] +10) as f32, 1.0) * 0.002;
        spawn_rect(&mut commands, &mut meshes, &mut materials, Color::BLACK, center, size_big); 
    }
    for wall in &map.walls {
        let upleft = Vec3::new(wall[0] as f32, wall[1] as f32, 0.0);
        let downright = Vec3::new(wall[2] as f32, wall[3] as f32, 0.0);
        let center = (upleft + downright) / 2.0 * 0.002;
        let size = Vec3::new(
            (wall[2] - wall[0]) as f32, 
            (wall[3] - wall[1]) as f32, 1.0) * 0.002;
        spawn_rect(&mut commands, &mut meshes, &mut materials, Color::WHITE, center, size); 
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let color = commands.spawn_bundle(MaterialMesh2dBundle {
        mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
        transform: Transform::default().with_scale(Vec3::splat(1.2)),
        material: materials.add(ColorMaterial::from(Color::BLACK)),
        ..default()
    }).id();

    commands.spawn_bundle(MaterialMesh2dBundle {
        mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
        transform: Transform::default().with_scale(Vec3::splat(0.03)),
        material: materials.add(ColorMaterial::from(Color::WHITE)),
        ..default()
    })
        .push_children(&[color])
        .insert(Player);

    setup_map(commands, meshes, materials);
}
