use std::time::Duration;

use bevy::render::color::Color;
use crossbeam_channel::{bounded, Receiver, Sender};
use futures_util::{
    future::BoxFuture,
    stream::{self, StreamExt},
};
use pumps::{Concurrency, Pipeline};

use crate::{
    pumps_vis::{
        FilterBlock, MapOrderedBlock, MapUnorderedBlock, SinkBlock, SourceBlock, StreamBlock,
    },
    FilteredOutEvent, StreamUpdate, StreamedUnit, UnitAdvanceBlockEvent, UnitCreatedEvent,
    UnitValueKind, UnitValueUpdateEvent,
};

const COLORS: [Color; 4] = [
    Color::rgb(0.50, 0.27, 0.45),
    Color::rgb(0.66, 0.39, 0.39),
    Color::rgb(0.61, 0.27, 0.27),
    Color::rgb(0.26, 0.46, 0.42),
];

pub struct PumpsVisBuilder {
    // stream: BoxStream<'static, StreamedUnit>,
    pipeline: Pipeline<StreamedUnit>,
    blocks: Vec<StreamBlock>,
    tx: Sender<StreamUpdate>,
    rx: Receiver<StreamUpdate>,
    rt: tokio::runtime::Runtime,
}

impl PumpsVisBuilder {
    pub fn source(size: usize) -> Self {
        let (tx, rx) = bounded::<StreamUpdate>(100);

        let tick_tx = tx.clone();
        let tick_stream = stream::iter(0..size).map(move |id| {
            let id = id as u32;
            log::debug!("new stream unit: {}", id);
            let update = StreamUpdate::Created(UnitCreatedEvent {
                id,
                block_id: 0,
                value: UnitValueKind::Value(Color::WHITE),
            });

            tick_tx.send(update.clone()).unwrap();

            StreamedUnit { id, block_id: 0 }
        });

        let rt = tokio::runtime::Runtime::new().unwrap();

        let pipeline = {
            let _g = rt.enter();
            Pipeline::from_stream(tick_stream)
        };

        PumpsVisBuilder {
            // stream: tick_stream.boxed(),
            pipeline,
            blocks: vec![StreamBlock::Source(SourceBlock { id: 0 })],
            tx,
            rx,
            rt,
        }
    }

    pub fn filter(self, timings: Vec<Duration>, filter_ratio: f32) -> Self {
        let id = self.blocks.len() as u32 + 1;

        let color = COLORS[(id as usize) % COLORS.len()];
        let max_duration = *timings.iter().max().unwrap();

        let f = updating_filter(id, self.tx.clone(), timings, filter_ratio, color);

        let pipeline = {
            let _g = self.rt.enter();
            self.pipeline.filter_map(f, Concurrency::serial())
        };

        PumpsVisBuilder {
            pipeline,
            tx: self.tx,
            rx: self.rx,
            rt: self.rt,
            blocks: self
                .blocks
                .into_iter()
                .chain(vec![StreamBlock::FilterBlock(FilterBlock {
                    id,
                    duration: max_duration,
                })])
                .collect(),
        }
    }

    pub fn map_buffered(
        self,
        timings: Vec<Duration>,
        buffered: usize,
        ui_duration: Duration,
    ) -> Self {
        let map_id = self.blocks.len() as u32 + 1;
        let color = COLORS[(map_id as usize) % COLORS.len()];

        let pipeline = {
            let _g = self.rt.enter();
            self.pipeline
                .map(
                    update_stream_state(self.tx.clone(), timings, map_id, color),
                    Concurrency::concurrent_ordered(buffered),
                )
                .backpressure(3)
        };

        PumpsVisBuilder {
            pipeline,
            tx: self.tx,
            rx: self.rx,
            rt: self.rt,
            blocks: self
                .blocks
                .into_iter()
                .chain(vec![StreamBlock::MapBuffer(MapOrderedBlock {
                    id: map_id,
                    duration: ui_duration,
                    concurrency: buffered,
                    units: Default::default(),
                })])
                .collect(),
        }
    }

    pub fn map_buffer_unordered(self, timings: Vec<Duration>, buffered: usize) -> Self {
        let map_id = self.blocks.len() as u32 + 1;
        let color = COLORS[(map_id as usize) % COLORS.len()];

        let max_duration = *timings.iter().max().unwrap();

        let pipeline = {
            let _g = self.rt.enter();
            self.pipeline.map(
                update_stream_state(self.tx.clone(), timings, map_id, color),
                Concurrency::concurrent_unordered(buffered),
            )
        };

        PumpsVisBuilder {
            pipeline,
            tx: self.tx,
            rx: self.rx,
            rt: self.rt,
            blocks: self
                .blocks
                .into_iter()
                .chain(vec![StreamBlock::MapBufferUnordered(
                    MapUnorderedBlock::new(
                        map_id,
                        buffered * 3, // TODO: fix this
                        max_duration,
                        buffered,
                    ),
                )])
                .collect(),
        }
    }

