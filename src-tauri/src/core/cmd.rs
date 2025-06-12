use std::fs;
use std::path::PathBuf;
use tauri::{command, AppHandle, Manager, Window}; // Adjusted imports

use crate::core::{
    conf::AppConf,
    constant::{ASK_HEIGHT, TITLEBAR_HEIGHT},
};

#[command]
pub fn view_reload(app: AppHandle) {
    app.get_window("core")
        .unwrap()
        .get_webview("main")
        .unwrap()
        .eval("window.location.reload()")
        .unwrap();
}

#[command]
pub fn view_url(app: AppHandle) -> tauri::Url {
    app.get_window("core")
        .unwrap()
        .get_webview("main")
        .unwrap()
        .url()
        .unwrap()
}

#[command]
pub fn view_go_forward(app: AppHandle) {
    app.get_window("core")
        .unwrap()
        .get_webview("main")
        .unwrap()
        .eval("window.history.forward()")
        .unwrap();
}

#[command]
pub fn view_go_back(app: AppHandle) {
    app.get_window("core")
        .unwrap()
        .get_webview("main")
        .unwrap()
        .eval("window.history.back()")
        .unwrap();
}

#[command]
pub fn window_pin(app: AppHandle, pin: bool) {
    let conf = AppConf::load(&app).unwrap();
    conf.amend(serde_json::json!({"stay_on_top": pin}))
        .unwrap()
        .save(&app)
        .unwrap();

    app.get_window("core")
        .unwrap()
        .set_always_on_top(pin)
        .unwrap();
}

#[command]
pub fn ask_sync(app: AppHandle, message: String) {
    app.get_window("core")
        .unwrap()
        .get_webview("main")
        .unwrap()
        .eval(&format!("ChatAsk.sync({})", message))
        .unwrap();
}

#[command]
pub fn ask_send(app: AppHandle) {
    let win = app.get_window("core").unwrap();

    win.get_webview("main")
        .unwrap()
        .eval(
            r#"
        ChatAsk.submit();
        setTimeout(() => {
            __TAURI__.webview.Webview.getByLabel('ask')?.setFocus();
        }, 500);
        "#,
        )
        .unwrap();
}

#[command]
pub fn set_theme(app: AppHandle, theme: String) {
    let conf = AppConf::load(&app).unwrap();
    conf.amend(serde_json::json!({"theme": theme}))
        .unwrap()
        .save(&app)
        .unwrap();

    app.restart();
}

#[command]
pub fn get_app_conf(app: AppHandle) -> AppConf {
    AppConf::load(&app).unwrap()
}

#[command]
pub fn set_view_ask(app: AppHandle, enabled: bool) {
    let conf = AppConf::load(&app).unwrap();
    conf.amend(serde_json::json!({"ask_mode": enabled}))
        .unwrap()
        .save(&app)
        .unwrap();

    let core_window = app.get_window("core").unwrap();
    let ask_mode_height = if enabled { ASK_HEIGHT } else { 0.0 };
    let scale_factor = core_window.scale_factor().unwrap();
    let titlebar_height = (scale_factor * TITLEBAR_HEIGHT).round() as u32;
    let win_size = core_window.inner_size().unwrap();
    let ask_height = (scale_factor * ask_mode_height).round() as u32;

    let main_view = core_window.get_webview("main").unwrap();
    let titlebar_view = core_window.get_webview("titlebar").unwrap();
    let ask_view = core_window.get_webview("ask").unwrap();

    if enabled {
        ask_view.set_focus().unwrap();
    } else {
        main_view.set_focus().unwrap();
    }

    let set_view_properties =
        |view: &tauri::Webview, position: LogicalPosition<f64>, size: PhysicalSize<u32>| {
            if let Err(e) = view.set_position(position) {
                eprintln!("[cmd:view:position] Failed to set view position: {}", e);
            }
            if let Err(e) = view.set_size(size) {
                eprintln!("[cmd:view:size] Failed to set view size: {}", e);
            }
        };

    #[cfg(target_os = "macos")]
    {
        set_view_properties(
            &main_view,
            LogicalPosition::new(0.0, TITLEBAR_HEIGHT),
            PhysicalSize::new(
                win_size.width,
                win_size.height - (titlebar_height + ask_height),
            ),
        );
        set_view_properties(
            &titlebar_view,
            LogicalPosition::new(0.0, 0.0),
            PhysicalSize::new(win_size.width, titlebar_height),
        );
        set_view_properties(
            &ask_view,
            LogicalPosition::new(
                0.0,
                (win_size.height as f64 / scale_factor) - ask_mode_height,
            ),
            PhysicalSize::new(win_size.width, ask_height),
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        set_view_properties(
            &main_view,
            LogicalPosition::new(0.0, 0.0),
            PhysicalSize::new(
                win_size.width,
                win_size.height - (ask_height + titlebar_height),
            ),
        );
        set_view_properties(
            &titlebar_view,
            LogicalPosition::new(
                0.0,
                (win_size.height as f64 / scale_factor) - TITLEBAR_HEIGHT,
            ),
            PhysicalSize::new(win_size.width, titlebar_height),
        );
        set_view_properties(
            &ask_view,
            LogicalPosition::new(
                0.0,
                (win_size.height as f64 / scale_factor) - ask_mode_height - TITLEBAR_HEIGHT,
            ),
            PhysicalSize::new(win_size.width, ask_height),
        );
    }
}

#[command]
pub fn debug_get_webview_content(app_handle: AppHandle, webview_label: String) -> Result<(), String> {
    match app_handle.get_webview(&webview_label) {
        Some(webview) => {
            let window_label = match webview.window() { // Match on the Result
                Ok(w) => w.label().to_string(),
                Err(e) => {
                    eprintln!("Error getting window for webview {}: {}", webview_label, e);
                    "unknown_window".to_string()
                }
            };
            let filename = format!("webview_content_window_{}_webview_{}.html", window_label, webview_label);
            let path = PathBuf::from("/tmp").join(&filename);

            let current_url = webview.url().map_or_else(
                |e| format!("Error getting URL: {}", e),
                |u| u.to_string()
            );

            // Attempt to get HTML content from eval
            let eval_result_html: Result<String, _> = webview.eval("return document.documentElement.outerHTML");

            match eval_result_html {
                Ok(html_output_str) => { // html_output_str should be String here
                    let full_content = format!("<!-- Window Label: {} -->
<!-- Webview Label: {} -->
<!-- Webview URL: {} -->
{}", window_label, &webview_label, current_url, html_output_str);
                    fs::write(&path, full_content)
                        .map_err(|e| format!("Failed to write HTML content to {}: {}", path.display(), e))?;
                    Ok(())
                }
                Err(e_eval) => { // Handle eval error
                    let error_message_for_file = format!("<!-- Window Label: {} -->
<!-- Webview Label: {} -->
<!-- Webview URL: {} -->
<!-- Error getting HTML content via eval: {} -->", window_label, &webview_label, current_url, e_eval);
                    fs::write(&path, error_message_for_file)
                        .map_err(|e_write| format!("Failed to write error content to {}: {}", path.display(), e_write))?;
                    Err(format!("Failed to eval script for HTML content in webview {}: {}", webview_label, e_eval))
                }
            }
        }
        None => {
            let filename = format!("webview_content_webview_{}_not_found.txt", webview_label);
            let path = PathBuf::from("/tmp").join(&filename);
            let error_message = format!("Webview with label {} not found.", webview_label);
            fs::write(&path, &error_message)
                .map_err(|e| format!("Failed to write error to {}: {}", path.display(), e))?;
            Err(error_message)
        }
    }
}
