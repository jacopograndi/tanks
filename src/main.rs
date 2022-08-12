use std::fs::File;
use std::io::BufReader;

use bevy::sprite::MaterialMesh2dBundle;
use bevy::{prelude::*, render::camera::ScalingMode, window::WindowResized};

use bevy_ggrs::{GGRSPlugin, Rollback, RollbackIdProvider, SessionType};
use ggrs::{
    Config, InputStatus, P2PSession, PlayerHandle, PlayerType, SessionBuilder, SpectatorSession,
    SyncTestSession, UdpNonBlockingSocket,
};

use bytemuck::{Pod, Zeroable};
use std::net::SocketAddr;

use structopt::StructOpt;

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = BoxInput;
    type State = u8;
    type Address = SocketAddr;
}

const FPS: usize = 60;
const ROLLBACK_CORE: &str = "rollback_core";
const ROLLBACK_MOVE_PLAYERS: &str = "rollback_move_players";
const ROLLBACK_MOVE_BULLETS: &str = "rollback_move_bullets";
const ROLLBACK_FUSE: &str = "rollback_fuse";

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
        .register_rollback_type::<Transform>()
        .register_rollback_type::<Rigidbody>()
        .register_rollback_type::<Fuse>()
        .register_rollback_type::<Player>()
        .register_rollback_type::<Bullet>()
        .with_rollback_schedule(
            Schedule::default()
                .with_stage(
                    ROLLBACK_CORE,
                    SystemStage::parallel()
                        .with_system(movement)
                        .with_system(shoot),
                )
                .with_stage_after(
                    ROLLBACK_CORE,
                    ROLLBACK_MOVE_PLAYERS,
                    SystemStage::single(move_players),
                )
                .with_stage_after(
                    ROLLBACK_MOVE_PLAYERS,
                    ROLLBACK_MOVE_BULLETS,
                    SystemStage::single(move_bullets),
                )
                .with_stage_after(
                    ROLLBACK_MOVE_BULLETS,
                    ROLLBACK_FUSE,
                    SystemStage::single(clean_fuses),
                ),
        )
        .build(&mut app);

    app.insert_resource(WindowDescriptor {
        title: "Tanks!".to_string(),
        resizable: true,
        ..Default::default()
    })
    .add_plugins(DefaultPlugins)
    .add_startup_system(setup)
    .add_startup_system(spawn_camera)
    // add your GGRS session
    .insert_resource(sess)
    .insert_resource(SessionType::P2PSession)
    .add_system_to_stage(CoreStage::PostUpdate, camera_follow)
    .add_system(window_resized_event)
    .run();

    Ok(())
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
        if let Some((_, transform)) = player_query.iter().find(|(p, _)| p.handle == handles[0]) {
            let mut camera_transform = camera_query.single_mut();
            camera_transform.translation.x = transform.translation.x;
            camera_transform.translation.y = transform.translation.y;
        };
    }
}

fn spawn_camera(mut commands: Commands) {
    let mut camera = Camera2dBundle::default();
    camera.projection.scaling_mode = ScalingMode::WindowSize;
    camera.transform = Transform::from_xyz(0.0, 0.0, 100.0);
    commands.spawn_bundle(camera);
}

#[derive(Component, Default, Reflect)]
pub struct Bullet;

#[derive(Component)]
pub struct Wall;

#[derive(Component, Default, Reflect)]
pub struct Fuse {
    lit: bool,
    timeleft: f32,
}

#[derive(Component, Default, Reflect)]
pub struct Player {
    pub handle: usize,
    pub speed: f32,
    pub radius: f32,
}