    pub fn sink(self) -> (Vec<StreamBlock>, Receiver<StreamUpdate>) {
        let sink_id = (self.blocks.len() + 1) as u32;

        std::thread::spawn(move || {
            let (mut reciever, _join_handle) = self.pipeline.build();

            self.rt.block_on(async move {
                while let Some(unit) = reciever.recv().await {
                    log::debug!("sink received unit({})", unit.id);
                    self.tx
                        .send(StreamUpdate::AdvanceBlock(UnitAdvanceBlockEvent {
                            id: unit.id,
                            block_id: sink_id,
                            from_block_id: unit.block_id,
                        }))
                        .unwrap();
                }
            })
        });

        let mut blocks = self.blocks;
        blocks.push(StreamBlock::Sink(SinkBlock { id: sink_id }));

        (blocks, self.rx)
    }
}

fn updating_filter(
    phase: u32,
    tx: Sender<StreamUpdate>,
    timings: Vec<Duration>,
    filter_ratio: f32,
    color: Color,
) -> impl FnMut(StreamedUnit) -> BoxFuture<'static, Option<StreamedUnit>> {
    move |unit| {
        let tx = tx.clone();

        tx.send(StreamUpdate::AdvanceBlock(UnitAdvanceBlockEvent {
            id: unit.id,
            block_id: phase,
            from_block_id: unit.block_id,
        }))
        .unwrap();

        tx.send(StreamUpdate::ChangeValue(UnitValueUpdateEvent {
            id: unit.id,
            value: UnitValueKind::PendingFuture(color),
        }))
        .unwrap();

        log::debug!("creating filter future for unit({})", unit.id);
        let duration = timings[unit.id as usize];
        Box::pin(async move {
            log::debug!("calling filter future for unit({})", unit.id);
            let unit_id = unit.id;
            updating_future(unit.clone(), phase, tx.clone(), duration).await;

            let is_in = rand::random::<f32>() < filter_ratio;

            if !is_in {
                tx.send(StreamUpdate::FilteredOut(FilteredOutEvent { id: unit_id }))
                    .unwrap();
            }

            is_in.then_some(unit)
        })
    }
}

async fn updating_future(
    unit: StreamedUnit,
    block_id: u32,
    tx: Sender<StreamUpdate>,
    duration: Duration,
) -> StreamedUnit {
    log::debug!(
        "starting future for unit({}) buffer({}) duration({})",
        unit.id,
        block_id,
        duration.as_millis()
    );
    let interval = 5;

    tx.send(StreamUpdate::ChangeValue(UnitValueUpdateEvent {
        id: unit.id,
        value: UnitValueKind::RunningFuture(0.),
    }))
    .unwrap();

    for i in 1..interval + 1 {
        log::trace!(
            "updating future for unit({}) buffer({}) {}/{} sleep {:?}",
            unit.id,
            block_id,
            i,
            interval,
            duration / interval
        );
        tokio::time::sleep(duration / interval).await;
        tx.send(StreamUpdate::ChangeValue(UnitValueUpdateEvent {
            id: unit.id,
            value: UnitValueKind::RunningFuture(i as f32 / interval as f32),
        }))
        .unwrap();
        log::trace!(
            "done update future for unit({}) buffer({}) {}/{}",
            unit.id,
            block_id,
            i,
            interval
        );
    }

    log::debug!("future done for unit({}) buffer({})", unit.id, block_id);
    StreamedUnit {
        id: unit.id,
        block_id,
    }
}

fn update_stream_state(
    tx: Sender<StreamUpdate>,
    // duration: JitteringDuration,
    timings: Vec<Duration>,
    phase2: u32,
    color: Color,
) -> impl Fn(StreamedUnit) -> BoxFuture<'static, StreamedUnit> {
    move |unit| {
        tx.send(StreamUpdate::AdvanceBlock(UnitAdvanceBlockEvent {
            id: unit.id,
            block_id: phase2,
            from_block_id: unit.block_id,
        }))
        .unwrap();

        tx.send(StreamUpdate::ChangeValue(UnitValueUpdateEvent {
            id: unit.id,
            value: UnitValueKind::PendingFuture(color),
        }))
        .unwrap();

        let tx = tx.clone();
        let block_id = phase2;

        log::debug!(
            "creating map future for unit({}), map_buffered({})",
            unit.id,
            block_id,
        );
        Box::pin(updating_future(
            StreamedUnit {
                id: unit.id,
                block_id,
            },
            block_id,
            tx,
            timings[unit.id as usize],
        ))
    }
}
