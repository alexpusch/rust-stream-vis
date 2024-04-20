use std::time::Duration;

use bevy::{
    prelude::*,
    render::{mesh::Indices, render_resource::PrimitiveTopology},
    sprite::MaterialMesh2dBundle,
};

#[derive(Debug)]
pub enum FutureState {
    Pending,
    Running(Timer),
    Done,
}

#[derive(Debug, Component)]
pub struct StreamUnit {
    pub id: u32,
    pub cur_block: u32,
    pub value: Color,
    pub future_state: FutureState,
}

#[derive(Component)]
pub struct UnitStroke;

#[derive(Component)]
pub struct UnitFutureProgress;

#[derive(Component)]
pub struct UnitBackground;
pub const UNIT_WIDTH: f32 = 12.;
pub const UNIT_STROKE_WIDTH: f32 = 1.;

pub fn stroke_mesh(width: f32, stroke_width: f32) -> Mesh {
    let mut positions = Vec::with_capacity(8);
    let mut normals = Vec::with_capacity(8);
    let mut uvs = Vec::with_capacity(8);

    /*
    3------------7
    |  2------6  |
    |  |      |  |
    |  |      |  |
    |  0------4  |
    1------------5
    */

    for i in [-1, 1] {
        for j in [-1, 1] {
            positions.push(Vec3::new(
                (i as f32) * width / 2.,
                (j as f32) * width / 2.,
                0.,
            ));
            normals.push(Vec3::new(0., 0., 1.));
            uvs.push(Vec2::new(0., 0.));

            positions.push(Vec3::new(
                (i as f32) * (width / 2. - stroke_width),
                (j as f32) * (width / 2. - stroke_width),
                0.,
            ));
            normals.push(Vec3::new(0., 0., 1.));
            uvs.push(Vec2::new(0., 0.));
        }
    }

    let indices = vec![
        1, 0, 5, 5, 4, 0, //
        5, 4, 7, 4, 6, 7, //
        3, 2, 7, 2, 7, 6, //
        1, 0, 3, 0, 2, 3, //
    ];

    Mesh::new(PrimitiveTopology::TriangleList)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_indices(Some(Indices::U32(indices)))
}

pub fn spawn_unit(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    id: u32,
    cur_block: u32,
    transform: Transform,
) {
    commands
        .spawn((
            StreamUnit {
                future_state: FutureState::Running(Timer::new(
                    Duration::from_secs(1),
                    TimerMode::Repeating,
                )),
                id,
                cur_block,
                value: Color::RED,
            },
            SpatialBundle::from_transform(transform),
        ))
        .with_children(|parent| {
            parent.spawn((
                MaterialMesh2dBundle {
                    mesh: meshes
                        .add(stroke_mesh(UNIT_WIDTH, UNIT_STROKE_WIDTH))
                        .into(),
                    material: materials.add(ColorMaterial::from(Color::BLACK)),
                    transform: Transform::from_xyz(0., 0., 20.),
                    ..Default::default()
                },
                UnitStroke,
            ));

            parent.spawn((
                MaterialMesh2dBundle {
                    mesh: meshes.add(Mesh::from(shape::Cube::new(UNIT_WIDTH))).into(),
                    material: materials.add(ColorMaterial::from(Color::WHITE)),
                    // transform,
                    ..Default::default()
                },
                UnitBackground,
            ));

            parent.spawn((
                MaterialMesh2dBundle {
                    mesh: meshes
                        .add(Mesh::from(shape::Box::new(UNIT_WIDTH, UNIT_WIDTH, 1.)))
                        .into(),
                    material: materials.add(ColorMaterial::from(Color::BLACK)),
                    transform: Transform::from_xyz(0., 0., 10.),
                    ..Default::default()
                },
                UnitFutureProgress,
            ));
        });
}
