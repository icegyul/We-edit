//! Shader modules, bind group layouts and render pipelines (created on first use).

use std::collections::HashMap;

const COMMON: &str = include_str!("shaders/common.wgsl");

/// Shader files and the bind group layout their fragment entry points use.
const MODULES: &[(&str, &str, Layout)] = &[
    (
        "composite",
        include_str!("shaders/composite.wgsl"),
        Layout::Main,
    ),
    ("color", include_str!("shaders/color.wgsl"), Layout::Main),
    ("blur", include_str!("shaders/blur.wgsl"), Layout::Main),
    (
        "distort",
        include_str!("shaders/distort.wgsl"),
        Layout::Main,
    ),
    ("keying", include_str!("shaders/keying.wgsl"), Layout::Main),
    ("looks", include_str!("shaders/looks.wgsl"), Layout::Main),
    ("art", include_str!("shaders/art.wgsl"), Layout::Main),
    (
        "stylize",
        include_str!("shaders/stylize.wgsl"),
        Layout::Main,
    ),
    (
        "transitions",
        include_str!("shaders/transitions.wgsl"),
        Layout::Main,
    ),
    ("output", include_str!("shaders/output.wgsl"), Layout::Main),
    (
        "convert",
        include_str!("shaders/convert.wgsl"),
        Layout::Convert,
    ),
    (
        "lumetri",
        include_str!("shaders/lumetri.wgsl"),
        Layout::Lumetri,
    ),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Layout {
    /// uniform, sampler, two float textures
    Main,
    /// uniform, sampler, four unsigned-integer plane textures
    Convert,
    /// uniform, sampler, image, curves, two 3D LUTs
    Lumetri,
}

pub const WORK: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DISPLAY: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Output format of an entry point.
pub fn target_format(entry: &str) -> wgpu::TextureFormat {
    match entry {
        "fs_present" | "fs_straight" | "fs_rgba8" => DISPLAY,
        "fs_luma8" => wgpu::TextureFormat::R8Unorm,
        "fs_chroma8" => wgpu::TextureFormat::Rg8Unorm,
        "fs_luma16" | "fs_alpha16" => wgpu::TextureFormat::R16Uint,
        "fs_chroma16" => wgpu::TextureFormat::Rg16Uint,
        _ => WORK,
    }
}

pub struct Pipelines {
    modules: HashMap<&'static str, (wgpu::ShaderModule, Layout)>,
    entries: HashMap<&'static str, &'static str>,
    layouts: HashMap<Layout, (wgpu::BindGroupLayout, wgpu::PipelineLayout)>,
    cache: HashMap<&'static str, wgpu::RenderPipeline>,
    pub sampler: wgpu::Sampler,
    pub scope_compute: wgpu::ComputePipeline,
    pub scope_draw: wgpu::RenderPipeline,
    pub scope_compute_layout: wgpu::BindGroupLayout,
    pub scope_draw_layout: wgpu::BindGroupLayout,
}

fn uniform_entry(
    binding: u32,
    dynamic: bool,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: wgpu::BufferSize::new(crate::params::SLOT),
        },
        count: None,
    }
}

fn texture_entry(
    binding: u32,
    sample: wgpu::TextureSampleType,
    dim: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: sample,
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    }
}

