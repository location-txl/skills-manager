use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{Emitter, Manager};
use tauri_plugin_window_state::{StateFlags, WindowExt};

pub mod commands;
pub mod core;

/// Shared flag: when true, CloseRequested should NOT be prevented.
static QUITTING: AtomicBool = AtomicBool::new(false);

const MAIN_TRAY_ID: &str = "skills-manager-tray";

#[cfg(target_os = "macos")]
const CUSTOM_TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray/tray-icon-32.png");
#[cfg(not(target_os = "macos"))]
const CUSTOM_TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray/tray-icon-color-32.png");

fn load_custom_tray_icon() -> Option<tauri::image::Image<'static>> {
    let img = image::load_from_memory_with_format(CUSTOM_TRAY_ICON_BYTES, image::ImageFormat::Png)
        .ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(tauri::image::Image::new_owned(
        rgba.into_raw(),
        width,
        height,
    ))
}

fn restore_main_window(app: &tauri::AppHandle) {
    let app_for_main = app.clone();
    if let Err(err) = app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = app_for_main.set_dock_visibility(true) {
                log::error!("Failed to show Dock icon on macOS: {err}");
            }
            if let Err(err) =
                app_for_main.set_activation_policy(tauri::ActivationPolicy::Regular)
            {
                log::error!("Failed to set activation policy to Regular: {err}");
            }
            if let Err(err) = app_for_main.show() {
                log::error!("Failed to show app on macOS: {err}");
            }
        }

        if let Some(w) = app_for_main.get_webview_window("main") {
            if let Err(err) = w.show() {
                log::error!("Failed to show main window: {err}");
            }
            if let Err(err) = w.unminimize() {
                log::error!("Failed to unminimize main window: {err}");
            }
            if let Err(err) = w.set_focus() {
                log::error!("Failed to focus main window: {err}");
            }
        } else {
            log::error!("Main window not found while restoring from tray");
        }
    }) {
        log::error!("Failed to schedule restore_main_window on main thread: {err}");
    }
}

/// Quit the application. The window-state plugin persists geometry on window
/// destruction; nothing else needs teardown — the skills CLI owns all data.
pub fn quit_app(app: &tauri::AppHandle) {
    QUITTING.store(true, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.destroy();
    }
    app.exit(0);
}

fn request_quit(app: &tauri::AppHandle) {
    let app_for_main = app.clone();
    if let Err(err) = app.run_on_main_thread(move || {
        quit_app(&app_for_main);
    }) {
        log::error!("Failed to schedule quit on main thread: {err}");
    }
}

/// Tray: pure window operations only — no presets, no update checks, no
/// database-driven menus (those concepts are gone with the old store).
fn ensure_tray_icon(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    if app.tray_by_id(MAIN_TRAY_ID).is_some() {
        return Ok(());
    }

    let open = MenuItem::with_id(app, "show", "Open Skills Manager", true, None::<&str>)?;
    let folder = MenuItem::with_id(
        app,
        "open-skills-folder",
        "Open Skills Folder",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &folder, &quit])?;

    let mut builder = TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .tooltip("Skills Manager")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => restore_main_window(app),
            "quit" => request_quit(app),
            "open-skills-folder" => {
                let dir = core::scope::global_skills_dir_for(
                    &dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")),
                );
                if !dir.is_dir() {
                    log::debug!("Tray: global skills folder {} missing", dir.display());
                }
                if let Err(err) = core::file_manager::open_dir(&dir) {
                    log::error!("Tray: failed to open skills folder: {err}");
                }
            }
            _ => {}
        });

    if let Some(icon) = load_custom_tray_icon().or_else(|| app.default_window_icon().cloned()) {
        builder = builder.icon(icon);
    }

    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(false);
    }

    // On macOS, left-click on tray icon opens the menu by default;
    // on Windows/Linux, left-click restores the window directly.
    if !cfg!(target_os = "macos") {
        builder = builder
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    restore_main_window(tray.app_handle());
                }
            });
    }

    builder.build(app)?;
    log::debug!("Tray icon created");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let vercel_state = Arc::new(core::state::AppState::new());

    tauri::Builder::default()
        .manage(vercel_state.clone())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            restore_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    StateFlags::SIZE
                        | StateFlags::POSITION
                        | StateFlags::MAXIMIZED
                        | StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .setup(move |app| {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .level_for("tao", log::LevelFilter::Warn)
                    .level_for("wry", log::LevelFilter::Warn)
                    .level_for("hyper", log::LevelFilter::Warn)
                    .level_for("reqwest", log::LevelFilter::Warn)
                    .level_for("rustls", log::LevelFilter::Warn)
                    .max_file_size(5 * 1024 * 1024)
                    .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                    .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                    .format(|out, message, record| {
                        out.finish(format_args!(
                            "{} {:5} [{}] {}",
                            chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z"),
                            record.level(),
                            record.target(),
                            message
                        ))
                    })
                    .build(),
            )?;

            core::panic_log::install_panic_hook(app.handle().clone());

            core::state::refresh_watch_roots(&vercel_state);
            core::change_watcher::start_change_watcher(
                app.handle().clone(),
                vercel_state.watch_roots.clone(),
            );

            if let Err(err) = ensure_tray_icon(app.handle()) {
                log::error!("Failed to create tray icon: {err}");
            }

            // Intercept window close — the frontend decides (exit vs tray).
            let win = app.get_webview_window("main").expect("main window");
            let win_for_event = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if QUITTING.load(Ordering::SeqCst) {
                        return; // allow close
                    }
                    let _ = win_for_event.emit("window-close-requested", ());
                    api.prevent_close();
                }
            });
            // The window config starts hidden (`visible: false`) so restored
            // geometry applies before anything is shown.
            if let Err(err) = win.restore_state(
                StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN,
            ) {
                log::warn!("Failed to restore saved window state: {err}");
            }
            win.show()?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vercel::get_runtime_status,
            commands::vercel::set_cli_path,
            commands::vercel::get_last_cli_error,
            commands::vercel::list_skills,
            commands::vercel::read_skill_doc,
            commands::vercel::list_agent_status,
            commands::vercel::search_marketplace,
            commands::vercel::get_diagnostic_info,
            commands::vercel::check_last_panic,
            commands::vercel::clear_last_panic,
            commands::vercel::get_recent_log_excerpt,
            commands::vercel::restart_app,
            commands::vercel::open_project,
            commands::vercel::close_project,
            commands::vercel::reveal_skill_folder,
            commands::vercel::install_skill,
            commands::vercel::remove_skill,
            commands::vercel::update_skill,
            commands::vercel::share_skill,
            commands::vercel::cancel_operation,
            commands::vercel::app_exit,
            commands::vercel::hide_to_tray,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Clicking the Dock icon with no visible window restores it.
            if let tauri::RunEvent::Reopen { .. } = event {
                #[cfg(target_os = "macos")]
                {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        let _ = handle.set_dock_visibility(true);
                        let _ = handle.set_activation_policy(tauri::ActivationPolicy::Regular);
                    });
                }
                restore_main_window(app);
            }
        });
}
