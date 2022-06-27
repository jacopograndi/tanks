mod debug;
use debug::DebugPlugin;

use serde::Deserialize;

use std::fs::File;
use std::io::BufReader;

use bevy_inspector_egui::Inspectable;

use bevy::{
    prelude::*, 
    sprite::{MaterialMesh2dBundle, collide_aabb::collide}, 
    render::camera::ScalingMode,
    ecs::system::EntityCommands,
    math::Vec3Swizzles,
    core::FixedTimestep,
};

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};


fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_startup_system(spawn_camera)
        .add_startup_system(setup)
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_stage_after(CoreStage::Update, "physics", SystemStage::parallel()
            .with_run_criteria(FixedTimestep::steps_per_second(60.0))
            .with_system(movement.label("movement"))
            .with_system(camera_follow.after("movement"))
        )
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
pub struct Player {
    speed: Vec3,
}

#[derive(Component, Clone)]
pub struct Collider {
    speed_mul: f32,
    height: bool,
}

fn wall_collision_check(
    target_player_pos: Vec3,
    wall_query: &Query<(&Transform, &Collider), 
        (With<Collider>, Without<Player>)>,
) -> Option<Collider> {
    for (wall_transform, collider) in wall_query.iter() {
        let collision = collide(
            target_player_pos,
            Vec2::splat(0.03),
            wall_transform.translation,
            wall_transform.scale.xy()
        );
        if collision.is_some() {
            return Some(collider.clone());
        }
    }
    None
}

fn movement (
    mut player_query: Query<(&mut Player, &mut Transform)>,
    wall_query: Query<(&Transform, &Collider), 
        (With<Collider>, Without<Player>)>,
    keyboard: Res<Input<KeyCode>>,
    time: Res<Time>,
) {
    let (mut player, mut transform) = player_query.single_mut();

    let mut acc = Vec3::new(0.0, 0.0, 0.0);
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
    if acc.length_squared() > 1.0 { acc = acc.normalize(); }
    acc *= time.delta_seconds();

    player.speed += acc * 0.1;
    player.speed *= 0.8;

    let target = transform.translation + player.speed;
    if let Some(collider) = wall_collision_check(target, &wall_query) {
        if !collider.height {
            transform.translation += player.speed
                * collider.speed_mul;
            player.speed *= 0.8;
        } else {
            player.speed *= 0.0;
        }
    } else {
        transform.translation = target;
    }
    /*
    let target = transform.translation + Vec3::new(acc.x, 0.0, 0.0);
    if let Some(collider) = wall_collision_check(target, &wall_query) {
        if !collider.height {
            transform.translation += Vec3::new(acc.x, 0.0, 0.0) 
                * collider.speed_mul;
        }
    } else {
        transform.translation = target;
    }
    
    let target = transform.translation + Vec3::new(0.0, acc.y, 0.0);
    if let Some(collider) = wall_collision_check(target, &wall_query) {
        if !collider.height {
            transform.translation += Vec3::new(0.0, acc.y, 0.0) 
                * collider.speed_mul;
        }
    } else {
        transform.translation = target;
    }*/
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

    let minx = map.walls.iter().map(|w| w[0]).min().unwrap() as f32;
    let maxx = map.walls.iter().map(|w| w[2]).max().unwrap() as f32;
    let miny = map.walls.iter().map(|w| w[1]).min().unwrap() as f32;
    let maxy = map.walls.iter().map(|w| w[3]).max().unwrap() as f32;
    let origin = Vec3::new(
        (maxx - minx), 
        (maxy - miny),
        0.0
    );

    for wall in &map.walls {
        let upleft = Vec3::new(wall[0] as f32, wall[1] as f32, 0.0);
        let downright = Vec3::new(wall[2] as f32, wall[3] as f32, 0.0);
        let center = (upleft + downright - origin) / 2.0 * 0.002;
        let size_big = Vec3::new(
            (wall[2] - wall[0] + 3) as f32, 
            (wall[3] - wall[1] + 3) as f32, 1.0) * 0.002;
        let movecenter = center - Vec3::new(0.0, 0.0, 
            if wall[4] == 2 { 1.0 } else { 0.0 });
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
        let center = (upleft + downright - origin) / 2.0 * 0.002;
        let size = Vec3::new(
            (wall[2] - wall[0]) as f32, 
            (wall[3] - wall[1]) as f32, 1.0) * 0.002;
        let color = match wall[4] {
            1 => Color::rgba(0.7, 0.2, 0.0, 1.0),
            2 => Color::rgba(0.15, 0.4, 0.03, 1.0),
            3 => Color::rgba(0.4, 0.4, 0.4, 1.0),
            _ => Color::rgba(1.0, 0.4, 0.03, 1.0),
        };
        let movecenter = center - Vec3::new(0.0, 0.0, 
            if wall[4] == 2 { 1.0 } else { 0.0 });
        commands.spawn_bundle(MaterialMesh2dBundle {
            mesh: meshes.add(Mesh::from(shape::Quad::default())).into(),
            transform: Transform {
                translation: movecenter,
                scale: size,
                ..default()
            },
            material: materials.add(ColorMaterial::from(color)),
            ..default()
        })
        .insert(Collider { 
            speed_mul: if wall[4] == 2 { 0.5 } else { 1.0 },
            height: if wall[4] == 2 { false } else { true }
        });
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
        .insert(Player { speed: Vec3::new(0.0, 0.0, 0.0) });

    setup_map(commands, meshes, materials);
}
