use std::time::Duration;

use bevy::render::color::Color;
use crossbeam_channel::{bounded, Receiver, Sender};
use futures_util::{
    future::BoxFuture,
    stream::{self, BoxStream, StreamExt},
};

use crate::{
    stream_vis::{
        BufferBlock, BufferUnrderedBlock, FilterBlock, SinkBlock, SourceBlock, StreamBlock,
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

#[derive(Clone, Copy)]
pub struct JitteringDuration {
    pub duration: Duration,
    pub jitter: f32,
}

impl JitteringDuration {
    pub fn from_millis(millis: u64, jitter: f32) -> Self {
        JitteringDuration {
            duration: Duration::from_millis(millis),
            jitter,
        }
    }

    pub fn get(&self) -> Duration {
        let jitter = self.duration.mul_f32(self.jitter);
        let delta = jitter.mul_f32(rand::random::<f32>());
        self.duration + delta
    }
}

pub struct StreamVisBuilder {
    stream: BoxStream<'static, StreamedUnit>,
    blocks: Vec<StreamBlock>,
    tx: Sender<StreamUpdate>,
    rx: Receiver<StreamUpdate>,
}

impl StreamVisBuilder {
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

        StreamVisBuilder {
            stream: tick_stream.boxed(),
            blocks: vec![StreamBlock::Source(SourceBlock { id: 0 })],
            tx,
            rx,
        }
    }

    pub fn filter(self, async_duration: JitteringDuration, filter_ratio: f32) -> Self {
        let id = self.blocks.len() as u32 + 1;

        let color = COLORS[(id as usize) % COLORS.len()];

        let stream = self
            .stream
            .filter_map(updating_filter(
                id,
                self.tx.clone(),
                async_duration,
                filter_ratio,
                color,
            ))
            .boxed();

        StreamVisBuilder {
            stream,
            tx: self.tx,
            rx: self.rx,
            blocks: self
                .blocks
                .into_iter()
                .chain(vec![StreamBlock::FilterBlock(FilterBlock {
                    id: id,
                    duration: async_duration.duration.clone(),
                })])
                .collect(),
        }
    }

    pub fn map_buffered(self, async_duration: JitteringDuration, buffered: usize) -> Self {
        let map_id = self.blocks.len() as u32 + 1;
        let color = COLORS[(map_id as usize) % COLORS.len()];

        let stream = self
            .stream
            .map(update_stream_state(
                self.tx.clone(),
                async_duration,
                map_id,
                color,
            ))
            .buffered(buffered)
            .boxed();

        StreamVisBuilder {
            stream,
            tx: self.tx,
            rx: self.rx,
            blocks: self
                .blocks
                .into_iter()
                .chain(vec![StreamBlock::MapBuffer(BufferBlock {
                    id: map_id,
                    duration: async_duration.duration.clone(),
                    buffered: buffered,
                    units: Default::default(),
                })])
                .collect(),
        }
    }

    pub fn map_buffer_unordered(self, async_duration: JitteringDuration, buffered: usize) -> Self {
        let map_id = self.blocks.len() as u32 + 1;
        let color = COLORS[(map_id as usize) % COLORS.len()];

        let stream = self
            .stream
            .map(update_stream_state(
                self.tx.clone(),
                async_duration,
                map_id,
                color,
            ))
            .buffer_unordered(buffered)
            .boxed();

        StreamVisBuilder {
            stream,
            tx: self.tx,
            rx: self.rx,
            blocks: self
                .blocks
                .into_iter()
                .chain(vec![StreamBlock::MapBufferUnordered(
                    BufferUnrderedBlock::new(
                        map_id,
                        buffered * 3, // TODO: fix this
                        async_duration.duration.clone(),
                        buffered,
                    ),
                )])
                .collect(),
        }
    }

    pub fn sink(self) -> (Vec<StreamBlock>, Receiver<StreamUpdate>) {
        let sink_id = (self.blocks.len() + 1) as u32;

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let mut stream = self.stream;
            rt.block_on(async move {
                while let Some(unit) = stream.next().await {
                    log::debug!("sink received unit({})", unit.id);
                    self.tx
                        .send(StreamUpdate::AdvanceBlock(UnitAdvanceBlockEvent {
                            id: unit.id,
                            block_id: sink_id,
                            from_block_id: unit.block_id.clone(),
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
    duration: JitteringDuration,
    filter_ratio: f32,
    color: Color,
) -> impl FnMut(StreamedUnit) -> BoxFuture<'static, Option<StreamedUnit>> {
    move |unit| {
        let tx = tx.clone();

        tx.send(StreamUpdate::AdvanceBlock(UnitAdvanceBlockEvent {
            id: unit.id,
            block_id: phase.clone(),
            from_block_id: unit.block_id.clone(),
        }))
        .unwrap();

        tx.send(StreamUpdate::ChangeValue(UnitValueUpdateEvent {
            id: unit.id,
            value: UnitValueKind::PendingFuture(color),
        }))
        .unwrap();

        log::debug!("creating filter future for unit({})", unit.id);
        Box::pin(async move {
            log::debug!("calling filter future for unit({})", unit.id);
            let unit_id = unit.id.clone();
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
    duration: JitteringDuration,
) -> StreamedUnit {
    let duration = duration.get();
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
    duration: JitteringDuration,
    phase2: u32,
    color: Color,
) -> impl Fn(StreamedUnit) -> BoxFuture<'static, StreamedUnit> {
    move |unit| {
        tx.send(StreamUpdate::AdvanceBlock(UnitAdvanceBlockEvent {
            id: unit.id,
            block_id: phase2.clone(),
            from_block_id: unit.block_id.clone(),
        }))
        .unwrap();

        tx.send(StreamUpdate::ChangeValue(UnitValueUpdateEvent {
            id: unit.id,
            value: UnitValueKind::PendingFuture(color),
        }))
        .unwrap();

        let tx = tx.clone();
        let block_id = phase2.clone();

        log::debug!(
            "creating map future for unit({}), map_buffered({})",
            unit.id,
            block_id,
        );
        Box::pin(updating_future(
            StreamedUnit {
                id: unit.id,
                block_id: block_id.clone(),
            },
            block_id,
            tx,
            duration,
        ))
    }
}
