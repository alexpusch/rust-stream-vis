mod future_vis;
mod stream_vis;
mod stream_vis_builder;

use argh::FromArgs;
use bevy_tweening::TweeningPlugin;
use crossbeam_channel::Receiver;

use stream_vis::{spawn_blocks, BG_COLOR, SECTION_HEIGHT};
use stream_vis_builder::{JitteringDuration, StreamVisBuilder};

use crate::stream_vis::{advance_units, create_units, handle_filtered_out, update_units};
use bevy::{
    prelude::*,
    render::view::screenshot::ScreenshotManager,
    sprite::MaterialMesh2dBundle,
    window::{PrimaryWindow, WindowCloseRequested},
};
use std::{
    env,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

#[derive(Component)]
struct MapBlock;

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

#[derive(Clone)]
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
    pub frames: Arc<Mutex<Vec<(u128, Image)>>>,
}

#[tokio::main]
async fn main() {
    let _ = env_logger::builder().format_timestamp_millis().try_init();
    let config: Config = argh::from_env();

    App::new()
        .add_event::<StreamEvent>()
        .add_plugins(DefaultPlugins)
        .add_plugins(TweeningPlugin)
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, read_stream)
        .add_systems(PreUpdate, create_units.after(read_stream))
        .add_systems(FixedUpdate, advance_units.after(create_units))
        .add_systems(FixedUpdate, update_units.after(advance_units))
        .add_systems(FixedUpdate, handle_filtered_out.after(advance_units))
        .add_systems(FixedUpdate, save_frame)
        .add_systems(Update, save_gif)
        .insert_resource(config)
        .insert_resource(ScreenshotStorage {
            started_writing: false,
            frames: Default::default(),
        })
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    asset_server: Res<AssetServer>,
    mut window: Query<&mut Window>,
) {
    let mut window = window.single_mut();
    window.resolution.set(800., SECTION_HEIGHT + 50.);

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

    // buffer 1
    // let (blocks, rx) = StreamVisBuilder::source(3)
    //     .map_buffered(JitteringDuration::from_millis(500, 3.), 1)
    //     .sink();

    // buffer 5
    // let (blocks, rx) = StreamVisBuilder::source(15)
    //     .map_buffered(JitteringDuration::from_millis(800, 4.), 5)
    //     .sink();

    // buffer unordered 5
    // let (blocks, rx) = StreamVisBuilder::source(15)
    //     .map_buffer_unordered(JitteringDuration::from_millis(500, 3.), 5)
    //     .sink();

    // filter
    // let (blocks, rx) = StreamVisBuilder::source(3)
    //     .filter(JitteringDuration::from_millis(500, 1.), 0.5)
    //     .sink();

    // buffer filter long
    let (blocks, rx) = StreamVisBuilder::source(10)
        .map_buffered(JitteringDuration::from_millis(500, 3.), 5)
        .filter(JitteringDuration::from_millis(1200, 1.), 0.5)
        .sink();

    // buffer unordered filter long
    // let (blocks, rx) = StreamVisBuilder::source(10)
    //     .map_buffer_unordered(JitteringDuration::from_millis(500, 3.), 5)
    //     .filter(JitteringDuration::from_millis(1200, 1.), 0.5)
    //     .sink();

    // let (blocks, rx) = StreamVisBuilder::source(10)
    //     .map_buffered(JitteringDuration::from_millis(500, 3.), 5)
    //     .map_buffered(JitteringDuration::from_millis(1000, 2.), 3)
    //     .sink();

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
    time: Res<Time>,
) {
    if screenshot_storage.started_writing {
        return;
    }

    let frames = screenshot_storage.frames.clone();
    let counter = time.elapsed().as_micros();

    _ = screenshot_manager.take_screenshot(main_window.single(), move |img| {
        frames.lock().unwrap().push((counter, img));
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
        let output_file = current_dir.join(&output_filename);
        _ = std::fs::remove_file(&output_file);

        let screenshot_dir = tempfile::tempdir().unwrap();
        let frames = screenshot_storage.frames.lock().unwrap();
        for (i, frame) in frames.iter().enumerate() {
            save_screenshot_to_disk(
                &frame.1,
                &screenshot_dir
                    .path()
                    .join(format!("screenshot-{:0>9}.png", i)),
            );
        }

        Command::new("ffmpeg")
            .args(&[
                "-y",
                "-i",
                "screenshot-%09d.png",
                "-vf",
                "palettegen",
                "palette.png",
            ])
            .current_dir(&screenshot_dir)
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .output()
            .unwrap();

        Command::new("ffmpeg")
            .args(&[
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
            .current_dir(&screenshot_dir)
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .output()
            .unwrap();
    }
}

fn save_screenshot_to_disk(img: &Image, path: &Path) {
    match img.clone().try_into_dynamic() {
        Ok(dyn_img) => match image::ImageFormat::from_path(&path) {
            Ok(format) => {
                // discard the alpha channel which stores brightness values when HDR is enabled to make sure
                // the screenshot looks right
                let img = dyn_img.to_rgb8();
                match img.save_with_format(&path, format) {
                    Ok(_) => debug!("Screenshot saved to {}", path.display()),
                    Err(e) => error!("Cannot save screenshot, IO error: {e}"),
                }
            }
            Err(e) => error!("Cannot save screenshot, requested format not recognized: {e}"),
        },
        Err(e) => error!("Cannot save screenshot, screen format cannot be understood: {e}"),
    }
}