#[derive(Component, Default, Reflect)]
pub struct Rigidbody {
    pub vel: Vec2,
    pub friction: f32,
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
    mut player_query: Query<(&mut Player, &mut Rigidbody)>,
    inputs: Res<Vec<(BoxInput, InputStatus)>>,
) {
    for (player, mut rb) in player_query.iter_mut() {
        let input = inputs[player.handle as usize].0.inp;
        let mut acc = Vec2::new(0.0, 0.0);
        if input & INPUT_UP != 0 && input & INPUT_DOWN == 0 {
            acc.y += 1.0;
        }
        if input & INPUT_UP == 0 && input & INPUT_DOWN != 0 {
            acc.y -= 1.0;
        }
        if input & INPUT_LEFT != 0 && input & INPUT_RIGHT == 0 {
            acc.x -= 1.0;
        }
        if input & INPUT_LEFT == 0 && input & INPUT_RIGHT != 0 {
            acc.x += 1.0;
        }
        if acc.length_squared() > 0.0 {
            acc /= acc.length();
        }
        rb.vel += acc * player.speed;
    }
}

fn shoot(
    player_query: Query<(&Player, &Transform, &Rigidbody)>,
    inputs: Res<Vec<(BoxInput, InputStatus)>>,
    mut commands: Commands,
    mut rip: ResMut<RollbackIdProvider>,
) {
    for (player, player_transform, _rb_vels) in player_query.iter() {
        let input = inputs[player.handle as usize].0;
        let sx: f32 = ((input.sx as f32) - 127.0) / 256.0;
        let sy: f32 = ((input.sy as f32) - 127.0) / 256.0;
        let mut acc = Vec2::new(sx, sy);
        if acc.length_squared() > 0.0 {
            // TODO: don't shoot when inside wall
            acc /= acc.length();
            let head = Vec3::new(acc.x, acc.y, 0.0) * (2.0 + player.radius);
            let angle = Vec2::angle_between(-Vec2::X, acc);
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
                .insert(Fuse {
                    lit: true,
                    timeleft: 2.0,
                })
                .insert(Rigidbody {
                    vel: acc * 10.0,
                    friction: 0.0,
                })
                .insert(Rollback::new(rip.next_id()));
        }
    }
}

// https://stackoverflow.com/questions/3838329
fn ccw(a: Vec3, b: Vec3, c: Vec3) -> bool {
    (c.y - a.y) * (b.x - a.x) > (b.y - a.y) * (c.x - a.x)
}

