//! Instanced solid rectangles (backgrounds, cursor, underlines).

use bytemuck::{Pod, Zeroable};
use hv_grid::Rgb8;
use wgpu::util::DeviceExt;

/// One rectangle in physical pixels.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Rect {
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

impl Rect {
    pub(crate) fn new(x: f32, y: f32, w: f32, h: f32, c: Rgb8) -> Self {
        Self::with_alpha(x, y, w, h, c, 1.0)
    }

    pub(crate) fn with_alpha(x: f32, y: f32, w: f32, h: f32, c: Rgb8, alpha: f32) -> Self {
        Self {
            pos: [x, y],
            size: [w, h],
            color: [
                f32::from(c.r) / 255.0,
                f32::from(c.g) / 255.0,
                f32::from(c.b) / 255.0,
                alpha,
            ],
        }
    }

    #[cfg(test)]
    pub(crate) fn x(&self) -> f32 {
        self.pos[0]
    }

    #[cfg(test)]
    pub(crate) fn y(&self) -> f32 {
        self.pos[1]
    }

    #[cfg(test)]
    pub(crate) fn w(&self) -> f32 {
        self.size[0]
    }

    #[cfg(test)]
    pub(crate) fn h(&self) -> f32 {
        self.size[1]
    }
}

pub(crate) struct RectPipeline {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
    back_count: u32,
    over_count: u32,
}

impl RectPipeline {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rects"),
            source: wgpu::ShaderSource::Wgsl(include_str!("rect.wgsl").into()),
        });
        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rect globals"),
            contents: bytemuck::cast_slice(&[0f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rect globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rect globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rects"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rects"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Rect>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 1024;
        let instances = Self::alloc(device, capacity);
        Self {
            pipeline,
            globals,
            bind_group,
            instances,
            capacity,
            back_count: 0,
            over_count: 0,
        }
    }

    fn alloc(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect instances"),
            size: (capacity * std::mem::size_of::<Rect>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: [f32; 2],
        back: &[Rect],
        over: &[Rect],
    ) {
        queue.write_buffer(
            &self.globals,
            0,
            bytemuck::cast_slice(&[screen[0], screen[1], 0.0, 0.0]),
        );
        let total = back.len() + over.len();
        if total > self.capacity {
            self.capacity = total.next_power_of_two();
            self.instances = Self::alloc(device, self.capacity);
        }
        let mut all = Vec::with_capacity(total);
        all.extend_from_slice(back);
        all.extend_from_slice(over);
        if !all.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&all));
        }
        self.back_count = u32::try_from(back.len()).unwrap_or(u32::MAX);
        self.over_count = u32::try_from(over.len()).unwrap_or(u32::MAX);
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, first: u32, count: u32) {
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, first..first + count);
    }

    pub(crate) fn draw_background(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw(pass, 0, self.back_count);
    }

    pub(crate) fn draw_overlay(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw(pass, self.back_count, self.over_count);
    }
}
