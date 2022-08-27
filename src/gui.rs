use crate::compositor::{dev::LogicalDevice, tex::GpuTexture, CompositeLayer};
use crate::silica::{ProcreateFile, SilicaGroup};
use crate::{
    compositor::Compositor,
    silica::{BlendingMode, SilicaHierarchy},
};
use egui_wgpu::renderer::{RenderPass, ScreenDescriptor};
use egui_winit_platform::{Platform, PlatformDescriptor};
use parking_lot::RwLock;
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use winit::{event::Event::*, event_loop::ControlFlow};

// fn gpu_render(
//     state: &Compositor,
//     gpu_textures: &[GpuTexture],
//     layers: &SilicaGroup,
// ) -> wgpu::TextureView {
//     // let output_buffer = state.handle.device.create_buffer(&wgpu::BufferDescriptor {
//     //     label: None,
//     //     size: (state.buffer_dimensions.padded_bytes_per_row * state.buffer_dimensions.height)
//     //         as u64,
//     //     usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
//     //     mapped_at_creation: false,
//     // });

//     // let result = if composite_reference {
//     //     state.render(&[CompositeLayer {
//     //         texture: &pc.composite.image,
//     //         clipped: None,
//     //         opacity: 1.0,
//     //         blend: BlendingMode::Normal,
//     //         name: Some("Composite"),
//     //     }])
//     // } else {
//     let result = state.render(&linearize(&state, gpu_textures, &layers));
//     // };

//     // state.handle.queue.submit(Some({
//     //     let mut encoder = state
//     //         .handle
//     //         .device
//     //         .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
//     //     // Copy the data from the texture to the buffer
//     //     encoder.copy_texture_to_buffer(
//     //         state.composite_texture.as_image_copy(),
//     //         wgpu::ImageCopyBuffer {
//     //             buffer: &output_buffer,
//     //             layout: wgpu::ImageDataLayout {
//     //                 offset: 0,
//     //                 bytes_per_row: NonZeroU32::new(state.buffer_dimensions.padded_bytes_per_row),
//     //                 rows_per_image: None,
//     //             },
//     //         },
//     //         state.texture_extent,
//     //     );

//     //     encoder.finish()
//     // }));

//     // let buffer_slice = output_buffer.slice(..);

//     // // NOTE: We have to create the mapping THEN device.poll() before await
//     // // the future. Otherwise the application will freeze.
//     // let (tx, rx) = futures::channel::oneshot::channel();
//     // buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
//     // state.handle.device.poll(wgpu::Maintain::Wait);
//     // block_on(rx).unwrap().unwrap();

//     // let data = buffer_slice.get_mapped_range();

//     // // eprintln!("Loading data to CPU");
//     // // let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
//     // //     state.buffer_dimensions.padded_bytes_per_row as u32 / 4,
//     // //     state.buffer_dimensions.height as u32,
//     // //     data,
//     // // )
//     // // .unwrap();
//     // // eprintln!("Writing image");

//     // eprintln!("Loading data to CPU");
//     // let mut buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
//     //     state.buffer_dimensions.padded_bytes_per_row as u32 / 4,
//     //     state.buffer_dimensions.height as u32,
//     //     data.to_vec(),
//     // )
//     // .unwrap();
//     // eprintln!("Rotating image");

//     // buffer = image::imageops::crop_imm(&buffer, 0, 0, pc.size.width, pc.size.height).to_image();
//     // match pc.orientation {
//     //     0 => {}
//     //     1 | 4 => buffer = image::imageops::rotate90(&buffer),
//     //     2 => buffer = image::imageops::rotate180(&buffer),
//     //     3 => buffer = image::imageops::rotate270(&buffer),
//     //     _ => println!("Unknown orientation!"),
//     // };
//     // eprintln!("Writing image");

//     // buffer.save(out_path).unwrap();

//     // eprintln!("Finished");
//     // drop(buffer);
//     // drop(buffer_slice);

//     // output_buffer.unmap();
//     result.make_view()
// }

fn linearize<'a>(
    gpu_textures: &'a [GpuTexture],
    layers: &crate::silica::SilicaGroup,
    composite_layers: &mut Vec<CompositeLayer<'a>>,
) {
    let mut mask_layer: Option<(usize, &crate::silica::SilicaLayer)> = None;

    for (index, layer) in layers.children.iter().rev().enumerate() {
        match layer {
            SilicaHierarchy::Group(group) if !group.hidden => {
                linearize(gpu_textures, group, composite_layers);
            }
            SilicaHierarchy::Layer(layer) if !layer.hidden => {
                if let Some((_, mask_layer)) = mask_layer {
                    if layer.clipped && mask_layer.hidden {
                        continue;
                    }
                }

                let gpu_texture = &gpu_textures[layer.image];

                composite_layers.push(CompositeLayer {
                    texture: gpu_texture,
                    clipped: layer.clipped.then(|| mask_layer.unwrap().0),
                    opacity: layer.opacity,
                    blend: layer.blend,
                });

                if !layer.clipped {
                    mask_layer = Some((index, layer));
                }
            }
            _ => continue,
        }
    }
}

