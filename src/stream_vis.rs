use std::{collections::VecDeque, time::Duration};

use bevy::{
    prelude::*,
    render::{mesh::Indices, render_resource::PrimitiveTopology},
    sprite::{Anchor, MaterialMesh2dBundle},
};
use bevy_tweening::{
    lens::{ColorMaterialColorLens, TransformPositionLens},
    Animator, AssetAnimator, EaseFunction, Tween,
};

use crate::{
    future_vis::{spawn_unit, StreamUnit, UnitBackground, UnitFutureProgress, UnitStroke},
    StreamEvent, StreamUpdate, UnitValueKind,
};

#[derive(Component, Default, Clone)]
pub struct BufferBlock {
    pub id: u32,
    pub duration: Duration,
    pub buffered: usize,
    pub units: VecDeque<u32>,
}

#[derive(Component, Default, Clone)]
pub struct BufferUnrderedBlock {
    pub id: u32,
    pub duration: Duration,
    pub buffered: usize,
    pub slots: VecDeque<Option<u32>>,
}

impl BufferUnrderedBlock {
    pub fn new(id: u32, size: usize, duration: Duration, buffered: usize) -> Self {
        Self {
            id,
            duration,
            buffered,
            slots: vec![None; size].into_iter().collect(),
        }
    }
}

#[derive(Component, Default, Clone)]
pub struct FilterBlock {
    pub id: u32,
    pub duration: Duration,
}

#[derive(Component, Clone)]
pub struct SourceBlock {
    pub id: u32,
}

#[derive(Component, Clone)]
pub struct SinkBlock {
    pub id: u32,
}

#[derive(Component, Clone)]
pub enum StreamBlock {
    Source(SourceBlock),
    MapBuffer(BufferBlock),
    MapBufferUnordered(BufferUnrderedBlock),
    FilterBlock(FilterBlock),
    Sink(SinkBlock),
}

impl StreamBlock {
    pub fn id(&self) -> u32 {
        match self {
            StreamBlock::Source(block) => block.id,
            StreamBlock::MapBuffer(block) => block.id,
            StreamBlock::MapBufferUnordered(block) => block.id,
            StreamBlock::FilterBlock(block) => block.id,
            StreamBlock::Sink(block) => block.id,
        }
    }
}

const BLOCK_PADDING: f32 = 5.;
const SECTION_MARGIN: f32 = 80.;
pub const BG_COLOR: Color = Color::rgb(34. / 255.0, 39. / 255.0, 46. / 255.0);

const UNIT_SIZE: f32 = 15.;
pub const SECTION_HEIGHT: f32 = 250.;

// buffer
const BUFFER_WIDTH: f32 = 7. * UNIT_SIZE + BLOCK_PADDING * 2.;
const BUFFER_HEIGHT: f32 = UNIT_SIZE + BLOCK_PADDING * 2.;
pub const BUFFER_COLOR: Color = Color::rgb(0.95, 0.71, 0.39);

// buffered unordered
const BUFFER_UNORDERED_WIDTH: f32 = UNIT_SIZE + BLOCK_PADDING * 2.;
const BUFFER_UNORDERED_HEIGHT: f32 = 9. * UNIT_SIZE + BLOCK_PADDING * 2.;
const BUFFER_UNORDERED_COLOR: Color = Color::rgb(0.95, 0.92, 0.56);

// filter
const FILTER_WIDTH: f32 = UNIT_SIZE + BLOCK_PADDING * 2.;
const FILTER_HEIGHT: f32 = UNIT_SIZE + BLOCK_PADDING * 2.;
const FILTER_COLOR: Color = Color::rgb(0.62, 0.73, 0.45);

// source/sink
const SOURCE_RAD: f32 = 50.;
const SOURCE_COLOR: Color = Color::rgb(0.73, 0.71, 0.78);

// text
const FONT_SIZE: f32 = 16.;
const TEXT_MARGIN: f32 = 120.;