fn intersect_segment_segment(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> bool {
    ccw(a, c, d) != ccw(b, c, d) && ccw(a, b, c) != ccw(a, b, d)
}

// https://stackoverflow.com/questions/1073336
fn intersect_segment_circle(e: Vec3, l: Vec3, c: Vec3, r: f32) -> bool {
    if (e + l - c).length_squared() < r * r {
        return true;
    }
    if (e - c).length_squared() < r * r {
        return true;
    }

    let d = l;
    let f = e - c;
    let a = d.length_squared();
    let b = 2.0 * f.dot(d);
    let z = f.dot(f) - r * r;
    let delta = b * b - 4.0 * a * z;
    if delta > 0.0 {
        let deltaroot = delta.sqrt();
        let t1 = (-b - deltaroot) / (2.0 * a);
        if t1 >= 0.0 && t1 <= 1.0 {
            return true;
        }
        let t2 = (-b + deltaroot) / (2.0 * a);
        if t2 >= 0.0 && t2 <= 1.0 {
            return true;
        }
    }
    false
}

fn collision_player_circle(pos: Vec3, vel: Vec3, center: Vec3, rad: f32) -> (Vec3, Vec3) {
    if intersect_segment_circle(pos, vel, center, rad) {
        let out = pos + vel - center;
        let norm = out.normalize();
        let perp = Vec3::new(-out.y, out.x, 0.0).dot(out) * 2.0;
        return (norm * rad * 1.0005 + center, out * perp);
    }
    (pos, vel)
}

fn collision_player_wall(
    pos: Vec3,
    vel: Vec3,
    rad: f32,
    tl: Vec3,
    br: Vec3,
) -> (bool, (Vec3, Vec3)) {
    let tr = Vec3::new(br.x, tl.y, 0.0);
    let bl = Vec3::new(tl.x, br.y, 0.0);
    let (l, r, b, t) = (tl.x, br.x, br.y, tl.y);

    if r < pos.x && pos.x < l && pos.y - rad < b {
        if pos.y + rad + vel.y > b {
            return (
                true,
                (
                    Vec3::new(pos.x, b - rad, 0.0),
                    Vec3::new(vel.x, vel.y * -0.1, 0.0),
                ),
            );
        }
    }
    if r < pos.x && pos.x < l && pos.y + rad > t {
        if pos.y - rad + vel.y < t {
            return (
                true,
                (
                    Vec3::new(pos.x, t + rad, 0.0),
                    Vec3::new(vel.x, vel.y * -0.1, 0.0),
                ),
            );
        }
    }
    if b < pos.y && pos.y < t && pos.x + rad > l {
        if pos.x - rad + vel.x < l {
            return (
                true,
                (
                    Vec3::new(l + rad, pos.y, 0.0),
                    Vec3::new(vel.x * -0.1, vel.y, 0.0),
                ),
            );
        }
    }
    if b < pos.y && pos.y < t && pos.x - rad < r {
        if pos.x + rad + vel.x > r {
            return (
                true,
                (
                    Vec3::new(r - rad, pos.y, 0.0),
                    Vec3::new(vel.x * -0.1, vel.y, 0.0),
                ),
            );
        }
    }

    if pos.x < r && pos.y < b {
        if rad * rad > (br - pos - vel).length_squared() {
            return (true, collision_player_circle(pos, vel, br, rad));
        }
    }
    if pos.x > l && pos.y < b {
        if rad * rad > (bl - pos - vel).length_squared() {
            return (true, collision_player_circle(pos, vel, bl, rad));
        }
    }
    if pos.x < r && pos.y > t {
        if rad * rad > (tr - pos - vel).length_squared() {
            return (true, collision_player_circle(pos, vel, tr, rad));
        }
    }
    if pos.x > l && pos.y > t {
        if rad * rad > (tl - pos - vel).length_squared() {
            return (true, collision_player_circle(pos, vel, tl, rad));
        }
    }
    (false, (pos, vel))
}

fn move_players(
    mut player_query: Query<
        (&mut Transform, &Player, &mut Rigidbody),
        (With<Player>, Without<Wall>),
    >,
    wall_query: Query<&Transform, (With<Wall>, Without<Player>)>,
) {
    for (mut player_tr, player, mut rb) in player_query.iter_mut() {
        for wall_tr in &wall_query {
            let center = wall_tr.translation;
            let halfsize = wall_tr.scale * 0.5;
            let bottomright = center + Vec3::new(-halfsize.x, -halfsize.y, 0.0);
            let topleft = center + Vec3::new(halfsize.x, halfsize.y, 0.0);
            let (_has_collided, (pos, vel)) = collision_player_wall(
                player_tr.translation,
                Vec3::new(rb.vel.x, rb.vel.y, 0.0),
                player.radius,
                topleft,
                bottomright,
            );
            player_tr.translation = pos;
            rb.vel = Vec2::new(vel.x, vel.y);
        }
        player_tr.translation.x += rb.vel.x;
        player_tr.translation.y += rb.vel.y;
        let friction = rb.friction;
        rb.vel *= 1.0 - friction;
    }
}

fn move_bullets(
    mut bullet_query: Query<
        (&mut Transform, &mut Rigidbody, &mut Fuse),
        (With<Bullet>, Without<Player>, Without<Wall>),
    >,
    player_query: Query<(&Transform, &Player), (With<Player>, Without<Bullet>, Without<Wall>)>,
    wall_query: Query<&Transform, (With<Wall>, Without<Bullet>, Without<Player>)>,
) {
    for (mut bullet_tr, mut rb, mut fuse) in &mut bullet_query {
        for (player_tr, player) in &player_query {
            if intersect_segment_circle(
                bullet_tr.translation,
                Vec3::new(rb.vel.x, rb.vel.y, 0.0),
                player_tr.translation,
                player.radius,
            ) {
                fuse.timeleft = 0.0;
                fuse.lit = true;
            }
        }
        for wall_tr in &wall_query {
            let hi = bullet_tr.translation;
            let lo = bullet_tr.translation + Vec3::new(rb.vel.x, rb.vel.y, 0.0);
            let center = wall_tr.translation;
            let halfsize = wall_tr.scale * 0.5;
            let bottomleft = center + Vec3::new(halfsize.x, -halfsize.y, 0.0);
            let bottomright = center + Vec3::new(-halfsize.x, -halfsize.y, 0.0);
            let topright = center + Vec3::new(-halfsize.x, halfsize.y, 0.0);
            let topleft = center + Vec3::new(halfsize.x, halfsize.y, 0.0);
            if intersect_segment_segment(hi, lo, bottomleft, bottomright)
                || intersect_segment_segment(hi, lo, bottomright, topright)
                || intersect_segment_segment(hi, lo, topright, topleft)
                || intersect_segment_segment(hi, lo, topleft, bottomleft)
            {
                fuse.timeleft = 0.0;
                fuse.lit = true;
            }
        }
        bullet_tr.translation.x += rb.vel.x;
        bullet_tr.translation.y += rb.vel.y;
        let friction = rb.friction;
        rb.vel *= 1.0 - friction;
    }
}

fn clean_fuses(mut commands: Commands, mut fuse_query: Query<(Entity, &mut Fuse)>) {
    for (entity, mut fuse) in &mut fuse_query {
        if fuse.lit {
            fuse.timeleft -= 1.0 / (FPS as f32);
            if fuse.timeleft <= 0.0 {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct Map {
    name: String,
    walls: Vec<Vec<i32>>,
    hives: Vec<i32>,
    lives: Vec<Vec<i32>>,
}

fn setup_map(mut commands: Commands) {
    let file = File::open("assets/maps/NAME.txt").expect("No map file found");
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
        let size = Vec3::new((wall[2] - wall[0]) as f32, (wall[3] - wall[1]) as f32, 1.0);
        let color = match wall[4] {
            1 => Color::rgba(0.7, 0.2, 0.0, 1.0),
            2 => Color::rgba(0.15, 0.4, 0.03, 1.0),
            3 => Color::rgba(0.4, 0.4, 0.4, 1.0),
            _ => Color::rgba(1.0, 0.4, 0.03, 1.0),
        };
        let movecenter = center - Vec3::new(0.0, 0.0, if wall[4] == 2 { 1.0 } else { 0.0 });

        commands.spawn_bundle(SpriteBundle {
            transform: Transform {
                translation: movecenter,
                scale: Vec3::new(
                    (wall[2] - wall[0] + 3) as f32,
                    (wall[3] - wall[1] + 3) as f32,
                    1.0,
                ),
                ..default()
            },
            sprite: Sprite {
                color: Color::BLACK,
                ..default()
            },
            ..default()
        });

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
            .insert(Wall)
            .id();
        /*
        if wall[4] == 1 {
            commands
                .entity(entity)
                .insert(CollisionGroups::new(0b100, 0b111));
        } else {
            commands
                .entity(entity)
                .insert(CollisionGroups::new(0b100, 0b101));
        }
        */
    }
}

fn setup(
    mut commands: Commands,
    mut rip: ResMut<RollbackIdProvider>,
    p2p_session: Option<Res<P2PSession<GGRSConfig>>>,
    synctest_session: Option<Res<SyncTestSession<GGRSConfig>>>,
    spectator_session: Option<Res<SpectatorSession<GGRSConfig>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let num_players = p2p_session
        .map(|s| s.num_players())
        .or_else(|| synctest_session.map(|s| s.num_players()))
        .or_else(|| spectator_session.map(|s| s.num_players()))
        .expect("No GGRS session found");

    for handle in 0..num_players {
        commands
            .spawn_bundle(MaterialMesh2dBundle {
                mesh: meshes.add(Mesh::from(shape::Circle::new(10.0))).into(),
                transform: Transform {
                    translation: Vec3::new((handle as f32) * 20.0, 0.0, 0.0),
                    scale: Vec3::splat(1.0),
                    ..default()
                },
                material: materials.add(ColorMaterial::from(Color::WHITE)).into(),
                ..default()
            })
            .insert(Player {
                handle,
                speed: 1.0,
                radius: 10.0,
            })
            .insert(Rigidbody {
                vel: Vec2::new(0.0, 0.0),
                friction: 0.2,
            })
            .insert(Rollback::new(rip.next_id()));
    }

    setup_map(commands);
}
