mod debug;

use serde::Deserialize;

use std::fs::File;
use std::io::BufReader;

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::ecs::archetype::Archetypes;
use bevy::ecs::component::ComponentId;
use bevy::{prelude::*, render::camera::ScalingMode, window::WindowResized};

use bevy_inspector_egui::Inspectable;
use bevy_rapier2d::{pipeline::CollisionEvent::*, prelude::*};

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
        .add_system_to_stage(CoreStage::Update, movement)
        .add_system_to_stage(CoreStage::Update, shoot)
        .add_system_to_stage(CoreStage::PostUpdate, camera_follow)
        .add_system_to_stage(CoreStage::PostUpdate, hits)
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

fn hits(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    bullet_query: Query<&Bullet>,
) {
    let mut despawned = Vec::<Entity>::new();
    for collision_event in collision_events.iter() {
        if let Started(ent, oth, _) = collision_event {
            if let Ok(_) = bullet_query.get(*ent) {
                if !despawned.contains(&*ent) {
                    despawned.push(*ent);
                    commands.entity(*ent).despawn();
                }
            }
            if let Ok(_) = bullet_query.get(*oth) {
                if !despawned.contains(&*oth) {
                    despawned.push(*oth);
                    commands.entity(*oth).despawn();
                }
            }
        }
    }
}

#[derive(Component, Inspectable)]
pub struct Bullet;

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
        rb_vels.linvel += acc * player.speed;
    }
}

fn shoot(
    player_query: Query<(&Player, &Transform, &Velocity)>,
    keyboard: Res<Input<KeyCode>>,
    mut commands: Commands,
) {
    for (player, player_transform, _rb_vels) in player_query.iter() {
        let mut acc = Vec2::new(0.0, 0.0);
        if keyboard.pressed(KeyCode::Up) {
            acc.y += 1.0;
        }
        if keyboard.pressed(KeyCode::Down) {
            acc.y -= 1.0;
        }
        if keyboard.pressed(KeyCode::Left) {
            acc.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::Right) {
            acc.x += 1.0;
        }
        if acc.length_squared() > 0.0 {
            acc /= acc.length();
            let head = Vec3::new(acc.x, acc.y, 0.0) * (2.0 + player.radius + 3.0);
            commands
                .spawn()
                .insert_bundle(SpriteBundle {
                    transform: Transform {
                        translation: player_transform.translation + head,
                        scale: Vec3::splat(4.0),
                        ..default()
                    },
                    sprite: Sprite {
                        color: Color::BLACK,
                        ..default()
                    },
                    ..default()
                })
                .insert(Bullet)
                .insert(RigidBody::Dynamic)
                .insert(Restitution::coefficient(0.0))
                .insert(Collider::ball(1.0))
                .insert(LockedAxes::ROTATION_LOCKED)
                .insert(Damping {
                    linear_damping: 0.3,
                    angular_damping: 1.0,
                })
                .insert(Ccd::enabled())
                .insert(Velocity::linear(acc * 1000.0))
                .insert(CollisionGroups::new(0b010, 0b101))
                .insert(ActiveEvents::COLLISION_EVENTS);
        }
    }
}

#[derive(Deserialize)]
struct Map {
    name: String,
    walls: Vec<Vec<i32>>,
    hives: Vec<i32>,
    lives: Vec<Vec<i32>>,
}

fn setup_map(mut commands: Commands) {
    let file = File::open("assets/maps/TERM.txt").expect("No map file found");
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

        commands.spawn_bundle(SpriteBundle {
            transform: Transform {
                translation: movecenter,
                scale: size_big,
                ..default()
            },
            sprite: Sprite {
                color: Color::BLACK,
                ..default()
            },
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
            .spawn_bundle(SpriteBundle {
                transform: Transform {
                    translation: movecenter,
                    scale: size,
                    ..default()
                },
                sprite: Sprite { color, ..default() },
                ..default()
            })
            .insert(Collider::cuboid(0.5, 0.5))
            .insert(CollisionGroups::new(0b100, 0b111));
    }
}

fn setup(mut commands: Commands, mut rapier_config: ResMut<RapierConfiguration>) {
    rapier_config.gravity = Vec2::ZERO;

    let color = commands
        .spawn()
        .insert_bundle(SpriteBundle {
            transform: Transform::default().with_scale(Vec3::splat(1.2)),
            sprite: Sprite {
                color: Color::BLACK,
                ..default()
            },
            ..default()
        })
        .id();

    commands
        .spawn()
        .insert_bundle(SpriteBundle {
            transform: Transform {
                translation: Vec3::new(0.0, 0.0, 0.0),
                scale: Vec3::splat(10.0),
                ..default()
            },
            sprite: Sprite {
                color: Color::WHITE,
                ..default()
            },
            ..default()
        })
        .push_children(&[color])
        .insert(Player {
            speed: 150.0,
            radius: 10.0,
        })
        .insert(RigidBody::Dynamic)
        .insert(Restitution::coefficient(0.0))
        .insert(Collider::ball(1.0))
        .insert(LockedAxes::ROTATION_LOCKED)
        .insert(Damping {
            linear_damping: 30.0,
            angular_damping: 1.0,
        })
        .insert(Friction {
            coefficient: 0.0,
            combine_rule: CoefficientCombineRule::Min,
        })
        .insert(Ccd::enabled())
        .insert(Velocity::zero())
        .insert(CollisionGroups::new(0b001, 0b111));

    setup_map(commands);
}