fn dashed_line(len: f32, segment_len: f32, segment_width: f32) -> Mesh {
    let segments_count = (len as usize) / (segment_len as usize);

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();

    let mut indices = Vec::new();

    for i in (0..segments_count).step_by(2) {
        let x = 0.;
        let y = i as f32 * segment_len - len / 2.;

        positions.push([x, y, 0.]);
        normals.push([0., 0., 1.]);
        uvs.push([0., 0.]);

        positions.push([x, y + segment_len, 0.]);
        normals.push([0., 0., 1.]);
        uvs.push([0., 0.]);

        positions.push([x + segment_width, y + segment_len, 0.]);
        normals.push([0., 0., 1.]);
        uvs.push([0., 0.]);

        positions.push([x + segment_width, y, 0.]);
        normals.push([0., 0., 1.]);
        uvs.push([0., 0.]);

        indices.extend_from_slice(&[
            (0 + i * 2) as u32,
            (1 + i * 2) as u32,
            (2 + i * 2) as u32,
            (0 + i * 2) as u32,
            (2 + i * 2) as u32,
            (3 + i * 2) as u32,
        ]);
    }

    Mesh::new(PrimitiveTopology::TriangleList)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_indices(Some(Indices::U32(indices)))
}

fn crecent_mesh(sides: usize, radius: f32) -> Mesh {
    let mut positions = Vec::with_capacity(sides);
    let mut normals = Vec::with_capacity(sides);
    let mut uvs = Vec::with_capacity(sides);

    let step = std::f32::consts::TAU / sides as f32;
    for i in 0..sides {
        let theta = std::f32::consts::FRAC_PI_2 - i as f32 * step;
        let (sin, cos) = theta.sin_cos();

        positions.push([cos * radius, sin * radius, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([0.5 * (cos + 1.0), 1.0 - 0.5 * (sin + 1.0)]);
    }

    let mut indices = Vec::with_capacity((sides - 2) * 3);
    for i in 1..(sides as u32 - 20) {
        indices.extend_from_slice(&[0, i + 1, i]);
    }

    for i in (sides as u32 + 20)..(sides as u32 - 1) {
        indices.extend_from_slice(&[0, i + 1, i]);
    }

    Mesh::new(PrimitiveTopology::TriangleList)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_indices(Some(Indices::U32(indices)))
}

fn spawn_buffered(
    buffer_block: BufferBlock,
    transform: Transform,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    asset_server: &Res<AssetServer>,
) {
    let font_handle = asset_server.load("Virgil.ttf");

    commands
        .spawn((
            StreamBlock::MapBuffer(buffer_block.clone()),
            SpatialBundle::from_transform(transform),
        ))
        .with_children(|parent| {
            parent.spawn(Text2dBundle {
                text_anchor: Anchor::Center,
                text: Text::from_sections([
                    TextSection::new(
                        ".map(",
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        buffer_block.duration.as_millis().to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::RED,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        "ms)",
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        "\n.buffer(".to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        buffer_block.buffered.to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::RED,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        ")".to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle,
                        },
                    ),
                ]),
                transform: Transform::from_translation(Vec3::new(
                    BUFFER_WIDTH / 2.,
                    -TEXT_MARGIN,
                    200.,
                )),
                ..default()
            });

            parent.spawn(MaterialMesh2dBundle {
                mesh: meshes
                    .add(
                        shape::Box::from_corners(
                            Vec3::new(0., BUFFER_HEIGHT / -2., 0.),
                            Vec3::new(BUFFER_WIDTH, BUFFER_HEIGHT / 2., 0.),
                        )
                        .into(),
                    )
                    .into(),
                material: materials.add(ColorMaterial::from(BUFFER_COLOR)),
                ..default()
            });
        });
}

fn spawn_buffer_unordered(
    block: BufferUnrderedBlock,
    transform: Transform,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    asset_server: &Res<AssetServer>,
) {
    commands
        .spawn((
            StreamBlock::MapBufferUnordered(block.clone()),
            SpatialBundle::from_transform(transform),
        ))
        .with_children(|parent| {
            let font_handle = asset_server.load("Virgil.ttf");
            parent.spawn(Text2dBundle {
                text_anchor: Anchor::Center,
                text: Text::from_sections([
                    TextSection::new(
                        ".map(",
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        block.duration.as_millis().to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::RED,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        "ms)",
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        "\n.buffered_unordered(".to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        block.buffered.to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::RED,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        ")".to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle,
                        },
                    ),
                ]),
                transform: Transform::from_translation(Vec3::new(
                    BUFFER_UNORDERED_WIDTH / 2.,
                    -TEXT_MARGIN,
                    200.,
                )),
                ..default()
            });

            parent.spawn(MaterialMesh2dBundle {
                mesh: meshes
                    .add(
                        shape::Box::from_corners(
                            Vec3::new(0., -BUFFER_UNORDERED_HEIGHT / 2., 0.),
                            Vec3::new(BUFFER_UNORDERED_WIDTH, BUFFER_UNORDERED_HEIGHT / 2., 0.),
                        )
                        .into(),
                    )
                    .into(),
                material: materials.add(ColorMaterial::from(BUFFER_UNORDERED_COLOR)),
                ..default()
            });
        });
}

