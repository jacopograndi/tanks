use serde::Deserialize;

use std::fs::File;
use std::io::BufReader;

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::{prelude::*, render::camera::ScalingMode, window::WindowResized};

use bevy_rapier2d::{pipeline::CollisionEvent::*, prelude::*};

use bevy_ggrs::{GGRSPlugin, Rollback, RollbackIdProvider, SessionType};
use ggrs::{
    Config, InputStatus, P2PSession, PlayerHandle, PlayerType, SessionBuilder, SpectatorSession,
    SyncTestSession, UdpNonBlockingSocket,
};

use bytemuck::{Pod, Zeroable};
use std::net::SocketAddr;

use bincode::{deserialize, serialize};
use structopt::StructOpt;

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = BoxInput;
    type State = u8;
    type Address = SocketAddr;
}

const FPS: usize = 10;
const ROLLBACK_DEFAULT: &str = "rollback_default";

// structopt will read command line parameters for u
#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    local_port: u16,
    #[structopt(short, long)]
    players: Vec<String>,
    #[structopt(short, long)]
    spectators: Vec<SocketAddr>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::from_args();
    let num_players = opt.players.len();
    assert!(num_players > 0);

    // create a GGRS session
    let mut sess_build = SessionBuilder::<GGRSConfig>::new()
        .with_num_players(num_players)
        .with_max_prediction_window(12) // (optional) set max prediction window
        .with_input_delay(2); // (optional) set input delay for the local player

    // add players
    for (i, player_addr) in opt.players.iter().enumerate() {
        // local player
        if player_addr == "localhost" {
            sess_build = sess_build.add_player(PlayerType::Local, i)?;
        } else {
            // remote players
            let remote_addr: SocketAddr = player_addr.parse()?;
            sess_build = sess_build.add_player(PlayerType::Remote(remote_addr), i)?;
        }
    }

    // optionally, add spectators
    for (i, spec_addr) in opt.spectators.iter().enumerate() {
        sess_build = sess_build.add_player(PlayerType::Spectator(*spec_addr), num_players + i)?;
    }

    // start the GGRS session
    let socket = UdpNonBlockingSocket::bind_to_port(opt.local_port)?;
    let sess = sess_build.start_p2p_session(socket)?;

    let mut app = App::new();
    GGRSPlugin::<GGRSConfig>::new()
        .with_update_frequency(FPS)
        .with_input_system(input)
        .register_rollback_type::<SerPhysics>()
        .register_rollback_type::<Player>()
        .register_rollback_type::<Bullet>()
        .with_rollback_schedule(
            Schedule::default().with_stage(
                ROLLBACK_DEFAULT,
                SystemStage::single_threaded()
                    .with_system(physics_deser)
                    .with_system(movement)
                    .with_system(shoot)
                    .with_system(hits)
                    .with_system_set(RapierPhysicsPlugin::<()>::get_systems(
                        PhysicsStages::SyncBackend,
                    ))
                    .with_system_set(RapierPhysicsPlugin::<()>::get_systems(
                        PhysicsStages::StepSimulation,
                    ))
                    .with_system_set(RapierPhysicsPlugin::<()>::get_systems(
                        PhysicsStages::Writeback,
                    ))
                    .with_system_set(RapierPhysicsPlugin::<()>::get_systems(
                        PhysicsStages::DetectDespawn,
                    ))
                    .with_system(physics_ser),
            ),
        )
        .build(&mut app);

    app.insert_resource(WindowDescriptor {
        title: "Tanks!".to_string(),
        resizable: true,
        ..Default::default()
    })
    // add your GGRS session
    .insert_resource(sess)
    .insert_resource(SessionType::P2PSession)
    .add_plugins(DefaultPlugins)
    .add_startup_system(physics_init)
    .add_startup_system(setup)
    .add_startup_system(spawn_camera)
    //.add_plugin(LogDiagnosticsPlugin::default())
    //.add_plugin(FrameTimeDiagnosticsPlugin::default())
    .add_plugin(
        RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0).with_default_system_setup(false),
    )
    //.add_plugin(RapierDebugRenderPlugin::default())
    .add_system_to_stage(CoreStage::PostUpdate, camera_follow)
    .add_system(window_resized_event)
    .run();

    Ok(())
}

#[derive(Default, Reflect, Component)]
struct SerPhysics {
    pub ser: Vec<u8>,
}

fn physics_init(
    mut commands: Commands,
    context: Res<RapierContext>,
    mut rip: ResMut<RollbackIdProvider>,
) {
    commands
        .spawn()
        .insert(SerPhysics {
            ser: bincode::serialize(context.into_inner()).unwrap(),
        })
        .insert(Rollback::new(rip.next_id()));
}

fn physics_ser(mut ser_query: Query<&mut SerPhysics>, context: Res<RapierContext>) {
    ser_query.single_mut().ser = bincode::serialize(context.into_inner()).unwrap();
}

fn physics_deser(ser_query: Query<&SerPhysics>, mut commands: Commands) {
    commands.remove_resource::<RapierContext>();
    commands
        .insert_resource(bincode::deserialize::<RapierContext>(&ser_query.single().ser).unwrap());
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
    p2p_session: Option<Res<P2PSession<GGRSConfig>>>,
) {
    let handles = p2p_session.unwrap().local_player_handles();
    if handles.len() > 0 {
        let (_, transform) = player_query
            .iter()
            .find(|(p, _)| p.handle == handles[0])
            .unwrap();
        let mut camera_transform = camera_query.single_mut();
        camera_transform.translation.x = transform.translation.x;
        camera_transform.translation.y = transform.translation.y;
    }
}

