use std::num::NonZeroU32;
use std::sync::mpsc::channel;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{watcher, RecursiveMode, Watcher};
use rand::Rng;
use wgpu::util::DeviceExt;
use wgpu::{BufferUsages, ComputePipeline, Extent3d, Features, TextureUsages};

use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use workerpool::thunk::{Thunk, ThunkWorker};
use workerpool::Pool;

const NUM_SLIMES: u32 = 1024 * 1024 * 3; // 1024 * 1024 * 2 is MAX. Computer will crash after that
#[allow(dead_code)]
const WINDOW_SIZE: (u32, u32) = (1600, 900);
// const WINDOW_SIZE: (u32, u32) = ((2560.0 * 0.6) as u32, (1440.0 * 0.6) as u32);
// const WORLD_SIZE: (u32, u32) = (1088, 2176); // Georg Phone (1284 x 2778)
// const WORLD_SIZE: (u32, u32) = (1280, 2776); // My Phone (1284 x 2778)
// const WORLD_SIZE: (u32, u32) = ((2560.0 * 1.5) as u32, (1440.0 * 1.5) as u32); // My Monitor
const WORLD_SIZE: (u32, u32) = (3200, 1800);
const FLOATS_PER_PIXEL: u32 = 4;
const VID_N_SKIP_FRAMES: u128 = 6;
const BEGIN_WITH_RECORDING: bool = false;

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Slime {
    pos: [f32; 2],
    heading: f32,
    species: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct StaticGlobalParams {
    width: u32,
    height: u32,
    num_slimes: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SlimeMoveConfig {
    delta_time: f32,
    random: f32,
    move_to_center: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WorldUpdateConfig {
    delta_time: f32,
}

enum RecordingState {
    Off,
    On(u128, usize),
}

impl Slime {
    fn new_swarm(size: usize) -> Vec<Slime> {
        let mut swarm = Vec::with_capacity(size);
        let mut rng = rand::thread_rng();
        for _ in 0..size {
            let r = rng.gen_range(0.0..10.0);
            let angle = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
            #[allow(unused)]
            let in_circle = [
                angle.cos() * r + (WORLD_SIZE.0 / 2) as f32,
                angle.sin() * r + (WORLD_SIZE.1 / 2) as f32,
            ];
            #[allow(unused)]
            let in_world = [
                rng.gen_range(0.0..WORLD_SIZE.0 as f32),
                rng.gen_range(0.0..WORLD_SIZE.1 as f32),
            ];
            swarm.push(Slime {
                pos: in_circle,
                heading: rng.gen_range(0.0..std::f32::consts::PI * 2.0),
                // heading: 0.0,
                species: rng.gen_range(0..2),
            });
        }
        swarm
    }
}

async fn run(event_loop: EventLoop<()>, window: Window) {
    // ============ Adapter, Device and Surface Creation ============== //

    let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
    let surface = unsafe { instance.create_surface(&window) };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("error finding adapter");

    let adapter_features = adapter.features();

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: adapter_features | Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                limits: Default::default(),
            },
            None,
        )
        .await
        .expect("error creating device");
    let size = window.inner_size();

    let format = surface.get_preferred_format(&adapter).unwrap();
    let sc = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
    };
    surface.configure(&device, &sc);

    // ============ Create Render Pipeline ============== //

    // We use a render pipeline just to copy the output buffer of the compute shader to the
    // swapchain. It would be nice if we could skip this, but swapchains with storage usage
    // are not fully portable.
    let copy_shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/shader.wgsl").into()),
    });
    let copy_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        // Should filterable be false if we want nearest-neighbor?
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
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
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&copy_bind_group_layout],
        push_constant_ranges: &[],
    });
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &copy_shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &copy_shader,
            entry_point: "fs_main",
            targets: &[format.into()],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
    });

    // Buffer to copy the render texture to output to file
    let world_texture_copy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (WORLD_SIZE.0 * WORLD_SIZE.1 * 4) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Create two buffers for the world data, and a texture to render, (custom swapchain)
    let init_world_data = vec![0.0f32; (WORLD_SIZE.0 * WORLD_SIZE.1 * FLOATS_PER_PIXEL) as usize];

    let world_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: Extent3d {
            width: WORLD_SIZE.0,
            height: WORLD_SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::COPY_SRC
            | TextureUsages::STORAGE_BINDING,
    });
    let world_texture_view = world_texture.create_view(&Default::default());

    let current_world_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&init_world_data),
        usage: wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::STORAGE,
    });

    let next_world_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&init_world_data),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let copy_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &copy_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&world_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    // === World Map Swap Chain === //
    let world_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

    let world_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &world_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: current_world_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: next_world_buffer.as_entire_binding(),
            },
        ],
    });
    let inverted_world_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &world_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: next_world_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: current_world_buffer.as_entire_binding(),
            },
        ],
    });

    // ========== Static Global Params Bind Group ====== //
    let static_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        usage: wgpu::BufferUsages::UNIFORM,
        contents: bytemuck::cast_slice(&[StaticGlobalParams {
            width: WORLD_SIZE.0,
            height: WORLD_SIZE.1,
            num_slimes: NUM_SLIMES,
        }]),
    });

    let static_params_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: None,
                    ty: wgpu::BufferBindingType::Uniform,
                },
            }],
        });

    let static_params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &&static_params_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: static_params_buffer.as_entire_binding(),
        }],
    });

    // ========== Slime Movement Shader ========== //

    let slime_move_params = SlimeMoveConfig {
        delta_time: 0.0,
        random: 0.0,
        move_to_center: 0,
    };

    let slimes = Slime::new_swarm(NUM_SLIMES as usize);

    let slimes_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        usage: BufferUsages::COPY_DST | BufferUsages::STORAGE,
        contents: bytemuck::cast_slice(&slimes),
    });

    let slime_move_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        usage: BufferUsages::COPY_DST | BufferUsages::STORAGE | BufferUsages::UNIFORM,
        contents: bytemuck::cast_slice(&[slime_move_params]),
    });

    let slime_move_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

    let slime_move_compute_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                &slime_move_bind_group_layout,
                &world_bind_group_layout,
                &static_params_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });
    let mut slime_move_pipeline = load_pipeline(
        "src/shaders/move_slimes.wgsl",
        &device,
        &slime_move_compute_pipeline_layout,
    )
    .unwrap();
    let slime_move_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &slime_move_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: slime_move_params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: slimes_buffer.as_entire_binding(),
            },
        ],
    });

    // ========== World Processing Shader ============ //
    let world_update_params = WorldUpdateConfig { delta_time: 0.0 };

    let world_update_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
        contents: bytemuck::cast_slice(&[world_update_params]),
    });

    let world_update_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
    let world_update_compute_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                &world_update_bind_group_layout,
                &world_bind_group_layout,
                &static_params_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });
    let mut world_update_pipeline = load_pipeline(
        "src/shaders/update_world.wgsl",
        &device,
        &world_update_compute_pipeline_layout,
    )
    .unwrap();

    let world_update_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &world_update_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: world_update_params_buffer.as_entire_binding(),
        }],
    });

    // ================== BUFFER TO TEXTURE COMPUTE SHADER ================== //

    let buf_to_tex_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: TEXTURE_FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
    let buf_to_tex_compute_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                &buf_to_tex_bind_group_layout,
                &static_params_bind_group_layout,
                &slime_move_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });
    let mut buf_to_tex_pipeline = load_pipeline(
        "src/shaders/world_to_tex.wgsl",
        &device,
        &buf_to_tex_compute_pipeline_layout,
    )
    .unwrap();
    let buf_to_tex_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &buf_to_tex_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: next_world_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&world_texture_view),
            },
        ],
    });

    // let start_time = std::time::Instant::now();
    let mut last_frame_time = std::time::Instant::now();
    let mut rng = rand::thread_rng();

    // Watch files in dir:

    let (file_tx, file_rx) = channel();
    let mut watcher = watcher(file_tx, Duration::from_millis(200)).unwrap();
    watcher
        .watch("./src/shaders/", RecursiveMode::Recursive)
        .unwrap();

    let save_img_pool = Pool::<ThunkWorker<()>>::new(128);

    let mut recording = if BEGIN_WITH_RECORDING {
        start_recording()
    } else {
        RecordingState::Off
    };
    let mut frame_counter: u128 = 0;

    let mut moving_to_center = 0;

    event_loop.run(move |event, _, control_flow| {
        // TODO: this may be excessive polling. It really should be synchronized with
        // swapchain presentation, but that's currently underbaked in wgpu.
        *control_flow = ControlFlow::Poll;
        match event {
            Event::RedrawRequested(_) => {
                let frame = surface
                    .get_current_texture()
                    .expect("error getting texture from swap chain");

                let delta_time = last_frame_time.elapsed();
                last_frame_time = std::time::Instant::now();

                // ----- Update Uniforms ----- //
                queue.write_buffer(
                    &slime_move_params_buffer,
                    0,
                    bytemuck::cast_slice(&[SlimeMoveConfig {
                        delta_time: delta_time.as_secs_f32(),
                        random: rng.gen_range(0.0..1.0),
                        move_to_center: moving_to_center,
                    }]),
                );
                queue.write_buffer(
                    &world_update_params_buffer,
                    0,
                    bytemuck::cast_slice(&[WorldUpdateConfig {
                        delta_time: delta_time.as_secs_f32(),
                    }]),
                );

                // ----- Run Compute Pipelines ----- //
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut cpass = encoder.begin_compute_pass(&Default::default());
                    cpass.set_pipeline(&slime_move_pipeline);
                    cpass.set_bind_group(0, &slime_move_bind_group, &[]);
                    cpass.set_bind_group(1, &world_bind_group, &[]);
                    cpass.set_bind_group(2, &static_params_bind_group, &[]);
                    cpass.dispatch(NUM_SLIMES / 64, 1, 1);
                }
                {
                    let mut cpass = encoder.begin_compute_pass(&Default::default());
                    cpass.set_pipeline(&world_update_pipeline);
                    cpass.set_bind_group(0, &world_update_bind_group, &[]);
                    cpass.set_bind_group(1, &inverted_world_bind_group, &[]);
                    cpass.set_bind_group(2, &static_params_bind_group, &[]);
                    cpass.dispatch(WORLD_SIZE.0 / 8, WORLD_SIZE.1 / 8, 1);
                }
                {
                    let mut cpass = encoder.begin_compute_pass(&Default::default());
                    cpass.set_pipeline(&buf_to_tex_pipeline);
                    cpass.set_bind_group(0, &buf_to_tex_bind_group, &[]);
                    cpass.set_bind_group(1, &static_params_bind_group, &[]);
                    cpass.set_bind_group(2, &slime_move_bind_group, &[]);
                    cpass.dispatch(WORLD_SIZE.0 / 8, WORLD_SIZE.1 / 8, 1);
                }
                encoder.copy_buffer_to_buffer(
                    &current_world_buffer,
                    0,
                    &next_world_buffer,
                    0,
                    (WORLD_SIZE.0 * WORLD_SIZE.1 * 4 * FLOATS_PER_PIXEL).into(),
                );

                // ----- Render to Screen ----- //
                {
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(&render_pipeline);
                    rpass.set_bind_group(0, &copy_bind_group, &[]);
                    rpass.draw(0..3, 0..2);
                }
                queue.submit(Some(encoder.finish()));
                frame.present();
                frame_counter += 1;

                if frame_counter % VID_N_SKIP_FRAMES == 0 {
                    recording = match recording {
                        RecordingState::On(time, f_index) => {
                            let filepath = format!("videos/video-{}/image-{}.png", time, f_index);
                            save_image(
                                &device,
                                &world_texture,
                                &world_texture_copy_buffer,
                                &queue,
                                &save_img_pool,
                                filepath,
                            );
                            RecordingState::On(time, f_index + 1)
                        }
                        RecordingState::Off => RecordingState::Off,
                    };
                }

                if let Ok(update) = file_rx.try_recv() {
                    match update {
                        notify::DebouncedEvent::Write(path) => {
                            match path.file_name().unwrap().to_str().unwrap() {
                                name @ "move_slimes.wgsl" => {
                                    if let Some(p) = load_pipeline(
                                        "src/shaders/move_slimes.wgsl",
                                        &device,
                                        &slime_move_compute_pipeline_layout,
                                    ) {
                                        slime_move_pipeline = p;
                                        println!("Reloaded Shader: {}", name);
                                    }
                                }
                                name @ "update_world.wgsl" => {
                                    if let Some(p) = load_pipeline(
                                        "src/shaders/update_world.wgsl",
                                        &device,
                                        &world_update_compute_pipeline_layout,
                                    ) {
                                        world_update_pipeline = p;
                                        println!("Reloaded Shader: {}", name);
                                    }
                                }
                                name @ "world_to_tex.wgsl" => {
                                    if let Some(p) = load_pipeline(
                                        "src/shaders/world_to_tex.wgsl",
                                        &device,
                                        &buf_to_tex_compute_pipeline_layout,
                                    ) {
                                        buf_to_tex_pipeline = p;
                                        println!("Reloaded Shader: {}", name);
                                    }
                                }
                                _ => (),
                            }
                        }

                        _ => (),
                    }
                }
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::WindowEvent {
                event:
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                state: ElementState::Pressed,
                                ..
                            },
                        ..
                    },
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::WindowEvent {
                event: win_event, ..
            } => match win_event {
                WindowEvent::KeyboardInput { input, .. } => match input {
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::S),
                        ..
                    } => {
                        let start = SystemTime::now();
                        let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap();
                        let filepath = format!("images/image-{}.png", since_the_epoch.as_millis());
                        save_image(
                            &device,
                            &world_texture,
                            &world_texture_copy_buffer,
                            &queue,
                            &save_img_pool,
                            filepath,
                        );
                    }
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::R),
                        ..
                    } => {
                        recording = match recording {
                            RecordingState::Off => start_recording(),
                            RecordingState::On(_, _) => RecordingState::Off,
                        };
                    }
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::Space),
                        ..
                    } => {
                        queue.write_buffer(&slimes_buffer, 0, bytemuck::cast_slice(&slimes));
                        queue.write_buffer(
                            &current_world_buffer,
                            0,
                            bytemuck::cast_slice(&init_world_data),
                        );
                        queue.write_buffer(
                            &next_world_buffer,
                            0,
                            bytemuck::cast_slice(&init_world_data),
                        );
                        queue.submit(None);
                    }
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::C),
                        ..
                    } => {
                        moving_to_center = (moving_to_center + 1) % 2;
                    }
                    _ => (),
                },
                _ => (),
            },
            _ => (),
        }
    });
}

