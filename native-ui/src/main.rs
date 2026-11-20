#![windows_subsystem = "windows"]

mod app;
mod backend;
mod backend_native;
mod errors;
mod import;
mod media_access;
mod media_tools;
mod persistence;
mod platform;
mod shortcuts;
mod single_instance;
mod theme;
mod thumbnails;
mod view_model;
mod widgets;
mod views;

use std::sync::Arc;

use backend::{Backend, StubBackend};
use single_instance::{Acquisition, DEFAULT_PORT, SingleInstance};

fn main() -> eframe::Result<()> {
    // A second launch must not open a window or a database connection.
    let single_instance = match SingleInstance::acquire(DEFAULT_PORT) {
        Acquisition::First(instance) => Some(instance),
        Acquisition::AlreadyRunning => return Ok(()),
    };

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir);
    // Mirrors portable mode: store the cache beside the executable.
    let data_dir = if exe_dir.join("portable.txt").exists() {
        exe_dir.join("data")
    } else {
        exe_dir.join("prompt-studio-native-data")
    };
    let cache_dir = data_dir.join("cache");
    let _ = std::fs::create_dir_all(&cache_dir);
    let access = Arc::new(media_access::AllowedRoots::new(cache_dir));

    // Real in-process backend for media flows; fall back to the in-memory stub
    // only when on-disk state cannot be opened (see PORTING.md).
    let backend: Box<dyn Backend> = match backend_native::NativeBackend::open() {
        Ok(real) => Box::new(real),
        Err(error) => {
            eprintln!("[backend] native backend unavailable: {error}; using in-memory stub");
            Box::new(StubBackend::default())
        }
    };

    let mut options = eframe::NativeOptions {
        // Window geometry is stored in our own state (clamped); letting eframe
        // restore it too can place the window off a smaller monitor.
        persist_window: false,
        viewport: egui::ViewportBuilder::default()
            .with_app_id("prompt-studio-native")
            .with_title("Prompt Studio (native · port in progress)")
            .with_inner_size([1150.0, 540.0]),
        ..Default::default()
    };

    // egui-wgpu otherwise enables the GL backend, which loads nvoglv64.dll and
    // faults on this NVIDIA driver. DX12 avoids that path entirely, so use a
    // real GPU adapter here: WARP (DeviceType::Cpu) renders every frame on the
    // CPU and makes the whole UI feel slow. `PROMPT_STUDIO_SOFTWARE_GPU=1`
    // forces the software adapter for driver troubleshooting.
    let force_software = std::env::var("PROMPT_STUDIO_SOFTWARE_GPU").ok().as_deref() == Some("1");
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12;
    setup.power_preference = if force_software {
        eframe::wgpu::PowerPreference::LowPower
    } else {
        eframe::wgpu::PowerPreference::HighPerformance
    };
    setup.native_adapter_selector = Some(Arc::new(move |adapters, _surface| {
        let pick = |want_cpu: bool| {
            adapters
                .iter()
                .find(|adapter| {
                    let info = adapter.get_info();
                    let is_cpu = info.device_type == eframe::wgpu::DeviceType::Cpu;
                    is_cpu == want_cpu && info.device_type != eframe::wgpu::DeviceType::Other
                })
                .cloned()
        };
        if force_software {
            return pick(true)
                .or_else(|| adapters.first().cloned())
                .ok_or_else(|| "No DX12 adapter available".to_string());
        }
        // Hardware first (discrete, then integrated), software only as a last
        // resort so the app still starts on a machine without a DX12 GPU.
        adapters
            .iter()
            .find(|adapter| {
                matches!(
                    adapter.get_info().device_type,
                    eframe::wgpu::DeviceType::DiscreteGpu | eframe::wgpu::DeviceType::IntegratedGpu
                )
            })
            .cloned()
            .or_else(|| pick(true))
            .or_else(|| adapters.first().cloned())
            .ok_or_else(|| "No DX12 adapter available".to_string())
    }));
    options.wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);
    // Vsync on, low latency: without it DX12 can present in a free-running loop
    // and burn a whole core while the window sits idle.
    options.wgpu_options.surface = eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY;

    eframe::run_native(
        "Prompt Studio",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::NativeApp::new(
                cc,
                backend,
                access,
                single_instance,
            )))
        }),
    )
}
