use rand::Rng;
use wgpu::util::DeviceExt;
use wgpu::{BufferUsages, Extent3d, Features, TextureUsages};

use winit::event::{ElementState, KeyboardInput, VirtualKeyCode};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

const NUM_SLIMES: u32 = 1024 * 1024;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Slime {
    pos: [f32; 2],
    heading: f32,
    species: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SlimeMoveConfig {
    width: u32,
    height: u32,
    num_slimes: u32,
    delta_time: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WorldUpdateConfig {
    width: u32,
    height: u32,
    delta_time: f32,
    random: f32,
}

impl Slime {
    fn new_swarm(size: usize) -> Vec<Slime> {
        let mut swarm = Vec::with_capacity(size);
        let mut rng = rand::thread_rng();
        for _ in 0..size {
            let r = rng.gen_range(0.1..0.2);
            swarm.push(Slime {
                pos: [
                    rng.gen_range(0.0..std::f32::consts::PI * 2.0).cos() * r + 0.5,
                    rng.gen_range(0.0..std::f32::consts::PI * 2.0).sin() * r + 0.5,
                ],
                heading: rng.gen_range(0.0..std::f32::consts::PI * 2.0),
                // heading: 0.0,
                species: 0,
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
        present_mode: wgpu::PresentMode::Mailbox,
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

    // Create two textures, (custom swapchain)
    let img = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: TextureUsages::STORAGE_BINDING
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST,
    });
    let img_view = img.create_view(&Default::default());

    let next_img = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: TextureUsages::STORAGE_BINDING
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
    });
    let next_img_view = next_img.create_view(&Default::default());

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
                resource: wgpu::BindingResource::TextureView(&img_view),
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
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
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
                resource: wgpu::BindingResource::TextureView(&img_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&next_img_view),
            },
        ],
    });
    let inverted_world_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &world_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&next_img_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&img_view),
            },
        ],
    });

    // ========== Slime Movement Shader ========== //

    let mut slime_move_params = SlimeMoveConfig {
        width: size.width,
        height: size.height,
        num_slimes: NUM_SLIMES,
        delta_time: 0.0,
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

    let slime_move_cs_module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/move_slimes.wgsl").into()),
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
            bind_group_layouts: &[&slime_move_bind_group_layout, &world_bind_group_layout],
            push_constant_ranges: &[],
        });
    let slime_move_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&slime_move_compute_pipeline_layout),
        module: &slime_move_cs_module,
        entry_point: "main",
    });
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
    let mut world_update_params = WorldUpdateConfig {
        width: size.width,
        height: size.height,
        delta_time: 0.0,
        random: rand::thread_rng().gen_range(0.0..1.0),
    };

    let world_update_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
        contents: bytemuck::cast_slice(&[world_update_params]),
    });

    let world_update_cs_module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/update_world.wgsl").into()),
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
            bind_group_layouts: &[&world_update_bind_group_layout, &world_bind_group_layout],
            push_constant_ranges: &[],
        });
    let world_update_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&world_update_compute_pipeline_layout),
        module: &world_update_cs_module,
        entry_point: "main",
    });
    let world_update_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &world_update_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: world_update_params_buffer.as_entire_binding(),
        }],
    });

    let start_time = std::time::Instant::now();
    let mut last_frame_time = std::time::Instant::now();
    let mut rng = rand::thread_rng();

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

                slime_move_params = SlimeMoveConfig {
                    width: size.width,
                    height: size.height,
                    num_slimes: NUM_SLIMES,
                    delta_time: delta_time.as_secs_f32(),
                };
                world_update_params = WorldUpdateConfig {
                    width: size.width,
                    height: size.height,
                    delta_time: delta_time.as_secs_f32(),
                    random: rng.gen_range(0.0..1.0),
                };

                queue.write_buffer(
                    &slime_move_params_buffer,
                    0,
                    bytemuck::cast_slice(&[slime_move_params]),
                );
                queue.write_buffer(
                    &world_update_params_buffer,
                    0,
                    bytemuck::cast_slice(&[world_update_params]),
                );

                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut cpass = encoder.begin_compute_pass(&Default::default());
                    cpass.set_pipeline(&slime_move_pipeline);
                    cpass.set_bind_group(0, &slime_move_bind_group, &[]);
                    cpass.set_bind_group(1, &inverted_world_bind_group, &[]);
                    cpass.dispatch(NUM_SLIMES / 64, 1, 1);
                }
                {
                    let mut cpass = encoder.begin_compute_pass(&Default::default());
                    cpass.set_pipeline(&world_update_pipeline);
                    cpass.set_bind_group(0, &world_update_bind_group, &[]);
                    cpass.set_bind_group(1, &world_bind_group, &[]);
                    cpass.dispatch(size.width / 16, size.height / 16, 1);
                }
                encoder.copy_texture_to_texture(
                    wgpu::ImageCopyTexture {
                        texture: &next_img,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::ImageCopyTexture {
                        texture: &img,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: size.width,
                        height: size.height,
                        depth_or_array_layers: 1,
                    },
                );
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
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
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
            _ => (),
        }
    });
}

fn main() {
    let event_loop = EventLoop::new();
    let window = winit::window::WindowBuilder::new()
        .with_title("GPU Slime Mould")
        .with_inner_size(winit::dpi::LogicalSize::new(1024, 1024))
        .build(&event_loop)
        .unwrap();
    pollster::block_on(run(event_loop, window));
}
