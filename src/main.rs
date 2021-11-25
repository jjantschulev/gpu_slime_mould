use std::time::Instant;

use rand::Rng;
use wgpu::{
    include_wgsl, util::DeviceExt, Backends, BindGroupLayoutEntry, BufferUsages, DeviceDescriptor,
    Features, Instance, Limits, RequestAdapterOptions, SurfaceConfiguration, TextureUsages,
};
use winit::{
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    pollster::block_on(run());
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Slime {
    x: f32,
    y: f32,
    heading: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    time: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, -1.0],
        uv: [0.0, 0.0],
    },
    Vertex {
        position: [-1.0, 1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0],
        uv: [1.0, 0.0],
    },
];

const INDICIES: &[u16] = &[0, 2, 1, 0, 3, 2];

const NUM_SLIMES: u32 = 64;
const TEXTURE_DIMS: (u32, u32) = (64, 64);

async fn run() {
    let mut uniforms = Uniforms { time: 1.0 };
    let slimes = create_slimes(NUM_SLIMES);

    // Create Window:
    let events_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("GPU Slime Mould")
        // .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)))
        .with_resizable(false)
        .with_inner_size(winit::dpi::LogicalSize::new(1024, 1024))
        .build(&events_loop)
        .unwrap();

    let window_size = window.inner_size();

    // Initialize Gpu devices
    let instance = Instance::new(Backends::PRIMARY);
    let surface = unsafe { instance.create_surface(&window) };

    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .unwrap();

    println!("Selected Adapter: {}", adapter.get_info().name);

    // Create Device and Configure Surface
    let (device, queue) = adapter
        .request_device(
            &DeviceDescriptor {
                label: None,
                features: Features::empty(),
                limits: Limits::default(),
            },
            None,
        )
        .await
        .unwrap();

    let surface_config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: surface.get_preferred_format(&adapter).unwrap(),
        width: window_size.width,
        height: window_size.height,
        present_mode: wgpu::PresentMode::Fifo,
    };

    surface.configure(&device, &surface_config);

    // Create texure;
    let texture_bytes = create_random_texture(TEXTURE_DIMS);

    let texture_size = wgpu::Extent3d {
        width: TEXTURE_DIMS.0,
        height: TEXTURE_DIMS.1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Texture"),
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::STORAGE_BINDING,
    });
    let texture_data_layout = wgpu::ImageDataLayout {
        offset: 0,
        bytes_per_row: std::num::NonZeroU32::new(4 * TEXTURE_DIMS.0 as u32),
        rows_per_image: std::num::NonZeroU32::new(TEXTURE_DIMS.1 as u32),
    };
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texture_bytes,
        texture_data_layout,
        texture_size,
    );
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler {
                        filtering: false,
                        comparison: false,
                    },
                    count: None,
                },
            ],
        });

    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&texture_sampler),
            },
        ],
    });

    // Uniforms Bind Group Layout
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[uniforms]),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let uniform_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &uniform_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    // Create Render Pipeline
    let shader = device.create_shader_module(&include_wgsl!("./shaders/shader.wgsl"));

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline layout descriptor"),
        bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
        push_constant_ranges: &[],
    });

    // Create a vertex and index buffer

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(INDICIES),
        usage: wgpu::BufferUsages::INDEX,
    });

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(VERTICES),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let vertex_buffer_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
    };

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[vertex_buffer_layout],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            clamp_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[wgpu::ColorTargetState {
                format: surface_config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            }],
        }),
    });

    // Create Compute Pipeline:

    // Step 1: Run compute shader for all slimes, update their positions, and update current pixel buffer
    // Step 2: Run compute shader for all pixels in the world and blur and fade the scent.

    // Slime Buffer
    // Current Pixel Values
    // Next Pixel Values

    let slime_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Slimes Buffer"),
        contents: bytemuck::cast_slice(&slimes),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let world = create_random_texture(TEXTURE_DIMS);
    let world_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("World Buffer"),
        contents: bytemuck::cast_slice(&world),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let slime_buffer_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        has_dynamic_offset: false,
                        min_binding_size: None,
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        has_dynamic_offset: false,
                        min_binding_size: None,
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                    },
                    count: None,
                },
            ],
        });
    let slime_buffer_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &slime_buffer_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &slime_buffer,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &world_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    });

    let compute_shader = device.create_shader_module(&include_wgsl!("./shaders/compute.wgsl"));

    let compute_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&slime_buffer_bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Compute Pipeline"),
        layout: Some(&compute_pipeline_layout),
        module: &compute_shader,
        entry_point: "compute_main",
    });

    let start_time = Instant::now();

    // Run event loop
    events_loop.run(move |evt, _, control_flow| {
        match evt {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                    Some(VirtualKeyCode::Escape) => {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    _ => (),
                },
                _ => (),
            },
            Event::RedrawRequested(_) => {
                uniforms.time = (Instant::now() - start_time).as_secs_f32();
                queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

                // Render the scene
                let output = surface.get_current_texture().unwrap();
                let view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Command Encoder"),
                });

                // Run the compute shaders:
                let mut compute_pass =
                    encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });

                compute_pass.set_pipeline(&compute_pipeline);
                compute_pass.set_bind_group(0, &slime_buffer_bind_group, &[]);
                compute_pass.dispatch(NUM_SLIMES, 1, 1);

                drop(compute_pass);

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: None,
                });

                render_pass.set_pipeline(&render_pipeline);
                render_pass.set_bind_group(0, &texture_bind_group, &[]);
                render_pass.set_bind_group(1, &uniform_bind_group, &[]);
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.draw_indexed(0..INDICIES.len() as u32, 0, 0..1);

                drop(render_pass);

                queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
            Event::MainEventsCleared => {
                // std::thread::sleep(std::time::Duration::from_millis(500));
                window.request_redraw();
            }
            _ => (),
        }
    });
}

fn create_slimes(n: u32) -> Vec<Slime> {
    let start = Instant::now();
    let mut slimes = Vec::with_capacity(n as usize);
    let mut rng = rand::thread_rng();
    for _ in 0..n {
        slimes.push(Slime {
            x: rng.gen_range(0.0..1.0),
            y: rng.gen_range(0.0..1.0),
            heading: rng.gen_range(0.0..std::f32::consts::PI * 2.0),
        })
    }
    println!("Created {} slimes in {:?}", n, Instant::now() - start);
    slimes
}

fn create_random_texture((w, h): (u32, u32)) -> Vec<u8> {
    let start = Instant::now();
    let n = (4 * w * h) as usize;
    let mut bytes = vec![0; n];
    let mut rng = rand::thread_rng();
    for i in (0..n).step_by(4) {
        bytes[i + 0] = rng.gen_range(0..=255);
        bytes[i + 1] = rng.gen_range(0..=255);
        bytes[i + 2] = rng.gen_range(0..=255);
        bytes[i + 3] = 255;
    }
    println!("Created new random texture in {:?}", Instant::now() - start);
    bytes
}