fn layout_layers(ui: &mut egui::Ui, layers: &mut SilicaGroup, i: &mut usize) {
    for layer in &mut layers.children {
        *i += 1;
        match layer {
            SilicaHierarchy::Layer(l) => {
                ui.push_id(*i, |ui| {
                    *i += 1;
                    ui.collapsing(l.name.as_deref().unwrap_or(""), |ui| {
                        ui.checkbox(&mut l.hidden, "Hidden").changed();
                        egui::ComboBox::from_label("Blending Mode")
                            .selected_text(format!("{:?}", l.blend))
                            .show_ui(ui, |ui| {
                                for b in BlendingMode::all() {
                                    ui.selectable_value(&mut l.blend, *b, b.to_str());
                                }
                            });
                        ui.add(egui::Slider::new(&mut l.opacity, 0.0..=1.0).text("Opacity"));
                    });
                });
            }
            SilicaHierarchy::Group(h) => {
                ui.push_id(*i, |ui| {
                    *i += 1;
                    ui.collapsing(h.name.to_string().as_str(), |ui| {
                        ui.checkbox(&mut h.hidden, "Hidden").changed();
                        layout_layers(ui, h, i);
                    })
                });
            }
        }
    }
}

fn layout_gui(
    context: &egui::Context,
    show_grid: &mut bool,
    egui_tex: egui::TextureId,
    state: &mut Compositor,
    pc: &mut ProcreateFile,
) {
    use egui::*;
    SidePanel::new(panel::Side::Right, "Side Panel")
        .default_width(300.0)
        .show(&context, |ui| {
            Frame::group(&Style::default()).show(ui, |ui| {
                ui.collapsing("File", |ui| {
                    ui.separator();

                    Grid::new("File Grid")
                        .num_columns(2)
                        .spacing([8.0, 10.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Name");
                            ui.label(pc.name.as_deref().unwrap_or("Not Specified"));
                            ui.end_row();
                            ui.label("Author");
                            ui.label(pc.author_name.as_deref().unwrap_or("Not Specified"));
                            ui.end_row();
                            ui.label("Stroke Count");
                            ui.label(pc.stroke_count.to_string());
                            ui.end_row();
                            ui.label("Canvas Size");
                            ui.label(format!("{} by {}", pc.size.width, pc.size.height));
                            ui.allocate_space(egui::vec2(ui.available_width(), 0.0))
                        });
                    ui.allocate_space(egui::vec2(ui.available_width(), 0.0))
                });
                ui.allocate_space(egui::vec2(ui.available_width(), 0.0))
            });

            Frame::group(&Style::default()).show(ui, |ui| {
                ui.collapsing("View Control", |ui| {
                    ui.separator();
                    if ui.button("Toggle Grid").clicked() {
                        *show_grid = !*show_grid;
                    }
                    ui.separator();

                    egui::Grid::new("Control Grid")
                        .num_columns(2)
                        .spacing([8.0, 10.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Flip");
                            ui.horizontal(|ui| {
                                if ui.button("Horizontal").clicked() {
                                    state.flip_vertices((true, false));
                                }
                                if ui.button("Vertical").clicked() {
                                    state.flip_vertices((false, true));
                                }
                            });
                            ui.end_row();
                            ui.label("Rotate");
                            ui.horizontal(|ui| {
                                if ui.button("CCW").clicked() {
                                    state.rotate_vertices(true);
                                    state.set_dimensions(state.dim.height, state.dim.width);
                                }
                                if ui.button("CW").clicked() {
                                    state.rotate_vertices(false);
                                    state.set_dimensions(state.dim.height, state.dim.width);
                                }
                            });
                            ui.allocate_space(egui::vec2(ui.available_width(), 0.0))
                        });
                    // ui.allocate_space(egui::vec2(ui.available_width(), 0.0))
                });
                ui.allocate_space(vec2(ui.available_width(), 0.0))
            });

            let mut i = 0;
            Frame::group(&Style::default()).show(ui, |ui| {
                ui.label("Layers");
                ui.separator();
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        layout_layers(ui, &mut pc.layers, &mut i);
                    });
                ui.allocate_space(vec2(ui.available_width(), 0.0))
            })
        });

    CentralPanel::default()
        .frame(Frame::none())
        .show(&context, |ui| {
            let mut plot = plot::Plot::new("Image View").data_aspect(1.0);

            if *show_grid {
                plot = plot.show_x(false).show_y(false).show_axes([false, false]);
            }

            plot.show(ui, |plot_ui| {
                let size = state.dim;
                plot_ui.image(plot::PlotImage::new(
                    egui_tex,
                    plot::PlotPoint { x: 0.0, y: 0.0 },
                    (size.width as f32, size.height as f32),
                ))
            });
        });
}

