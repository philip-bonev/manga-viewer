use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ArchiveFormat {
    Zip,
    Tar,
    SevenZ,
}

pub(crate) struct AppState {
    pub file_path: Option<PathBuf>,
    pub cli_file: Option<String>,
    pub archive_format: Option<ArchiveFormat>,
}

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tiff", "tif",
];

#[derive(Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub href: String,
    pub mime_type: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArchiveImages {
    pub images: Vec<ImageInfo>,
}

fn read_zip_entry(path: &std::path::Path, href: &str) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    let mut entry = archive.by_name(href).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}

fn read_tar_entry(path: &std::path::Path, href: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::new(file);
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().to_string();
        if entry_path == href {
            let mut data = Vec::new();
            entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
            return Ok(data);
        }
    }
    Err(format!("Entry '{}' not found in archive", href))
}

fn read_7z_entry(path: &std::path::Path, href: &str) -> Result<Vec<u8>, String> {
    let mut archive = sevenz_rust::SevenZReader::open(
        path.to_string_lossy().as_ref(),
        "".into(),
    )
    .map_err(|e| e.to_string())?;

    let mut result: Option<Vec<u8>> = None;
    archive
        .for_each_entries(|entry, reader| {
            if entry.name() == href {
                let mut data = Vec::new();
                if std::io::Read::read_to_end(reader, &mut data).is_ok() {
                    result = Some(data);
                }
                Ok(false)
            } else {
                // Must consume entry data to advance the decoder
                // (required for solid 7z archives where data is interleaved)
                let _ = std::io::copy(reader, &mut std::io::sink());
                Ok(true)
            }
        })
        .map_err(|e| e.to_string())?;

    result.ok_or_else(|| format!("Entry '{}' not found in archive", href))
}

fn mime_from_ext(ext: &str) -> &str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tiff" | "tif" => "image/tiff",
        _ => "image/jpeg",
    }
}

fn collect_images(
    entries: impl Iterator<Item = (String, String)>,
) -> Vec<ImageInfo> {
    let mut images: Vec<ImageInfo> = entries
        .filter(|(name, _)| {
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            IMAGE_EXTENSIONS.contains(&ext.as_str())
        })
        .map(|(name, _)| {
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            ImageInfo {
                id: String::new(),
                href: name,
                mime_type: mime_from_ext(&ext).to_string(),
            }
        })
        .collect();

    images.sort_by(|a, b| a.href.cmp(&b.href));

    for (i, img) in images.iter_mut().enumerate() {
        img.id = format!("{:05}", i);
    }

    images
}

#[tauri::command]
fn get_image_data(state: State<'_, Mutex<AppState>>, href: String) -> Result<String, String> {
    let state = state.lock().unwrap();
    let path = state
        .file_path
        .as_ref()
        .ok_or("No file opened")?
        .clone();
    let fmt = state.archive_format.unwrap_or(ArchiveFormat::Zip);
    drop(state);

    let image_data = match fmt {
        ArchiveFormat::Zip => read_zip_entry(&path, &href)?,
        ArchiveFormat::Tar => read_tar_entry(&path, &href)?,
        ArchiveFormat::SevenZ => read_7z_entry(&path, &href)?,
    };

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_data);
    Ok(encoded)
}

#[tauri::command]
fn close_file(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    state.file_path = None;
    state.archive_format = None;
    Ok(())
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

fn extract_series_info(file_stem: &str) -> Option<(String, f64)> {
    let re = regex::Regex::new(r"^(.*?)[-_ ](\d+(?:\.\d+)?)").ok()?;
    let captures = re.captures(file_stem)?;
    let series = captures.get(1)?.as_str().trim().to_lowercase();
    let num: f64 = captures.get(2)?.as_str().parse().ok()?;
    if series.is_empty() {
        return None;
    }
    Some((series, num))
}

#[tauri::command]
fn get_next_file(path: String) -> Option<String> {
    let file_path = std::path::PathBuf::from(&path);
    let parent = file_path.parent()?;
    let file_name = file_path.file_stem()?.to_string_lossy().to_string();
    let extension = file_path.extension()?.to_string_lossy().to_string();

    let (current_series, current_num) = extract_series_info(&file_name)?;

    let mut candidates: Vec<(f64, std::path::PathBuf)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if !entry_path.is_file() {
                continue;
            }
            if entry_path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                != Some(extension.clone())
            {
                continue;
            }
            let entry_stem = entry_path.file_stem()?.to_string_lossy().to_string();
            if let Some((series, num)) = extract_series_info(&entry_stem) {
                if series == current_series {
                    candidates.push((num, entry_path));
                }
            }
        }
    }

    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    for (num, next_path) in candidates {
        if num > current_num {
            return next_path.to_str().map(|s| s.to_string());
        }
    }

    None
}