fn spawn_filter(
    block: FilterBlock,
    transform: Transform,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    asset_server: &Res<AssetServer>,
) {
    commands
        .spawn((
            StreamBlock::FilterBlock(block.clone()),
            SpatialBundle::from_transform(transform),
        ))
        .with_children(|parent| {
            let font_handle = asset_server.load("Virgil.ttf");
            parent.spawn(Text2dBundle {
                text_anchor: Anchor::Center,
                text: Text::from_sections([
                    TextSection::new(
                        ".filter(",
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        block.duration.as_millis().to_string(),
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::RED,
                            font: font_handle.clone(),
                        },
                    ),
                    TextSection::new(
                        "ms)",
                        TextStyle {
                            font_size: FONT_SIZE,
                            color: Color::WHITE,
                            font: font_handle.clone(),
                        },
                    ),
                ]),
                transform: Transform::from_translation(Vec3::new(
                    FILTER_WIDTH / 2.,
                    -TEXT_MARGIN,
                    200.,
                )),
                ..default()
            });

            parent.spawn(MaterialMesh2dBundle {
                mesh: meshes
                    .add(
                        shape::Box::from_corners(
                            Vec3::new(0., -1. * FILTER_HEIGHT / 2., 0.),
                            Vec3::new(FILTER_WIDTH, FILTER_HEIGHT / 2., 0.),
                        )
                        .into(),
                    )
                    .into(),
                material: materials.add(ColorMaterial::from(FILTER_COLOR)),
                transform: Transform::from_translation(Vec3::new(0., 0., 0.)),
                ..default()
            });
        });
}

fn spawn_source(
    block: SourceBlock,
    transform: Transform,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let sides = 64;
    let radius = SOURCE_RAD / 2.;

    let mesh = crecent_mesh(sides, radius);

    commands
        .spawn((
            StreamBlock::Source(block),
            SpatialBundle::from_transform(transform),
        ))
        .with_children(|parent| {
            let mut transform = Transform::from_translation(Vec3::new(0., 0., 200.));
            transform.rotate_z(std::f32::consts::TAU * 0.59);

            parent.spawn(MaterialMesh2dBundle {
                mesh: meshes.add(mesh).into(),
                material: materials.add(ColorMaterial::from(SOURCE_COLOR)),
                transform,
                ..default()
            });
        });
}

fn spawn_sink(
    block: SinkBlock,
    transform: Transform,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let sides = 64;
    let radius = SOURCE_RAD / 2.;

    let mesh = crecent_mesh(sides, radius);

    let mut transform = transform.clone();
    transform.translation.z = 100.;
    transform.rotate_z(std::f32::consts::TAU * 0.095);

    commands.spawn((
        MaterialMesh2dBundle {
            mesh: meshes.add(mesh).into(),
            material: materials.add(ColorMaterial::from(SOURCE_COLOR)),
            transform,
            ..default()
        },
        StreamBlock::Sink(block),
    ));
}

fn spawn_divider(
    transform: Transform,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(MaterialMesh2dBundle {
        mesh: meshes.add(dashed_line(SECTION_HEIGHT, 5., 2.)).into(),
        material: materials.add(ColorMaterial::from(Color::rgba_u8(250, 240, 230, 80))),
        transform,
        ..default()
    });
}

