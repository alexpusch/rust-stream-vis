mod future_vis;
mod pumps_vis;
mod pumps_vis_builder;
mod stream_vis;
mod stream_vis_builder;

use argh::FromArgs;
use bevy_tweening::TweeningPlugin;
use crossbeam_channel::Receiver;

use image::DynamicImage;
use pumps_vis_builder::PumpsVisBuilder;
use rand::{rngs::StdRng, Rng, SeedableRng};
use stream_vis::{BG_COLOR, SECTION_HEIGHT};
use stream_vis_builder::StreamVisBuilder;
use tempfile::TempDir;

use crate::pumps_vis::{create_units, spawn_blocks};

use bevy::{
    prelude::*,
    render::view::screenshot::ScreenshotManager,
    sprite::MaterialMesh2dBundle,
    window::{PrimaryWindow, WindowCloseRequested},
};
use std::{
    env,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

const SEED: [u8; 32] = [1; 32];

#[derive(Resource, Deref)]
struct StreamReceiver(Receiver<StreamUpdate>);

#[derive(Clone, Debug)]
pub enum UnitValueKind {
    PendingFuture(Color),
    RunningFuture(f32),
    Value(Color),
}

#[derive(Clone, Debug)]
pub struct UnitCreatedEvent {
    pub id: u32,
    pub block_id: u32,
    pub value: UnitValueKind,
}

#[derive(Clone, Debug)]
pub struct UnitValueUpdateEvent {
    pub id: u32,
    pub value: UnitValueKind,
}

#[derive(Clone, Debug)]
pub struct FilteredOutEvent {
    pub id: u32,
}
#[derive(Clone, Debug)]
pub struct UnitAdvanceBlockEvent {
    pub id: u32,
    pub block_id: u32,
    pub from_block_id: u32,
}

#[derive(Clone, Debug)]
pub enum StreamUpdate {
    Created(UnitCreatedEvent),
    ChangeValue(UnitValueUpdateEvent),
    AdvanceBlock(UnitAdvanceBlockEvent),
    FilteredOut(FilteredOutEvent),
}

#[derive(Clone, Event, Debug)]
pub struct StreamEvent(pub StreamUpdate);

#[derive(Clone, Debug)]
pub struct StreamedUnit {
    pub id: u32,
    pub block_id: u32,
}

#[derive(Debug, FromArgs, Resource)]
/// stream vis config
struct Config {
    /// whether or not to jump
    #[argh(positional)]
    output_filename: Option<String>,
}

#[derive(Resource)]
struct ScreenshotStorage {
    pub started_writing: bool,
    // pub frames: Arc<Mutex<Vec<(u128, Image)>>>,
    pub sender: mpsc::Sender<DynamicImage>,
    pub path: TempDir,
}

#[tokio::main]
async fn main() {
    let _ = env_logger::builder().format_timestamp_millis().try_init();
    let config: Config = argh::from_env();

    let (sender, receiver) = mpsc::channel::<DynamicImage>();

    let screenshot_dir = tempfile::tempdir().unwrap();
    let screenshot_dir2 = screenshot_dir.path().to_path_buf();

    thread::spawn(move || {
        let mut i = 0;

        while let Ok(img) = receiver.recv() {
            let path = &screenshot_dir2.join(format!("screenshot-{:0>9}.png", i));

            i += 1;

            match img.save_with_format(path, image::ImageFormat::Png) {
                Ok(_) => debug!("Screenshot saved to {}", path.display()),
                Err(e) => error!("Cannot save screenshot, IO error: {e}"),
            }
        }
    });

    let vis_stream = false;

    if vis_stream {
        App::new()
            .add_event::<StreamEvent>()
            .add_plugins(DefaultPlugins)
            .add_plugins(TweeningPlugin)
            .add_systems(Startup, setup_stream_vis)
            .add_systems(PreUpdate, read_stream)
            .add_systems(PreUpdate, stream_vis::create_units.after(read_stream))
            .add_systems(FixedUpdate, stream_vis::advance_units.after(create_units))
            .add_systems(
                FixedUpdate,
                stream_vis::update_units.after(stream_vis::advance_units),
            )
            .add_systems(
                FixedUpdate,
                stream_vis::handle_filtered_out.after(stream_vis::advance_units),
            )
            .add_systems(FixedUpdate, save_frame)
            .add_systems(Update, save_gif)
            .insert_resource(config)
            .insert_resource(ScreenshotStorage {
                started_writing: false,
                sender,
                path: screenshot_dir,
            })
            .run();
    } else {
        App::new()
            .add_event::<StreamEvent>()
            .add_plugins(DefaultPlugins)
            .add_plugins(TweeningPlugin)
            .add_systems(Startup, setup_pumps_vis)
            .add_systems(PreUpdate, read_stream)
            .add_systems(PreUpdate, pumps_vis::create_units.after(read_stream))
            .add_systems(FixedUpdate, pumps_vis::advance_units.after(create_units))
            .add_systems(
                FixedUpdate,
                pumps_vis::update_units.after(pumps_vis::advance_units),
            )
            .add_systems(
                FixedUpdate,
                pumps_vis::handle_filtered_out.after(pumps_vis::advance_units),
            )
            .add_systems(FixedUpdate, save_frame)
            .add_systems(Update, save_gif)
            .insert_resource(config)
            .insert_resource(ScreenshotStorage {
                started_writing: false,
                sender,
                path: screenshot_dir,
            })
            .run();
    }
}

fn setup_window(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    window: &mut Query<&mut Window>,
) {
    let mut window = window.single_mut();
    window.resolution.set(740., SECTION_HEIGHT + 50.);

    // background
    commands.spawn(MaterialMesh2dBundle {
        mesh: meshes
            .add(
                shape::Box::from_corners(
                    Vec3::new(-1000., -1000., 0.),
                    Vec3::new(1000., 1000., 0.),
                )
                .into(),
            )
            .into(),
        transform: Transform::from_translation(Vec3::new(0., 0., -200.)),
        material: materials.add(ColorMaterial::from(BG_COLOR)),
        ..default()
    });
}

fn setup_pumps_vis(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    asset_server: Res<AssetServer>,
    mut window: Query<&mut Window>,
) {
    setup_window(&mut commands, &mut meshes, &mut materials, &mut window);

    let n = 30;
    let mut rng: StdRng = SeedableRng::from_seed(SEED);

    let timings1 = (0..n)
        .map(|_| Duration::from_millis(rng.gen_range(1000..2000)))
        .collect::<Vec<_>>();

    let timings2 = (0..n)
        .map(|_| Duration::from_millis(rng.gen_range(1500..2500)))
        .collect::<Vec<_>>();

    let (blocks, rx) = PumpsVisBuilder::source(n)
        .map_buffered(timings1, 3, Duration::from_millis(1000))
        .map_buffered(timings2, 3, Duration::from_millis(2000))
        .sink();

    let end = spawn_blocks(
        blocks,
        &mut commands,
        &mut meshes,
        &mut materials,
        asset_server,
    );

    commands.spawn(Camera2dBundle {
        transform: Transform::from_translation(Vec3::new(end / 2., 0., 0.)),
        ..Default::default()
    });

    commands.insert_resource(StreamReceiver(rx));
}

fn setup_stream_vis(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    asset_server: Res<AssetServer>,
    mut window: Query<&mut Window>,
) {
    setup_window(&mut commands, &mut meshes, &mut materials, &mut window);

    let n = 30;
    let mut rng: StdRng = SeedableRng::from_seed(SEED);

    let timings1 = (0..n)
        .map(|_| Duration::from_millis(rng.gen_range(1000..2000)))
        .collect::<Vec<_>>();

    let timings2 = (0..n)
        .map(|_| Duration::from_millis(rng.gen_range(1000..2000)))
        .collect::<Vec<_>>();

    let (blocks, rx) = StreamVisBuilder::source(20)
        .map_buffered(timings1, 3, Duration::from_millis(1000))
        .map_buffered(timings2, 3, Duration::from_millis(1000))
        .sink();

    let end = stream_vis::spawn_blocks(
        blocks,
        &mut commands,
        &mut meshes,
        &mut materials,
        asset_server,
    );

    commands.spawn(Camera2dBundle {
        transform: Transform::from_translation(Vec3::new(end / 2., 0., 0.)),
        ..Default::default()
    });

    commands.insert_resource(StreamReceiver(rx));
}

// This system reads from the receiver and sends events to Bevy
fn read_stream(receiver: Res<StreamReceiver>, mut events: EventWriter<StreamEvent>) {
    for from_stream in receiver.try_iter() {
        events.send(StreamEvent(from_stream));
    }
}

fn save_frame(
    main_window: Query<Entity, With<PrimaryWindow>>,
    mut screenshot_manager: ResMut<ScreenshotManager>,
    screenshot_storage: Res<ScreenshotStorage>,
) {
    if screenshot_storage.started_writing {
        return;
    }

    let sender = screenshot_storage.sender.clone();
    _ = screenshot_manager.take_screenshot(main_window.single(), move |img| {
        match img.clone().try_into_dynamic() {
            Ok(dyn_img) => {
                sender.send(dyn_img).unwrap();
            }
            Err(e) => error!("Cannot save screenshot, screen format cannot be understood: {e}"),
        }
    });
}

fn save_gif(
    mut reader: EventReader<WindowCloseRequested>,
    config: Res<Config>,
    mut screenshot_storage: ResMut<ScreenshotStorage>,
) {
    for _ in reader.read().take(1) {
        debug!("close event received");
        let Some(output_filename) = &config.output_filename else {
            return;
        };

        screenshot_storage.started_writing = true;

        let current_dir = env::current_dir().unwrap();
        let output_file = current_dir.join(output_filename);
        _ = std::fs::remove_file(&output_file);

        Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                "screenshot-%09d.png",
                "-vf",
                "palettegen",
                "palette.png",
            ])
            .current_dir(&screenshot_storage.path)
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .output()
            .unwrap();

        Command::new("ffmpeg")
            .args([
                "-i",
                "screenshot-%09d.png",
                "-i",
                "palette.png",
                "-r",
                "60",
                "-filter_complex",
                "paletteuse",
                output_file.to_str().unwrap(),
            ])
            .current_dir(&screenshot_storage.path)
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .output()
            .unwrap();
    }
}