fn start_recording() -> RecordingState {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let dirpath = format!("videos/video-{}", now);
    std::fs::create_dir(dirpath).unwrap();
    RecordingState::On(now, 0)
}

fn save_image(
    device: &wgpu::Device,
    world_texture: &wgpu::Texture,
    world_texture_copy_buffer: &wgpu::Buffer,
    queue: &wgpu::Queue,
    save_img_pool: &Pool<ThunkWorker<()>>,
    filepath: String,
) {
    // Save Current World Texture;
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &world_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &world_texture_copy_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(4 * WORLD_SIZE.0),
                rows_per_image: NonZeroU32::new(WORLD_SIZE.1),
            },
        },
        wgpu::Extent3d {
            width: WORLD_SIZE.0,
            height: WORLD_SIZE.1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    // Download Buffer from GPU
    {
        let buffer_slice = world_texture_copy_buffer.slice(..);
        let mapping = buffer_slice.map_async(wgpu::MapMode::Read);
        device.poll(wgpu::Maintain::Wait);
        pollster::block_on(mapping).unwrap();
        let mut data = vec![0u8; (WORLD_SIZE.0 * WORLD_SIZE.1 * 4) as usize];
        data.copy_from_slice(&buffer_slice.get_mapped_range());

        let fp = filepath.clone();
        save_img_pool.execute(Thunk::of(move || {
            image::save_buffer_with_format(
                &fp,
                &data,
                WORLD_SIZE.0,
                WORLD_SIZE.1,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .unwrap();
            // println!("Image Saved: {}", &fp);
        }));
    }
    world_texture_copy_buffer.unmap();
}

fn main() {
    let event_loop = EventLoop::new();
    // let mut monitor = event_loop.available_monitors();
    let window = winit::window::WindowBuilder::new()
        .with_title("GPU Slime Mould")
        .with_resizable(false)
        // .with_movable_by_window_background(true)
        // .with_fullscreen(Some(winit::window::Fullscreen::Borderless(
        //     event_loop.primary_monitor(),
        // )))
        .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_SIZE.0, WINDOW_SIZE.1))
        .build(&event_loop)
        .unwrap();
    pollster::block_on(run(event_loop, window));
}

fn load_pipeline(
    path: &str,
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
) -> Option<ComputePipeline> {
    let shader_code = std::fs::read_to_string(path).unwrap();
    match naga::front::wgsl::parse_str(&shader_code) {
        Ok(_) => {
            let cs_module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(shader_code.into()),
            });

            Some(
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: None,
                    layout: Some(&pipeline_layout),
                    module: &cs_module,
                    entry_point: "main",
                }),
            )
        }
        Err(ref e) => {
            e.emit_to_stderr(&shader_code);
            None
        }
    }
}
