use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{
    webview::DownloadEvent, App, Manager, WebviewBuilder, WebviewUrl, WindowBuilder, WindowEvent,
};
// LogicalPosition, PhysicalSize might become unused, also ASK_HEIGHT, TITLEBAR_HEIGHT from constants
use tauri_plugin_dialog::{DialogExt, MessageDialogBuilder};
use tauri_plugin_shell::ShellExt;

#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;

use crate::core::{
    conf::AppConf,
    constant::INIT_SCRIPT, // ASK_HEIGHT, TITLEBAR_HEIGHT removed
    template,
};

pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle();

    // let conf = &AppConf::load(handle)?; // ask_mode_height is no longer needed here
    // let ask_mode_height = if conf.ask_mode { ASK_HEIGHT } else { 0.0 };

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

                                            // Use MessageDialogBuilder with callback-based show
                                            let dialog_builder = MessageDialogBuilder::new(
                                                app_handle_clone.dialog().clone(), // Correctly get Dialog<R> and clone it
                                                "Open File?",
                                                &message
                                            );

                                            // final_path_clone and app_handle_clone are captured by the async move block
                                            dialog_builder.show(move |confirmed| {
                                                if confirmed {
                                                    if let Err(e) = app_handle_clone.shell().open(final_path_clone.to_string_lossy(), None) {
                                                        eprintln!("[Download] Failed to open file {}: {}", final_path_clone.display(), e);
                                                    }
                                                }
                                            });
                                        });
                                    }
                                }
                                _ => (),
                            }
                            true
                        }
                    })
                    // .initialization_script(&AppConf::load_script(&handle, "ask.js")) // Removed ask.js
                    .initialization_script(INIT_SCRIPT);

            // titlebar_view and ask_view are removed
            // let titlebar_view = WebviewBuilder::new(...)
            // let ask_view = WebviewBuilder::new(...)

            let win = window.lock().unwrap();
            // scale_factor, titlebar_height, ask_height might be unused now unless other logic needs them
            // let scale_factor = win.scale_factor().unwrap();
            // let titlebar_height = (scale_factor * TITLEBAR_HEIGHT).round() as u32;
            // let ask_height = (scale_factor * ask_mode_height).round() as u32;

            // Add main_view as the only child, filling the window
            // The PhysicalSize can be derived from win_size obtained earlier, or let auto_resize handle it.
            // For clarity, explicitly set it to the window's inner dimensions.
            win.add_child(
                main_view,
                tauri::LogicalPosition::new(0.0, 0.0), // Position at top-left
                win_size, // Size to fill the window
            )
            .unwrap();

            // The complex multi-view resizing logic in on_window_event is removed.
            // main_view has auto_resize(), so it should adapt.
            // If other window events need to be handled, the on_window_event can be kept,
            // but the specific resizing code for the three views is gone.
            // For now, we remove the specific Resized event handling logic.
            let window_clone = Arc::clone(&window);
            win.on_window_event(move |event| {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        // Example: if you wanted to prevent close or do something else
                        // api.prevent_close();
                        // For now, just let it proceed or remove if no custom handling needed.
                        // Default close behavior will occur if not handled.
                    }
                    WindowEvent::Resized(_size) => {
                        // The main_view should auto-resize. If specific adjustments were needed
                        // for a single view, they could be done here, but usually not necessary
                        // if the view is added to fill the parent.
                        // We can log or leave this empty if auto-resize is sufficient.
                        // println!("Window resized. Main view should auto-resize.");
                    }
                    // Handle other events as needed
                    _ => {}
                }
            });
        }
    });

    Ok(())
}