pub fn spawn_blocks(
    blocks: Vec<StreamBlock>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    assets_server: Res<AssetServer>,
) -> f32 {
    let start_pos = Vec3::new(0., 0., 0.);
    let mut transform = Transform::from_translation(start_pos);

    for block in blocks {
        match block {
            StreamBlock::Source(block) => {
                spawn_source(block, transform, commands, meshes, materials);

                transform.translation += Vec3::new(SECTION_MARGIN, 0., 0.);
                spawn_divider(transform, commands, meshes, materials);
            }
            StreamBlock::MapBuffer(map_buffer_block) => {
                transform.translation += Vec3::new(SECTION_MARGIN / 2., 0., 0.);

                spawn_buffered(
                    map_buffer_block,
                    transform,
                    commands,
                    meshes,
                    materials,
                    &assets_server,
                );
                transform.translation += Vec3::new(SECTION_MARGIN / 2. + BUFFER_WIDTH, 0., 0.);

                spawn_divider(transform, commands, meshes, materials);
            }
            StreamBlock::MapBufferUnordered(map_buffer_block) => {
                transform.translation += Vec3::new(SECTION_MARGIN, 0., 0.);

                spawn_buffer_unordered(
                    map_buffer_block,
                    transform,
                    commands,
                    meshes,
                    materials,
                    &assets_server,
                );

                transform.translation += Vec3::new(SECTION_MARGIN + BUFFER_UNORDERED_WIDTH, 0., 0.);
                spawn_divider(transform, commands, meshes, materials);
            }
            StreamBlock::FilterBlock(filter) => {
                transform.translation += Vec3::new(SECTION_MARGIN / 2., 0., 0.);

                spawn_filter(
                    filter,
                    transform,
                    commands,
                    meshes,
                    materials,
                    &assets_server,
                );

                transform.translation += Vec3::new(SECTION_MARGIN / 2. + FILTER_WIDTH, 0., 0.);
                spawn_divider(transform, commands, meshes, materials);
            }
            StreamBlock::Sink(block) => {
                transform.translation += Vec3::new(SECTION_MARGIN, 0., 0.);

                spawn_sink(block, transform, commands, meshes, materials);
            }
        }
    }

    transform.translation.x
}

pub fn handle_filtered_out(
    mut commands: Commands,
    mut reader: EventReader<StreamEvent>,
    mut units: Query<(Entity, &mut StreamUnit, &mut Transform, &Children)>,
    unit_strokes: Query<Entity, With<UnitStroke>>,
    unit_background: Query<Entity, With<UnitBackground>>,
    unit_future_progress: Query<Entity, With<UnitFutureProgress>>,
) {
    if reader.len() == 0 {
        return;
    }

    let events = reader.read().collect::<Vec<_>>();

    let filtered_out_events = events.iter().filter_map(|event| match event.0 {
        StreamUpdate::FilteredOut(ref event) => Some(event),
        _ => None,
    });

    for event in filtered_out_events {
        log::debug!("handling filtered out event {}", event.id);
        let (entity, _, unit_transform, children) = units
            .iter_mut()
            .find(|(_, unit, _, _)| unit.id == event.id)
            .unwrap();

        let pos_tween = Tween::new(
            EaseFunction::ExponentialOut,
            Duration::from_secs(1),
            TransformPositionLens {
                start: glam::Vec3::new(
                    unit_transform.translation.x,
                    unit_transform.translation.y,
                    10.,
                ),
                end: glam::Vec3::new(
                    unit_transform.translation.x,
                    unit_transform.translation.y + FILTER_WIDTH * 1.5,
                    10.,
                ),
            },
        );

        for child in children {
            if let Ok(entity) = unit_future_progress.get(*child) {
                let color_tween = Tween::new(
                    EaseFunction::ExponentialOut,
                    Duration::from_secs(1),
                    ColorMaterialColorLens {
                        start: Color::GRAY,
                        end: Color::GRAY.with_a(0.),
                    },
                );

                commands
                    .entity(entity)
                    .insert(AssetAnimator::new(color_tween));
            }

            if let Ok(entity) = unit_strokes.get(*child) {
                let color_tween = Tween::new(
                    EaseFunction::ExponentialOut,
                    Duration::from_secs(1),
                    ColorMaterialColorLens {
                        start: Color::WHITE,
                        end: Color::WHITE.with_a(0.),
                    },
                );

                commands
                    .entity(entity)
                    .insert(AssetAnimator::new(color_tween));
            }

            if let Ok(entity) = unit_background.get(*child) {
                let color_tween = Tween::new(
                    EaseFunction::ExponentialOut,
                    Duration::from_secs(1),
                    ColorMaterialColorLens {
                        start: Color::WHITE,
                        end: Color::WHITE.with_a(0.),
                    },
                );

                commands
                    .entity(entity)
                    .insert(AssetAnimator::new(color_tween));
            }
        }

        commands.entity(entity).insert(Animator::new(pos_tween));
    }
}