impl Pipelines {
    pub fn new(device: &wgpu::Device) -> Pipelines {
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let uint = wgpu::TextureSampleType::Uint;
        let d2 = wgpu::TextureViewDimension::D2;
        let d3 = wgpu::TextureViewDimension::D3;
        let sampler_entry = wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let frag = wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX;
        let make = |label: &str, entries: &[wgpu::BindGroupLayoutEntry]| {
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(&bgl)],
                immediate_size: 0,
            });
            (bgl, pl)
        };
        let mut layouts = HashMap::new();
        layouts.insert(
            Layout::Main,
            make(
                "main",
                &[
                    uniform_entry(0, true, frag),
                    sampler_entry,
                    texture_entry(2, float, d2),
                    texture_entry(3, float, d2),
                ],
            ),
        );
        layouts.insert(
            Layout::Convert,
            make(
                "convert",
                &[
                    uniform_entry(0, true, frag),
                    sampler_entry,
                    texture_entry(2, uint, d2),
                    texture_entry(3, uint, d2),
                    texture_entry(4, uint, d2),
                    texture_entry(5, uint, d2),
                ],
            ),
        );
        layouts.insert(
            Layout::Lumetri,
            make(
                "lumetri",
                &[
                    uniform_entry(0, true, frag),
                    sampler_entry,
                    texture_entry(2, float, d2),
                    texture_entry(3, float, d2),
                    texture_entry(4, float, d3),
                    texture_entry(5, float, d3),
                ],
            ),
        );

        let mut modules = HashMap::new();
        let mut entries = HashMap::new();
        for (name, src, layout) in MODULES {
            let code = format!("{COMMON}\n{src}");
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(name),
                source: wgpu::ShaderSource::Wgsl(code.into()),
            });
            for line in src.lines() {
                if let Some(rest) = line.trim().strip_prefix("fn fs_")
                    && let Some(end) = rest.find('(')
                {
                    let entry: &'static str =
                        Box::leak(format!("fs_{}", &rest[..end]).into_boxed_str());
                    entries.insert(entry, *name);
                }
            }
            modules.insert(*name, (module, *layout));
        }

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // scopes
        let scope_src = include_str!("shaders/scopes.wgsl");
        let scope_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scopes"),
            source: wgpu::ShaderSource::Wgsl(scope_src.into()),
        });
        let scope_uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let scope_compute_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scope compute"),
                entries: &[
                    scope_uniform(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: d2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
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
        let scope_draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope draw"),
            entries: &[
                scope_uniform(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let cpl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scope compute"),
            bind_group_layouts: &[Some(&scope_compute_layout)],
            immediate_size: 0,
        });
        let scope_compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("scope accumulate"),
            layout: Some(&cpl),
            module: &scope_module,
            entry_point: Some("cs_accumulate"),
            compilation_options: Default::default(),
            cache: None,
        });
        let dpl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scope draw"),
            bind_group_layouts: &[Some(&scope_draw_layout)],
            immediate_size: 0,
        });
        let scope_draw = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scope draw"),
            layout: Some(&dpl),
            vertex: wgpu::VertexState {
                module: &scope_module,
                entry_point: Some("vs_scope"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &scope_module,
                entry_point: Some("fs_scope"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: DISPLAY,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Pipelines {
            modules,
            entries,
            layouts,
            cache: HashMap::new(),
            sampler,
            scope_compute,
            scope_draw,
            scope_compute_layout,
            scope_draw_layout,
        }
    }

    pub fn layout_of(&self, entry: &str) -> Layout {
        let module = self
            .entries
            .get(entry)
            .unwrap_or_else(|| panic!("unknown shader entry {entry}"));
        self.modules[module].1
    }

    pub fn bind_layout(&self, layout: Layout) -> &wgpu::BindGroupLayout {
        &self.layouts[&layout].0
    }

    /// Every fragment entry point (for warming up and tests).
    pub fn entry_names(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.entries.keys().copied().collect();
        v.sort();
        v
    }

    pub fn get(&mut self, device: &wgpu::Device, entry: &'static str) -> &wgpu::RenderPipeline {
        if !self.cache.contains_key(entry) {
            let module_name = self
                .entries
                .get(entry)
                .unwrap_or_else(|| panic!("unknown shader entry {entry}"));
            let (module, layout) = &self.modules[module_name];
            let pl = &self.layouts[layout].1;
            let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(pl),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format(entry),
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
            self.cache.insert(entry, p);
        }
        &self.cache[entry]
    }
}