pub fn start_gui(
    (pc, gpu_textures): (ProcreateFile, Vec<GpuTexture>),
    dev: LogicalDevice,
    window: winit::window::Window,
    event_loop: winit::event_loop::EventLoop<()>,
) {
    let surface = unsafe { dev.instance.create_surface(&window) };

    let dev = &*Box::leak(Box::new(dev));

    let compositor = Arc::new(RwLock::new({
        let mut state =
            Compositor::new((!pc.background_hidden).then_some(pc.background_color), dev);
        state.flip_vertices((pc.flipped.horizontally, pc.flipped.vertically));
        state.set_dimensions(pc.size.width, pc.size.height);
        state
    }));

    let tex = Arc::new(RwLock::new(
        compositor.read().base_composite_texture().make_view(),
    ));

    let size = window.inner_size();
    let surface_format = surface.get_supported_formats(&dev.adapter)[0];
    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
    };
    surface.configure(&dev.device, &surface_config);

    // We use the egui_winit_platform crate as the platform.
    let mut platform = Platform::new(PlatformDescriptor {
        physical_width: size.width,
        physical_height: size.height,
        scale_factor: window.scale_factor(),
        font_definitions: egui::FontDefinitions::default(),
        style: Default::default(),
    });

    let pc = Arc::new(RwLock::new(pc));
    let running = Arc::new(AtomicBool::new(true));

    // We use the egui_wgpu_backend crate as the render backend.
    let mut egui_rpass = RenderPass::new(&dev.device, surface_format, 1);

    let mut egui_tex =
        egui_rpass.register_native_texture(&dev.device, &tex.read(), wgpu::FilterMode::Linear);

    std::thread::spawn({
        let compositor = compositor.clone();
        let tex = tex.clone();
        let running = running.clone();
        let pc = pc.clone();
        move || {
            let mut resolved_layers = Vec::new();
            while running.load(std::sync::atomic::Ordering::SeqCst) {
                let gpu_textures = &gpu_textures;
                resolved_layers.clear();
                linearize(
                    gpu_textures,
                    &pc.read().layers.clone(),
                    &mut resolved_layers,
                );
                *tex.write() = compositor.read().render(&resolved_layers).make_view();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    });

    let mut show_grid = true;

    let start_time = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        // Pass the winit events to the platform integration.
        platform.handle_event(&event);

        match event {
            RedrawRequested(..) => {
                platform.update_time(start_time.elapsed().as_secs_f64());

                let output_frame = match surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(wgpu::SurfaceError::Outdated) => {
                        // This error occurs when the app is minimized on Windows.
                        // Silently return here to prevent spamming the console with:
                        // "The underlying surface has changed, and therefore the swap chain must be updated"
                        return;
                    }
                    Err(e) => {
                        eprintln!("Dropped frame with error: {}", e);
                        return;
                    }
                };
                let output_view = output_frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Begin to draw the UI frame.
                platform.begin_frame();

                let context = platform.context();

                layout_gui(
                    &context,
                    &mut show_grid,
                    egui_tex,
                    &mut compositor.write(),
                    &mut pc.write(),
                );

                let full_output = platform.end_frame(Some(&window));

                let paint_jobs = context.tessellate(full_output.shapes);

                let mut encoder =
                    dev.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("encoder"),
                        });

                // Upload all resources for the GPU.
                let screen_descriptor = ScreenDescriptor {
                    size_in_pixels: [surface_config.width, surface_config.height],
                    pixels_per_point: window.scale_factor() as f32,
                };
                let tdelta: egui::TexturesDelta = full_output.textures_delta;

                for (id, image_delta) in &tdelta.set {
                    egui_rpass.update_texture(&dev.device, &dev.queue, *id, image_delta);
                }
                for id in &tdelta.free {
                    egui_rpass.free_texture(id);
                }
                egui_rpass.update_buffers(&dev.device, &dev.queue, &paint_jobs, &screen_descriptor);

                // Record all render passes.
                egui_rpass.execute(
                    &mut encoder,
                    &output_view,
                    &paint_jobs,
                    &screen_descriptor,
                    Some(wgpu::Color::BLACK),
                );
                // Submit the commands.
                dev.queue.submit(Some(encoder.finish()));

                // Redraw egui
                output_frame.present();

                tdelta.free.iter().for_each(|z| egui_rpass.free_texture(z));

                if let Some(z) = tex.try_read() {
                    egui_rpass.free_texture(&egui_tex);
                    egui_tex = egui_rpass.register_native_texture(
                        &dev.device,
                        &z,
                        wgpu::FilterMode::Linear,
                    );
                }
            }
            MainEventsCleared => {
                window.request_redraw();
            }
            WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        surface_config.width = size.width;
                        surface_config.height = size.height;
                        surface.configure(&dev.device, &surface_config);
                    }
                }
                winit::event::WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    running.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                _ => {}
            },
            _ => (),
        }
    });
}