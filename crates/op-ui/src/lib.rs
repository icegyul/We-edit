//! User interface (egui). A thin layer over `op_application::Editor`: panels read the editor and
//! call its methods; the GPU compositor draws the monitors on the window's own device.

mod app;
mod dialogs;
mod effect_controls;
mod effects_panel;
pub mod i18n;
mod icons;
mod keys;
mod monitor;
mod panels;
mod project_panel;
mod theme;
mod timeline;
mod widgets;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;

pub use app::App;

/// Command-line options.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// A project or media files to open at start.
    pub open: Vec<PathBuf>,
    /// Keep all settings next to the program (portable mode).
    pub portable: Option<PathBuf>,
}

fn wgpu_setup(backends: wgpu::Backends) -> egui_wgpu::WgpuConfiguration {
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = backends;
    create.power_preference = wgpu::PowerPreference::HighPerformance;
    create.device_descriptor = Arc::new(|adapter: &wgpu::Adapter| wgpu::DeviceDescriptor {
        label: Some("OpenPremier"),
        required_features: op_render::gpu::optional_features(adapter),
        required_limits: op_render::gpu::limits(adapter),
        ..Default::default()
    });
    egui_wgpu::WgpuConfiguration {
        wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(create),
        ..Default::default()
    }
}

fn native_options(backends: wgpu::Backends) -> eframe::NativeOptions {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../assets/icons/openpremier-256.png"
    ))
    .ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(op_application::APP_NAME)
        .with_inner_size([1600.0, 940.0])
        .with_min_inner_size([960.0, 600.0])
        .with_app_id("io.github.openpremier.OpenPremier");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_setup(backends),
        persist_window: true,
        ..Default::default()
    }
}

/// Starts the application window.
pub fn run(opts: Options) -> Result<(), String> {
    let primary = wgpu::Backends::VULKAN | wgpu::Backends::DX12 | wgpu::Backends::METAL;
    let make = |opts: Options| -> eframe::AppCreator<'static> {
        Box::new(move |cc| {
            let app = App::new(cc, opts)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            Ok(Box::new(app))
        })
    };
    match eframe::run_native(
        op_application::APP_NAME,
        native_options(primary),
        make(opts.clone()),
    ) {
        Ok(()) => Ok(()),
        Err(e) => {
            // machines without Vulkan/Direct3D 12 still get a window through OpenGL
            log::warn!("starting with Vulkan/Direct3D 12 failed ({e}); trying OpenGL");
            eframe::run_native(
                op_application::APP_NAME,
                native_options(wgpu::Backends::GL),
                make(opts),
            )
            .map_err(|e| e.to_string())
        }
    }
}
