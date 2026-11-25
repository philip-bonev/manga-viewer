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

//! Tauri command handlers exposed to the frontend over IPC.

use crate::archive::{open_sevenz, open_tar, open_zip, read_entry};
use crate::model::{AppInfo, ArchiveImages};
use crate::series::find_sibling_file;
use crate::state::{lock_state, AppState};
use std::sync::Mutex;
use tauri::State;

const LICENSE_TEXT: &str = include_str!("../LICENSE");

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_date: env!("BUILD_DATE").to_string(),
        author: env!("CARGO_PKG_AUTHORS").to_string(),
        license_name: "Apache License 2.0".to_string(),
        license_text: LICENSE_TEXT.to_string(),
        repository: env!("CARGO_PKG_REPOSITORY").to_string(),
    }
}

#[tauri::command(async)]
pub fn get_image_data(
    state: State<'_, Mutex<AppState>>,
    href: String,
) -> Result<String, String> {
    let mut state = lock_state(&state);
    let path = state
        .file_path
        .clone()
        .ok_or_else(|| "No file opened".to_string())?;

    let image_data = match state.archive.as_mut() {
        Some(archive) => read_entry(archive, &path, &href)?,
        None => return Err("No file opened".to_string()),
    };
    drop(state);

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_data);
    Ok(encoded)
}

#[tauri::command(async)]
pub fn open_cbz_file(
    state: State<'_, Mutex<AppState>>,
    path: String,
) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);
    let (images, archive) = open_zip(&file_path)?;

    {
        let mut state = lock_state(&state);
        state.file_path = Some(file_path);
        state.archive = Some(archive);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command(async)]
pub fn open_cbt_file(
    state: State<'_, Mutex<AppState>>,
    path: String,
) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);
    let (images, archive) = open_tar(&file_path)?;

    {
        let mut state = lock_state(&state);
        state.file_path = Some(file_path);
        state.archive = Some(archive);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command(async)]
pub fn open_cb7_file(
    state: State<'_, Mutex<AppState>>,
    path: String,
) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);
    let (images, archive) = open_sevenz(&file_path)?;

    {
        let mut state = lock_state(&state);
        state.file_path = Some(file_path);
        state.archive = Some(archive);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command]
pub fn get_cli_file(state: State<'_, Mutex<AppState>>) -> Option<String> {
    let mut state = lock_state(&state);
    state.cli_file.take()
}

#[tauri::command]
pub fn get_next_file(path: String) -> Option<String> {
    find_sibling_file(&path, true)
}

#[tauri::command]
pub fn get_prev_file(path: String) -> Option<String> {
    find_sibling_file(&path, false)
}