#[tauri::command]
fn get_prev_file(path: String) -> Option<String> {
    let file_path = std::path::PathBuf::from(&path);
    let parent = file_path.parent()?;
    let file_name = file_path.file_stem()?.to_string_lossy().to_string();
    let extension = file_path.extension()?.to_string_lossy().to_string();

    let (current_series, current_num) = extract_series_info(&file_name)?;

    let mut candidates: Vec<(f64, std::path::PathBuf)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if !entry_path.is_file() {
                continue;
            }
            if entry_path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                != Some(extension.clone())
            {
                continue;
            }
            let entry_stem = entry_path.file_stem()?.to_string_lossy().to_string();
            if let Some((series, num)) = extract_series_info(&entry_stem) {
                if series == current_series {
                    candidates.push((num, entry_path));
                }
            }
        }
    }

    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    for (num, prev_path) in candidates {
        if num < current_num {
            return prev_path.to_str().map(|s| s.to_string());
        }
    }

    None
}

#[tauri::command]
fn open_cbz_file(state: State<'_, Mutex<AppState>>, path: String) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let mut file = std::fs::File::open(&file_path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let entries = (0..archive.len()).filter_map(|i| {
        let file = archive.by_index(i).ok()?;
        let name = file.name().to_string();
        if name.starts_with("__MACOSX") || name.ends_with('/') {
            return None;
        }
        let ext = name.split('.').last().unwrap_or("").to_lowercase();
        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            Some((name, ext))
        } else {
            None
        }
    });

    let images = collect_images(entries);

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(file_path);
        state.archive_format = Some(ArchiveFormat::Zip);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command]
fn open_cbt_file(state: State<'_, Mutex<AppState>>, path: String) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let file = std::fs::File::open(&file_path).map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::new(file);
    let mut archive = tar::Archive::new(reader);

    let entries = archive.entries().map_err(|e| e.to_string())?;
    let names: Vec<(String, String)> = entries
        .filter_map(|e| {
            let entry = e.ok()?;
            let path = entry.path().ok()?;
            let name = path.to_string_lossy().to_string();
            if name.ends_with('/') {
                return None;
            }
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                Some((name, ext))
            } else {
                None
            }
        })
        .collect();

    let images = collect_images(names.into_iter());

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(file_path);
        state.archive_format = Some(ArchiveFormat::Tar);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command]
fn open_cb7_file(state: State<'_, Mutex<AppState>>, path: String) -> Result<ArchiveImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let reader = sevenz_rust::SevenZReader::open(
        file_path.to_string_lossy().as_ref(),
        "".into(),
    )
    .map_err(|e| e.to_string())?;

    let entries: Vec<(String, String)> = reader
        .archive()
        .files
        .iter()
        .filter_map(|entry| {
            let name = entry.name().to_string();
            if entry.is_directory() {
                return None;
            }
            let ext = name.split('.').last().unwrap_or("").to_lowercase();
            if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                Some((name, ext))
            } else {
                None
            }
        })
        .collect();

    let images = collect_images(entries.into_iter());

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(file_path);
        state.archive_format = Some(ArchiveFormat::SevenZ);
    }

    Ok(ArchiveImages { images })
}

#[tauri::command]
fn get_cli_file(state: State<'_, Mutex<AppState>>) -> Option<String> {
    let mut state = state.lock().unwrap();
    state.cli_file.take()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli_file = find_file_in_args(std::env::args().skip(1));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            use tauri::Emitter;
            use tauri::Manager;
            if let Some(path) = find_file_in_args(argv.iter().cloned()) {
                let state = app.state::<Mutex<AppState>>();
                let mut state = state.inner().lock().unwrap();
                state.cli_file = Some(path.clone());
                let _ = app.emit("file-opened", path);
            }
        }))
        .manage(Mutex::new(AppState {
            file_path: None,
            cli_file,
            archive_format: None,
        }))
        .invoke_handler(tauri::generate_handler![
            open_cbz_file,
            open_cbt_file,
            open_cb7_file,
            get_image_data,
            close_file,
            exit_app,
            get_cli_file,
            get_next_file,
            get_prev_file
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    #[allow(unused_variables)]
    app.run(|app_handle, event| {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
        if let tauri::RunEvent::Opened { urls } = event {
            use tauri::Emitter;
            use tauri::Manager;
            for url in urls {
                if url.scheme() == "file" {
                    if let Ok(path) = url.to_file_path() {
                        if let Some(ext) = path.extension() {
                            let ext = ext.to_string_lossy().to_lowercase();
                            if matches!(ext.as_ref(), "cbz" | "cbt" | "cb7") {
                                let path_str = path.to_string_lossy().to_string();
                                let state = app_handle.state::<Mutex<AppState>>();
                                let mut state = state.inner().lock().unwrap();
                                state.cli_file = Some(path_str.clone());
                                let _ = app_handle.emit("file-opened", path_str);
                            }
                        }
                    }
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
        let path = std::path::Path::new(&arg);
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if matches!(ext.as_ref(), "cbz" | "cbt" | "cb7") {
                let decoded = percent_encoding::percent_decode_str(&arg).decode_utf8_lossy();
                return Some(
                    decoded
                        .strip_prefix("file://")
                        .unwrap_or(&decoded)
                        .to_string(),
                );
            }
        }
    }
    None
}
