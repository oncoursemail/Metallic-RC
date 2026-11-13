//! Frame timing, for the benchmark (`examples/bench.rs`) and the quality auto-detect: the wall time of each frame,
//! the main world's CPU time (`First`..`Last`), the render world's (its `Render` schedule; it includes waiting for a
//! free swap-chain image, so it grows when the GPU is the bottleneck), and the GPU time per pass that Bevy's
//! `RenderDiagnosticsPlugin` reads from timestamp queries (Vulkan and DX12 only; elsewhere it stays empty).
use std::{
    collections::BTreeMap,
    sync::{atomic::{AtomicU64, Ordering}, Mutex},
    time::Instant,
};

use bevy::{
    diagnostic::DiagnosticsStore,
    prelude::*,
    render::{Render, RenderApp, RenderSystems},
};

/// The render world's last `Render` schedule, in microseconds (written by the render thread).
static RENDER_US: AtomicU64 = AtomicU64::new(0);
static RENDER_START: Mutex<Option<Instant>> = Mutex::new(None);

/// One frame's times, in milliseconds (`gpu_ms` is 0 until the GPU timings arrive; they lag a few frames).
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub frame_ms: f32,
    pub main_ms: f32,
    pub render_ms: f32,
    pub gpu_ms: f32,
}

/// What `PerfPlugin` measured. Samples are kept only while `recording` (the benchmark's measured lap); `last` and
/// `smoothed_ms` always follow the latest frames.
#[derive(Resource, Default)]
pub struct Frames {
    pub recording: bool,
    pub samples: Vec<Sample>,
    pub last: Sample,
    /// exponential average of the wall frame time (about the last second)
    pub smoothed_ms: f32,
    /// per render diagnostic (`render/<pass>/<field>`): sum and count of the values seen while recording
    pub passes: BTreeMap<String, (f64, u32)>,
    seen: Option<Instant>,
    main_start: Option<Instant>,
}

impl Frames {
    /// The `p`th percentile (0..=100) of `f` over the recorded samples.
    pub fn percentile(&self, p: f32, f: impl Fn(&Sample) -> f32) -> f32 {
        let mut v: Vec<f32> = self.samples.iter().map(f).filter(|x| *x > 0.0).collect();
        if v.is_empty() {
            return 0.0;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        v[((p / 100.0 * (v.len() - 1) as f32).round() as usize).min(v.len() - 1)]
    }

    /// The mean of a render diagnostic over the recording (`pass` like `main_opaque_pass_3d`, `field` like
    /// `elapsed_gpu`).
    pub fn pass_mean(&self, key: &str) -> Option<f64> {
        self.passes.get(key).map(|(s, n)| s / *n as f64)
    }
}

/// Frame cap for when vsync is off (uncapped frames only burn power and heat): the main loop waits out the rest of
/// each `1 / max_fps`. `None` = uncapped (the benchmark). The client sets it from the settings key `max_fps`
/// (0 = uncapped); phones and headsets always run with vsync, which caps them already.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FrameLimit {
    pub max_fps: Option<f32>,
    last: Option<Instant>,
}

impl FrameLimit {
    pub fn new(max_fps: Option<f32>) -> Self {
        FrameLimit { max_fps: max_fps.filter(|f| *f > 0.0), last: None }
    }
}

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Frames>()
            .init_resource::<FrameLimit>()
            .add_systems(First, main_start)
            .add_systems(Last, (main_end, limit.after(main_end)));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(Render, (render_start.before(RenderSystems::ExtractCommands), render_end.after(RenderSystems::PostCleanup)));
        }
    }
}

fn limit(mut l: ResMut<FrameLimit>) {
    let now = Instant::now();
    if let (Some(fps), Some(last)) = (l.max_fps, l.last) {
        let frame = std::time::Duration::from_secs_f32(1.0 / fps);
        if let Some(rest) = frame.checked_sub(now - last) {
            std::thread::sleep(rest);
        }
    }
    l.last = Some(Instant::now());
}

fn main_start(mut frames: ResMut<Frames>) {
    frames.main_start = Some(Instant::now());
}

fn render_start() {
    *RENDER_START.lock().unwrap() = Some(Instant::now());
}

fn render_end() {
    if let Some(t) = RENDER_START.lock().unwrap().take() {
        RENDER_US.store(t.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
}

fn main_end(time: Res<Time<Real>>, store: Option<Res<DiagnosticsStore>>, mut frames: ResMut<Frames>) {
    let frame_ms = time.delta_secs() * 1000.0;
    let main_ms = frames.main_start.map_or(0.0, |t| t.elapsed().as_secs_f32() * 1000.0);
    let render_ms = RENDER_US.load(Ordering::Relaxed) as f32 / 1000.0;
    // the newest GPU timings: the top-level passes' elapsed_gpu, taken once per batch of measurements
    let mut gpu_ms = 0.0;
    if let Some(store) = store {
        let mut newest = frames.seen;
        let recording = frames.recording;
        for d in store.iter().filter(|d| d.path().as_str().starts_with("render/")) {
            let Some(m) = d.measurement() else { continue };
            if frames.seen.is_some_and(|s| m.time <= s) {
                continue;
            }
            newest = newest.max(Some(m.time));
            let path = d.path().as_str();
            if path.ends_with("/elapsed_gpu") && d.path().components().count() == 3 {
                gpu_ms += m.value as f32;
            }
            if recording {
                let e = frames.passes.entry(path["render/".len()..].to_string()).or_default();
                e.0 += m.value;
                e.1 += 1;
            }
        }
        frames.seen = newest;
    }
    // a frame without fresh GPU timings records 0 (left out of percentiles); `last` keeps the previous value
    let s = Sample { frame_ms, main_ms, render_ms, gpu_ms };
    frames.last = Sample { gpu_ms: if gpu_ms > 0.0 { gpu_ms } else { frames.last.gpu_ms }, ..s };
    frames.smoothed_ms = if frames.smoothed_ms == 0.0 { frame_ms } else { frames.smoothed_ms * 0.95 + frame_ms * 0.05 };
    if frames.recording {
        frames.samples.push(s);
    }
}
