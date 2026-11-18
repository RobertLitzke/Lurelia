//! GPU time per part of the frame, from timestamp queries. Off unless
//! `VOXEL_GPU_PROFILE=1` is set (and the adapter supports timestamps inside
//! passes); then every 60 frames it prints the average milliseconds of each
//! part to stderr as a `GPUPROF` line. Reading the queries back stalls the
//! CPU on the GPU every frame, so frame rates while profiling mean nothing;
//! the per-part times are what to read.

/// The points written each frame, in order; part i is the time from point
/// i - 1 to point i.
pub const POINTS: [&str; 9] = ["start", "shadows", "terrain", "creatures", "sprites", "sky", "motes", "water", "post"];

pub struct GpuProf {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period_ns: f32,
    sums: [f64; POINTS.len()],
    frames: u32,
}

impl GpuProf {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let n = POINTS.len() as u32;
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu prof"),
            ty: wgpu::QueryType::Timestamp,
            count: n,
        });
        let size = n as u64 * 8;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu prof resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu prof read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            set,
            resolve,
            read,
            period_ns: queue.get_timestamp_period(),
            sums: [0.0; POINTS.len()],
            frames: 0,
        }
    }

    pub fn at_encoder(&self, enc: &mut wgpu::CommandEncoder, point: usize) {
        enc.write_timestamp(&self.set, point as u32);
    }

    pub fn in_pass(&self, pass: &mut wgpu::RenderPass<'_>, point: usize) {
        pass.write_timestamp(&self.set, point as u32);
    }

    /// Copy this frame's timestamps out; call last on the encoder.
    pub fn resolve(&self, enc: &mut wgpu::CommandEncoder) {
        enc.resolve_query_set(&self.set, 0..POINTS.len() as u32, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, POINTS.len() as u64 * 8);
    }

    /// After submitting: wait for the frame, add it up, print every 60 frames.
    pub fn collect(&mut self, device: &wgpu::Device) {
        let slice = self.read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::Wait);
        let t: Vec<u64> = bytemuck::cast_slice(&slice.get_mapped_range()).to_vec();
        self.read.unmap();
        for i in 1..POINTS.len() {
            self.sums[i] += t[i].saturating_sub(t[i - 1]) as f64 * self.period_ns as f64 / 1e6;
        }
        self.frames += 1;
        if self.frames == 60 {
            let f = self.frames as f64;
            let total: f64 = self.sums.iter().sum::<f64>() / f;
            let parts: Vec<String> = (1..POINTS.len()).map(|i| format!("{} {:.3}", POINTS[i], self.sums[i] / f)).collect();
            eprintln!("GPUPROF total {:.3} | {}", total, parts.join(" | "));
            self.sums = [0.0; POINTS.len()];
            self.frames = 0;
        }
    }
}