fn spawn_camera(mut commands: Commands) {
    let mut camera = Camera2dBundle::default();
    camera.projection.scaling_mode = ScalingMode::WindowSize;
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

#[derive(Component, Default, Reflect)]
pub struct Bullet;

#[derive(Component, Default, Reflect)]
pub struct Player {
    pub handle: usize,
    pub speed: f32,
    pub radius: f32,
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Pod, Zeroable)]
pub struct BoxInput {
    pub inp: u8,
    pub sx: u8,
    pub sy: u8,
}

const INPUT_UP: u8 = 1 << 0;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_LEFT: u8 = 1 << 2;
const INPUT_RIGHT: u8 = 1 << 3;

pub fn input(_handle: In<PlayerHandle>, keyboard_input: Res<Input<KeyCode>>) -> BoxInput {
    let mut input: u8 = 0;

    if keyboard_input.pressed(KeyCode::W) {
        input |= INPUT_UP;
    }
    if keyboard_input.pressed(KeyCode::A) {
        input |= INPUT_LEFT;
    }
    if keyboard_input.pressed(KeyCode::S) {
        input |= INPUT_DOWN;
    }
    if keyboard_input.pressed(KeyCode::D) {
        input |= INPUT_RIGHT;
    }

    let mut x: u8 = 127;
    let mut y: u8 = 127;
    if keyboard_input.pressed(KeyCode::Up) {
        y = 255;
    }
    if keyboard_input.pressed(KeyCode::Down) {
        y = 0;
    }
    if keyboard_input.pressed(KeyCode::Left) {
        x = 0;
    }
    if keyboard_input.pressed(KeyCode::Right) {
        x = 255;
    }

    BoxInput {
        inp: input,
        sx: x,
        sy: y,
    }
}

fn movement(
    mut player_query: Query<(&mut Player, &mut Velocity)>,
    inputs: Res<Vec<(BoxInput, InputStatus)>>,
) {
    for (player, mut rb_vels) in player_query.iter_mut() {
        let input = inputs[player.handle as usize].0.inp;
        let mut acc = Vec2::new(0.0, 0.0);
        if input & INPUT_UP != 0 && input & INPUT_DOWN == 0 {
            acc.y -= 1.0;
        }
        if input & INPUT_UP == 0 && input & INPUT_DOWN != 0 {
            acc.x -= 1.0;
        }
        if input & INPUT_LEFT != 0 && input & INPUT_RIGHT == 0 {
            acc.x += 1.0;
        }
        if input & INPUT_LEFT == 0 && input & INPUT_RIGHT != 0 {
            acc.x -= 1.0;
        }
        if acc.length_squared() > 0.0 {
            acc /= acc.length();
        }
        rb_vels.linvel += acc * player.speed;
    }
}

fn shoot(
    player_query: Query<(&Player, &Transform, &Velocity)>,
    inputs: Res<Vec<(BoxInput, InputStatus)>>,
    mut commands: Commands,
    mut rip: ResMut<RollbackIdProvider>,
) {
    for (player, player_transform, _rb_vels) in player_query.iter() {
        let input = inputs[player.handle as usize].0;
        let sx: f32 = (input.sx as f32) - 127.0 / 256.0;
        let sy: f32 = (input.sy as f32) - 127.0 / 256.0;
        let mut acc = Vec2::new(sx, sy);
        if acc.length_squared() > 0.0 {
            acc /= acc.length();
            let head = Vec3::new(acc.x, acc.y, 0.0) * (2.0 + player.radius + 3.0);
            let angle = Vec2::angle_between(Vec2::new(1.0, 0.0), acc);
            commands
                .spawn()
                .insert_bundle(SpriteBundle {
                    transform: Transform {
                        translation: player_transform.translation + head,
                        rotation: Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, angle),
                        scale: Vec3::new(5.0, 2.0, 1.0),
                    },
                    sprite: Sprite {
                        color: Color::WHITE,
                        ..default()
                    },
                    ..default()
                })
                .insert(Bullet)
                .insert(RigidBody::Dynamic)
                .insert(Restitution::coefficient(0.0))
                .insert(Collider::cuboid(0.5, 0.5))
                .insert(LockedAxes::ROTATION_LOCKED)
                .insert(Damping {
                    linear_damping: 0.3,
                    angular_damping: 1.0,
                })
                .insert(Ccd::enabled())
                .insert(Velocity::linear(acc * 1000.0))
                .insert(CollisionGroups::new(0b010, 0b101))
                .insert(ActiveEvents::COLLISION_EVENTS)
                .insert(Rollback::new(rip.next_id()));
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
        let entity = commands
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
            .id();
        if wall[4] == 1 {
            commands
                .entity(entity)
                .insert(CollisionGroups::new(0b100, 0b111));
        } else {
            commands
                .entity(entity)
                .insert(CollisionGroups::new(0b100, 0b101));
        }
    }
}

fn setup(
    mut commands: Commands,
    mut rapier_config: ResMut<RapierConfiguration>,
    mut rip: ResMut<RollbackIdProvider>,
    p2p_session: Option<Res<P2PSession<GGRSConfig>>>,
    synctest_session: Option<Res<SyncTestSession<GGRSConfig>>>,
    spectator_session: Option<Res<SpectatorSession<GGRSConfig>>>,
) {
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

    let num_players = p2p_session
        .map(|s| s.num_players())
        .or_else(|| synctest_session.map(|s| s.num_players()))
        .or_else(|| spectator_session.map(|s| s.num_players()))
        .expect("No GGRS session found");

    for handle in 0..num_players {
        commands
            .spawn()
            .insert_bundle(SpriteBundle {
                transform: Transform {
                    translation: Vec3::new(handle as f32, 0.0, 0.0),
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
                handle,
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
            .insert(CollisionGroups::new(0b001, 0b111))
            .insert(Rollback::new(rip.next_id()));
    }

    setup_map(commands);
}
