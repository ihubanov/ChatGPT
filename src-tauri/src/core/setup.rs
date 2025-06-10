use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{
    webview::DownloadEvent, App, LogicalPosition, Manager, PhysicalSize, WebviewBuilder,
    WebviewUrl, WindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::DialogExt; // Added for dialog
use tauri_plugin_shell::ShellExt;

#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;

use crate::core::{
    conf::AppConf,
    constant::{ASK_HEIGHT, INIT_SCRIPT, TITLEBAR_HEIGHT},
    template,
};

pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle();

    let conf = &AppConf::load(handle)?;
    let ask_mode_height = if conf.ask_mode { ASK_HEIGHT } else { 0.0 };

    template::Template::new(AppConf::get_scripts_path(handle)?);

    tauri::async_runtime::spawn({
        let handle = handle.clone();
        async move {
            let mut core_window = WindowBuilder::new(&handle, "core").title("ChatGPT");

            #[cfg(target_os = "macos")]
            {
                core_window = core_window
                    .title_bar_style(TitleBarStyle::Overlay)
                    .hidden_title(true);
            }

            core_window = core_window
                .resizable(true)
                .inner_size(800.0, 600.0)
                .min_inner_size(300.0, 200.0)
                .theme(Some(AppConf::get_theme(&handle)));

            let core_window = core_window
                .build()
                .expect("[core:window] Failed to build window");

            let win_size = core_window
                .inner_size()
                .expect("[core:window] Failed to get window size");
            // Wrap the window in Arc<Mutex<_>> to manage ownership across threads
            let window = Arc::new(Mutex::new(core_window));

            let main_view =
                WebviewBuilder::new("main", WebviewUrl::App("https://chatgpt.com".into()))
                    .auto_resize()
                    .on_download({
                        let app_handle = handle.clone();
                        let download_path = Arc::new(Mutex::new(PathBuf::new()));
                        move |_, event| {
                            match event {
                                DownloadEvent::Requested { destination: original_destination_ref, .. } => {
                                    // 2.a. Get the original destination
                                    let original_destination = original_destination_ref.clone();

                                    // 2.b. Create a safe_filename
                                    let safe_filename = original_destination.file_name()
                                        .map(PathBuf::from)
                                        .unwrap_or_else(|| PathBuf::from("downloaded_file.unknown"));

                                    // 2.c. Get download_dir
                                    let download_dir = app_handle
                                        .path()
                                        .download_dir()
                                        .expect("[view:download] Failed to get download directory");

                                    // 2.d. Construct the new final_save_path
                                    let final_save_path = download_dir.join(&safe_filename);

                                    // 3. Update the Arc<Mutex<PathBuf>> used by DownloadEvent::Finished
                                    let mut shared_download_path_lock = download_path
                                        .lock()
                                        .expect("[view:download] Failed to lock shared download path for update");
                                    *shared_download_path_lock = final_save_path.clone();

                                    // 2.e. Update the mutable destination in the event
                                    *original_destination_ref = final_save_path;
                                }
                                DownloadEvent::Finished { success, .. } => {
                                    let final_path = download_path
                                        .lock()
                                        .expect("[view:download] Failed to lock download path")
                                        .clone();

                                    if success {
                                        // Clone necessary variables for the async block
                                        let final_path_clone = final_path.clone();
                                        let app_handle_clone = app_handle.clone();

                                        // Spawn a new task for the async dialog
                                        tauri::async_runtime::spawn(async move {
                                            let file_name_str = final_path_clone
                                                .file_name()
                                                .unwrap_or_default() // Should have a filename due to prior sanitization
                                                .to_string_lossy()
                                                .to_string();

                                            let message = format!(
                                                "Download complete: {}. Do you want to open it?",
                                                file_name_str
                                            );

                                            // Show an ask dialog
                                            let confirmed = app_handle_clone
                                                .dialog()
                                                .ask(&message, "Open File?")
                                                .await
                                                .unwrap_or(false); // Default to false if dialog fails

                                            if confirmed {
                                                app_handle_clone
                                                    .shell()
                                                    .open(final_path_clone.to_string_lossy(), None)
                                                    .expect("[view:download] Failed to open file");
                                            }
                                        });
                                    }
                                }
                                _ => (),
                            }
                            true
                        }
                    })
                    .initialization_script(&AppConf::load_script(&handle, "ask.js"))
                    .initialization_script(INIT_SCRIPT);

            let titlebar_view = WebviewBuilder::new(
                "titlebar",
                WebviewUrl::App("index.html".into()),
            )
            .auto_resize();

            let ask_view =
                WebviewBuilder::new("ask", WebviewUrl::App("index.html".into()))
                    .auto_resize();

            let win = window.lock().unwrap();
            let scale_factor = win.scale_factor().unwrap();
            let titlebar_height = (scale_factor * TITLEBAR_HEIGHT).round() as u32;
            let ask_height = (scale_factor * ask_mode_height).round() as u32;

            #[cfg(target_os = "macos")]
            {
                let main_area_height = win_size.height - titlebar_height;

                win.add_child(
                    titlebar_view,
                    LogicalPosition::new(0, 0),
                    PhysicalSize::new(win_size.width, titlebar_height),
                )
                .unwrap();
                win.add_child(
                    ask_view,
                    LogicalPosition::new(
                        0.0,
                        (win_size.height as f64 / scale_factor) - ask_mode_height,
                    ),
                    PhysicalSize::new(win_size.width, ask_height),
                )
                .unwrap();
                win.add_child(
                    main_view,
                    LogicalPosition::new(0.0, TITLEBAR_HEIGHT),
                    PhysicalSize::new(win_size.width, main_area_height - ask_height),
                )
                .unwrap();
            }

            #[cfg(not(target_os = "macos"))]
            {
                win.add_child(
                    ask_view,
                    LogicalPosition::new(
                        0.0,
                        (win_size.height as f64 / scale_factor) - ask_mode_height,
                    ),
                    PhysicalSize::new(win_size.width, ask_height),
                )
                .unwrap();
                win.add_child(
                    titlebar_view,
                    LogicalPosition::new(
                        0.0,
                        (win_size.height as f64 / scale_factor) - ask_mode_height - TITLEBAR_HEIGHT,
                    ),
                    PhysicalSize::new(win_size.width, titlebar_height),
                )
                .unwrap();
                win.add_child(
                    main_view,
                    LogicalPosition::new(0.0, 0.0),
                    PhysicalSize::new(
                        win_size.width,
                        win_size.height - (ask_height + titlebar_height),
                    ),
                )
                .unwrap();
            }

            let window_clone = Arc::clone(&window);
            let set_view_properties =
                |view: &tauri::Webview, position: LogicalPosition<f64>, size: PhysicalSize<u32>| {
                    if let Err(e) = view.set_position(position) {
                        eprintln!("[view:position] Failed to set view position: {}", e);
                    }
                    if let Err(e) = view.set_size(size) {
                        eprintln!("[view:size] Failed to set view size: {}", e);
                    }
                };

            win.on_window_event(move |event| {
                let conf = &AppConf::load(&handle).unwrap();
                let ask_mode_height = if conf.ask_mode { ASK_HEIGHT } else { 0.0 };
                let ask_height = (scale_factor * ask_mode_height).round() as u32;

                if let WindowEvent::Resized(size) = event {
                    let win = window_clone.lock().unwrap();

                    let main_view = win
                        .get_webview("main")
                        .expect("[view:main] Failed to get webview window");
                    let titlebar_view = win
                        .get_webview("titlebar")
                        .expect("[view:titlebar] Failed to get webview window");
                    let ask_view = win
                        .get_webview("ask")
                        .expect("[view:ask] Failed to get webview window");

                    #[cfg(target_os = "macos")]
                    {
                        set_view_properties(
                            &main_view,
                            LogicalPosition::new(0.0, TITLEBAR_HEIGHT),
                            PhysicalSize::new(
                                size.width,
                                size.height - (titlebar_height + ask_height),
                            ),
                        );
                        set_view_properties(
                            &titlebar_view,
                            LogicalPosition::new(0.0, 0.0),
                            PhysicalSize::new(size.width, titlebar_height),
                        );
                        set_view_properties(
                            &ask_view,
                            LogicalPosition::new(
                                0.0,
                                (size.height as f64 / scale_factor) - ask_mode_height,
                            ),
                            PhysicalSize::new(size.width, ask_height),
                        );
                    }

                    #[cfg(not(target_os = "macos"))]
                    {
                        set_view_properties(
                            &main_view,
                            LogicalPosition::new(0.0, 0.0),
                            PhysicalSize::new(
                                size.width,
                                size.height - (ask_height + titlebar_height),
                            ),
                        );
                        set_view_properties(
                            &titlebar_view,
                            LogicalPosition::new(
                                0.0,
                                (size.height as f64 / scale_factor) - TITLEBAR_HEIGHT,
                            ),
                            PhysicalSize::new(size.width, titlebar_height),
                        );
                        set_view_properties(
                            &ask_view,
                            LogicalPosition::new(
                                0.0,
                                (size.height as f64 / scale_factor)
                                    - ask_mode_height
                                    - TITLEBAR_HEIGHT,
                            ),
                            PhysicalSize::new(size.width, ask_height),
                        );
                    }
                }
            });
        }
    });

    Ok(())
}