pub fn advance_units(
    mut commands: Commands,
    mut reader: EventReader<StreamEvent>,
    mut blocks: Query<(&mut StreamBlock, &Transform)>,
    mut units: Query<
        (
            Entity,
            &mut StreamUnit,
            &mut Transform,
            // &Handle<ColorMaterial>,
        ),
        Without<StreamBlock>,
    >,
) {
    if reader.len() == 0 {
        return;
    }

    let events = reader.read().collect::<Vec<_>>();

    let advance_block_events = events.iter().filter_map(|event| match event.0 {
        StreamUpdate::AdvanceBlock(ref event) => Some(event),
        _ => None,
    });

    let advance_block_events = advance_block_events.collect::<Vec<_>>();

    let mut blocks = blocks.iter_mut().collect::<Vec<_>>();

    blocks.sort_by_key(|(block, _)| -1 * block.id() as i64);

    for (block, block_transform) in blocks.iter_mut() {
        let block_id = block.id().clone();

        let cur_advance_block_events = advance_block_events
            .iter()
            .filter(|e| e.block_id == block_id)
            .collect::<Vec<_>>();

        let unit_leave_block_events = advance_block_events
            .iter()
            .filter(|e| e.from_block_id == block_id)
            .collect::<Vec<_>>();

        for event in cur_advance_block_events.iter() {
            log::debug!(
                "handling advance block event.  unit({}) from block({}) to block({})",
                event.id,
                event.from_block_id,
                event.block_id
            );

            let (_, mut unit, _) = units
                .iter_mut()
                .find(|(_, unit, _)| unit.id == event.id)
                .unwrap();

            unit.cur_block = event.block_id.clone();

            match block.as_mut() {
                StreamBlock::Sink(_) => {
                    let (entity, _, unit_transform) = units
                        .iter_mut()
                        .find(|(_, unit, _)| unit.id == event.id)
                        .unwrap();

                    let tween = Tween::new(
                        EaseFunction::ExponentialOut,
                        Duration::from_secs(1),
                        TransformPositionLens {
                            start: Vec3::new(
                                unit_transform.translation.x,
                                unit_transform.translation.y,
                                10.,
                            ),
                            end: Vec3::new(
                                block_transform.translation.x,
                                block_transform.translation.y,
                                10.,
                            ),
                        },
                    );
                    commands.entity(entity).insert(Animator::new(tween));
                }

                StreamBlock::FilterBlock(_) => {
                    let (entity, _, unit_transform) = units
                        .iter_mut()
                        .find(|(_, unit, _)| unit.id == event.id)
                        .unwrap();

                    let tween = Tween::new(
                        EaseFunction::ExponentialOut,
                        Duration::from_secs(1),
                        TransformPositionLens {
                            start: Vec3::new(
                                unit_transform.translation.x,
                                unit_transform.translation.y,
                                10.,
                            ),
                            end: Vec3::new(
                                block_transform.translation.x + FILTER_WIDTH / 2.,
                                block_transform.translation.y,
                                10.,
                            ),
                        },
                    );
                    commands.entity(entity).insert(Animator::new(tween));
                }

                StreamBlock::MapBuffer(ref mut block_state) => {
                    block_state.units.push_back(unit.id);
                }
                StreamBlock::MapBufferUnordered(ref mut block_state) => {
                    // put in first non None slot
                    *block_state
                        .slots
                        .iter_mut()
                        .find(|slot| slot.is_none())
                        .unwrap() = Some(unit.id);
                }
                _ => (),
            }
        }

        for event in unit_leave_block_events.iter() {
            match block.as_mut() {
                StreamBlock::MapBuffer(ref mut block_state) => {
                    block_state.units.retain(|id| *id != event.id);
                }
                StreamBlock::MapBufferUnordered(ref mut block_state) => {
                    block_state.slots.iter_mut().for_each(|slot| {
                        if let Some(id) = slot {
                            if *id == event.id {
                                *slot = None;
                            }
                        }
                    });
                }
                _ => (),
            }
        }

        // adjust positions after updates
        match block.as_mut() {
            StreamBlock::MapBuffer(ref mut block_state) => {
                for (i, id) in block_state.units.iter().enumerate() {
                    let (entity, _, transform) = units
                        .iter_mut()
                        .find(|(_, unit, _)| unit.id == *id)
                        .unwrap();

                    let block_br_x = block_transform.translation.x + BUFFER_WIDTH - UNIT_SIZE;
                    let block_br_y = block_transform.translation.y;

                    let pos_in_block = i as i64;

                    let x = block_br_x - (pos_in_block as f32) * (UNIT_SIZE + 5.);
                    let y = block_br_y;

                    let tween = Tween::new(
                        EaseFunction::ExponentialOut,
                        Duration::from_secs(1),
                        TransformPositionLens {
                            start: transform.translation,
                            end: Vec3::new(x, y, transform.translation.z),
                        },
                    );
                    commands.entity(entity).insert(Animator::new(tween));
                }
            }
            StreamBlock::MapBufferUnordered(ref mut block_state) => {
                for (i, id) in block_state.slots.iter().enumerate() {
                    if let Some(id) = id {
                        let (entity, _, transform) = units
                            .iter_mut()
                            .find(|(_, unit, _)| unit.id == *id)
                            .unwrap();

                        let block_x = block_transform.translation.x + BUFFER_UNORDERED_WIDTH / 2.;
                        let block_y = block_transform.translation.y + BUFFER_WIDTH / 2.;

                        let pos_in_block = i as i64;

                        let x = block_x;
                        let y = block_y - (pos_in_block as f32) * (UNIT_SIZE + 5.);

                        let tween = Tween::new(
                            EaseFunction::ExponentialOut,
                            Duration::from_secs(1),
                            TransformPositionLens {
                                start: transform.translation,
                                end: Vec3::new(x, y, transform.translation.z),
                            },
                        );
                        commands.entity(entity).insert(Animator::new(tween));
                    }
                }
            }
            _ => (),
        }
    }
}

