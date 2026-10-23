//! FilmCraft for Android.
//!
//! A thin host, like `apps/filmcraft` (desktop) and `apps/filmcraft-web`: it owns the window, the
//! audio device and the file pickers, and hands everything else to `filmcraft-ui-egui` through
//! `HostHooks`. Android-specific pieces:
//! - `bridge`: JNI calls to `MainActivity.kt` (Storage Access Framework pickers, links) and the
//!   callback that delivers the user's choice back to the frame loop.
//! - `touch`: finger-sized controls and a phone workspace (Program monitor + Timeline).
//! - `audio` / `audio_in`: the desktop app's cpal code, reused as-is (cpal drives Oboe on Android).
#![cfg(target_os = "android")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

mod bridge;
mod touch;

#[path = "../../filmcraft/src/audio.rs"]
mod audio;
#[path = "../../filmcraft/src/audio_in.rs"]
mod audio_in;

use android_activity::AndroidApp;
use chrono::TimeZone;
use filmcraft_engine::Session;
use filmcraft_engine::autosave::AutosaveConfig;
use filmcraft_ui_egui::FilmcraftApp;
use std::path::PathBuf;

type AppResult = Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>>;

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_tag("FilmCraft").with_max_level(log::LevelFilter::Info));
    log::info!("FilmCraft {} (android {})", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH);
    let data_dir = app.internal_data_path();
    filmcraft_ui_egui::crash::install(data_dir.as_ref().map(|d| d.join("Logs")));
    // No OS hardware decoder on Android yet (MediaCodec is phase 2): our own decoders are used.
    let _ = filmcraft_platform::register();
    filmcraft_export::register_frame_renderer(|| {
        filmcraft_gpu::ExportRenderer::new().map(|r| Box::new(GpuFrameRenderer(r)) as Box<dyn filmcraft_export::FrameRenderer>)
    });

    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    if let Err(e) = eframe::run_native("FilmCraft", options, Box::new(move |cc| create_app(cc, data_dir))) {
        let msg = format!("FilmCraft could not start its window: {e}");
        log::error!("{msg}");
        filmcraft_ui_egui::crash::record(&msg);
    }
}

fn create_app(cc: &eframe::CreationContext<'_>, data_dir: Option<PathBuf>) -> AppResult {
    bridge::set_context(cc.egui_ctx.clone());
    touch::install(&cc.egui_ctx);

    let mut session = Session::default();
    if let Some(dir) = data_dir {
        let mut cfg = AutosaveConfig::new(dir);
        cfg.local_offset = local_offset;
        if let Err(e) = session.start_autosave(cfg) {
            log::warn!("auto-save and crash recovery unavailable: {e}");
        }
    }
    // voice-over recording reads the microphone through cpal (needs the RECORD_AUDIO permission)
    session.voiceover.input = Some(Box::new(audio_in::CpalIn::new(&session.prefs.audio_hardware.device_class)));

    let mut app = FilmcraftApp::new(session);
    app.ui.dock = touch::phone_layout();
    app.integrated_titlebar = false;
    if let Some(rs) = cc.wgpu_render_state.clone() {
        app.set_wgpu(rs);
    }
    app.audio = Some(Box::new(audio::CpalOut::new()));

    // Every picker goes through MainActivity (Storage Access Framework) and answers on a channel,
    // so the frame loop never blocks while the system picker is up.
    app.hooks.run_dialog = Some(Box::new(bridge::run_dialog));
    app.hooks.open_url = Some(Box::new(bridge::open_url));
    app.hooks.system_languages = Some(Box::new(|| sys_locale::get_locales().collect()));
    // `open_path` (Edit Original, Reveal Log Files) and `clipboard_media` are desktop notions: left unset.
    Ok(Box::new(app))
}

/// Local UTC offset at a unix time (auto-save file names and recovery times use local time).
fn local_offset(unix: i64) -> i32 {
    chrono::Local.timestamp_opt(unix, 0).single().map(|d| d.offset().local_minus_utc()).unwrap_or(0)
}

/// The GPU export renderer, as in the desktop app.
struct GpuFrameRenderer(filmcraft_gpu::ExportRenderer);

impl filmcraft_export::FrameRenderer for GpuFrameRenderer {
    fn render(
        &mut self,
        project: &filmcraft_project::Project,
        seq: filmcraft_project::ItemId,
        t: filmcraft_time::Tick,
        opts: filmcraft_render::RenderOptions,
        sources: &dyn filmcraft_render::SourceProvider,
    ) -> Option<filmcraft_render::Image> {
        self.0.render(project, seq, t, opts, sources)
    }
}
