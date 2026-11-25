// Copyright 2026 Philip Bonev
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://apache.org
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod archive;
mod commands;
mod images;
mod model;
mod series;
mod state;

use state::{AppState, lock_state};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

pub fn run() {
    let cli_file = find_file_in_args(std::env::args().skip(1));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(path) = find_file_in_args(argv.iter().skip(1).cloned()) {
                let state = app.state::<Mutex<AppState>>();
                let mut state = lock_state(state.inner());
                state.cli_file = Some(path.clone());
                let _ = app.emit("file-opened", path);
            }
        }))
        .manage(Mutex::new(AppState {
            file_path: None,
            archive: None,
            cli_file,
        }))
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::open_cbz_file,
            commands::open_cbt_file,
            commands::open_cb7_file,
            commands::get_image_data,
            commands::get_cli_file,
            commands::get_next_file,
            commands::get_prev_file
        ])
        .setup(|_app| {
            // Only automatically open DevTools during debug builds
            #[cfg(debug_assertions)]
            if let Some(window) = _app.get_webview_window("main") {
                window.open_devtools();
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    #[allow(unused_variables)]
    app.run(|app_handle, event| {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
        if let tauri::RunEvent::Opened { urls } = event {
            for url in urls {
                if url.scheme() != "file" {
                    continue;
                }
                let Ok(path) = url.to_file_path() else {
                    continue;
                };
                let Some(ext) = path.extension() else {
                    continue;
                };
                let ext = ext.to_string_lossy().to_lowercase();
                if matches!(ext.as_str(), "cbz" | "cbt" | "cb7") {
                    let path_str = path.to_string_lossy().to_string();
                    let state = app_handle.state::<Mutex<AppState>>();
                    let mut state = lock_state(state.inner());
                    state.cli_file = Some(path_str.clone());
                    let _ = app_handle.emit("file-opened", path_str);
                }
            }
        }
    });
}

fn find_file_in_args(args: impl Iterator<Item = String>) -> Option<String> {
    for arg in args {
        #[cfg(target_os = "macos")]
        if arg.starts_with("-psn_") {
            continue;
        }
        if arg.starts_with("-") {
            continue;
        }
        let decoded = percent_encoding::percent_decode_str(&arg)
            .decode_utf8_lossy()
            .to_string();
        let mut path_str = decoded
            .strip_prefix("file://")
            .unwrap_or(&decoded)
            .to_string();
        if cfg!(windows) {
            // "file:///C:/dir/file.cbz" strips to "/C:/dir/file.cbz"; drop the leading slash
            let bytes = path_str.as_bytes();
            if bytes.len() >= 3
                && bytes[0] == b'/'
                && bytes[1].is_ascii_alphabetic()
                && bytes[2] == b':'
            {
                path_str = path_str[1..].to_string();
            }
        }
        if let Some(ext) = std::path::Path::new(&path_str).extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if matches!(ext.as_str(), "cbz" | "cbt" | "cb7") {
                return Some(path_str);
            }
        }
    }
    None
}