pub fn create_units(
    mut commands: Commands,
    mut reader: EventReader<StreamEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut blocks: Query<(&mut StreamBlock, &Transform)>,
) {
    let events = reader.read().collect::<Vec<_>>();

    let create_event = events.iter().filter_map(|event| match event.0 {
        StreamUpdate::Created(ref event) => Some(event),
        _ => None,
    });

    let mut blocks = blocks.iter_mut().collect::<Vec<_>>();

    for event in create_event {
        log::debug!("handling create event {}", event.id);

        let (block, block_transform) = blocks
            .iter_mut()
            .find(|(block, _)| block.id() == event.block_id)
            .unwrap();

        let x = block_transform.translation.x;
        let y = block_transform.translation.y;

        spawn_unit(
            &mut commands,
            &mut meshes,
            &mut materials,
            event.id,
            block.id().clone(),
            Transform::from_translation(Vec3::new(x, y, 10.)),
        );
    }
}

pub fn update_units(
    mut reader: EventReader<StreamEvent>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut units: Query<(&mut StreamUnit, &Children)>,
    mut unit_strokes: Query<(&mut Visibility, &Handle<ColorMaterial>), With<UnitStroke>>,
    unit_background: Query<&Handle<ColorMaterial>, With<UnitBackground>>,
    mut unit_future_progress: Query<
        (&mut Transform, &Handle<ColorMaterial>),
        With<UnitFutureProgress>,
    >,
) {
    let events = reader.read().collect::<Vec<_>>();

    let change_value_event = events.iter().filter_map(|event| match event.0 {
        StreamUpdate::ChangeValue(ref event) => Some(event),
        _ => None,
    });

    for event in change_value_event {
        log::debug!(
            "handling change value event {} to value {:?}",
            event.id,
            event.value
        );

        let (_, children) = units
            .iter_mut()
            .find(|(unit, _)| unit.id == event.id)
            .unwrap();

        match event.value {
            UnitValueKind::PendingFuture(color) => {
                for child in children {
                    if let Ok((mut store_visibility, store_material)) = unit_strokes.get_mut(*child)
                    {
                        *store_visibility = Visibility::Visible;
                        materials.get_mut(store_material).unwrap().color = color;
                    }

                    if let Ok(background) = unit_background.get(*child) {
                        let fade_color = color.with_a(0.1);
                        materials.get_mut(background).unwrap().color = fade_color;
                    }

                    if let Ok((mut progress_transform, progress_matrial)) =
                        unit_future_progress.get_mut(*child)
                    {
                        materials.get_mut(progress_matrial).unwrap().color = color;
                        progress_transform.scale.y = 0.;
                    }
                }
            }
            UnitValueKind::Value(value) => {
                for child in children {
                    if let Ok(background) = unit_background.get(*child) {
                        materials.get_mut(background).unwrap().color = value;
                    }
                }
            }
            UnitValueKind::RunningFuture(progress) => {
                for child in children {
                    if let Ok((mut progress_transform, _)) = unit_future_progress.get_mut(*child) {
                        progress_transform.scale.y = progress;
                        progress_transform.translation.y = -UNIT_SIZE * (1. - progress) / 2.;
                    }

                    if progress == 1. {
                        if let Ok((_, progress_matrial)) = unit_future_progress.get_mut(*child) {
                            let material = materials.get_mut(progress_matrial).unwrap();

                            material.color.set_a(1.);
                            material.color.set_l(material.color.l() * 1.5);
                            material.color.set_s(material.color.s() * 1.5);
                        }
                    }
                }
            }
        }
    }
}
