use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

pub struct EpubState {
    pub file_path: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub href: String,
    pub mime_type: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EpubImages {
    pub images: Vec<ImageInfo>,
}

fn resolve_base_path(opf_content: &str) -> String {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(opf_content);
    reader.config_mut().trim_text(true);

    let mut base_path = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"package" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"xml:base" {
                        base_path = String::from_utf8_lossy(&attr.value).to_string();
                        break;
                    }
                }
                break;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => continue,
        }
    }

    base_path
}

fn parse_opf(opf_content: &str) -> (HashMap<String, (String, String)>, Vec<String>) {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(opf_content);
    reader.config_mut().trim_text(true);

    let mut manifest: HashMap<String, (String, String)> = HashMap::new();
    let mut spine: Vec<String> = Vec::new();
    let mut in_manifest = false;
    let mut in_spine = false;
    let mut current_item_id = String::new();
    let mut current_item_href = String::new();
    let mut current_item_media = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"manifest" => in_manifest = true,
                    b"spine" => in_spine = true,
                    b"item" if in_manifest => {
                        current_item_id.clear();
                        current_item_href.clear();
                        current_item_media.clear();
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"id" => {
                                    current_item_id =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                                b"href" => {
                                    current_item_href =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                                b"media-type" => {
                                    current_item_media =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                                _ => {}
                            }
                        }
                    }
                    b"itemref" if in_spine => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"idref" {
                                let idref = String::from_utf8_lossy(&attr.value).to_string();
                                spine.push(idref);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                match e.name().as_ref() {
                    b"item" if in_manifest && !current_item_id.is_empty() => {
                        manifest.insert(
                            current_item_id.clone(),
                            (current_item_href.clone(), current_item_media.clone()),
                        );
                    }
                    b"manifest" => in_manifest = false,
                    b"spine" => in_spine = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => continue,
        }
    }

    (manifest, spine)
}

fn extract_images_from_html(
    html_content: &str,
    base_path: &str,
    manifest: &HashMap<String, (String, String)>,
) -> Vec<String> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(html_content);
    reader.config_mut().trim_text(true);

    let mut images = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"img" || e.name().as_ref() == b"image" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"src" || attr.key.as_ref() == b"xlink:href" {
                            let src = String::from_utf8_lossy(&attr.value).to_string();
                            let resolved = if base_path.is_empty() {
                                src.clone()
                            } else {
                                format!("{}/{}", base_path, src)
                            };
                            images.push(resolved);
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"img" || e.name().as_ref() == b"image" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"src" || attr.key.as_ref() == b"xlink:href" {
                            let src = String::from_utf8_lossy(&attr.value).to_string();
                            let resolved = if base_path.is_empty() {
                                src.clone()
                            } else {
                                format!("{}/{}", base_path, src)
                            };
                            images.push(resolved);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => continue,
        }
    }

    images
        .into_iter()
        .filter(|href| {
            manifest.values().any(|(_, media)| {
                media.starts_with("image/") && href.ends_with(&href.split('/').last().unwrap_or(""))
            })
        })
        .collect()
}

fn normalize_path(base: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();

    for part in relative.split('/') {
        match part {
            ".." => {
                if !parts.is_empty() {
                    parts.pop();
                }
            }
            "." => {}
            _ => parts.push(part),
        }
    }

    parts.join("/")
}

#[tauri::command]
fn open_epub_file(
    state: State<'_, Mutex<EpubState>>,
    path: String,
) -> Result<EpubImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let path = file_path;

    let mut epub_file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    epub_file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let mut container_xml = String::new();
    archive
        .by_name("META-INF/container.xml")
        .map_err(|e| e.to_string())?
        .read_to_string(&mut container_xml)
        .map_err(|e| e.to_string())?;

    let opf_path = {
        use quick_xml::events::Event;
        use quick_xml::reader::Reader;

        let mut reader = Reader::from_str(&container_xml);
        reader.config_mut().trim_text(true);
        let mut opf_path = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) if e.name().as_ref() == b"rootfile" => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"full-path" {
                            opf_path = String::from_utf8_lossy(&attr.value).to_string();
                            break;
                        }
                    }
                    break;
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => continue,
            }
        }

        if opf_path.is_empty() {
            return Err("Could not find OPF file in EPUB".to_string());
        }
        opf_path
    };

    let mut opf_content = String::new();
    archive
        .by_name(&opf_path)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut opf_content)
        .map_err(|e| e.to_string())?;

    let base_path = resolve_base_path(&opf_content);
    let (manifest, spine) = parse_opf(&opf_content);

    let mut all_images: Vec<ImageInfo> = Vec::new();
    let mut seen_hrefs: HashMap<String, usize> = HashMap::new();

    for item_id in &spine {
        if let Some((href, media_type)) = manifest.get(item_id) {
            if media_type.starts_with("text/") || media_type.contains("html") {
                let full_href = if base_path.is_empty() {
                    href.clone()
                } else {
                    format!("{}/{}", base_path, href)
                };

                if let Ok(mut file) = archive.by_name(&full_href) {
                    let mut html_content = String::new();
                    if file.read_to_string(&mut html_content).is_ok() {
                        let img_hrefs =
                            extract_images_from_html(&html_content, &base_path, &manifest);
                        for img_href in img_hrefs {
                            let normalized = normalize_path(&full_href, &img_href);
                            if let Some((_, (_, mime_type))) =
                                manifest.iter().find(|(_, (h, _))| {
                                    let full_h = if base_path.is_empty() {
                                        h.clone()
                                    } else {
                                        format!("{}/{}", base_path, h)
                                    };
                                    full_h == normalized || h == &img_href
                                })
                            {
                                let mime = mime_type.clone();
                                let id = item_id.clone();
                                let entry = seen_hrefs.entry(normalized.clone()).or_insert(0);
                                *entry += 1;
                                all_images.push(ImageInfo {
                                    id: format!("{}-{}", id, entry),
                                    href: normalized,
                                    mime_type: mime,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if all_images.is_empty() {
        for (id, (href, media_type)) in &manifest {
            if media_type.starts_with("image/") {
                let full_href = if base_path.is_empty() {
                    href.clone()
                } else {
                    format!("{}/{}", base_path, href)
                };
                all_images.push(ImageInfo {
                    id: id.clone(),
                    href: full_href,
                    mime_type: media_type.clone(),
                });
            }
        }
    }

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(path);
    }

    Ok(EpubImages { images: all_images })
}

#[tauri::command]
fn get_image_data(
    state: State<'_, Mutex<EpubState>>,
    href: String,
) -> Result<String, String> {
    let state = state.lock().unwrap();
    let path = state
        .file_path
        .as_ref()
        .ok_or("No EPUB file opened")?
        .clone();
    drop(state);

    let mut epub_file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    epub_file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let mut file = archive.by_name(&href).map_err(|e| e.to_string())?;
    let mut image_data = Vec::new();
    file.read_to_end(&mut image_data).map_err(|e| e.to_string())?;

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_data);

    Ok(encoded)
}

#[tauri::command]
fn close_epub(state: State<'_, Mutex<EpubState>>) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    state.file_path = None;
    Ok(())
}

#[tauri::command]
fn open_cbz_file(
    state: State<'_, Mutex<EpubState>>,
    path: String,
) -> Result<EpubImages, String> {
    let file_path = std::path::PathBuf::from(&path);

    let mut epub_file = std::fs::File::open(&file_path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    epub_file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(buffer);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let image_extensions = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tiff", "tif"];
    let mut images: Vec<ImageInfo> = Vec::new();

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();

        if name.starts_with("__MACOSX") || name.ends_with('/') {
            continue;
        }

        let ext = name.split('.').last().unwrap_or("").to_lowercase();
        if image_extensions.contains(&ext.as_str()) {
            let mime = match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "bmp" => "image/bmp",
                "svg" => "image/svg+xml",
                "tiff" | "tif" => "image/tiff",
                _ => "image/jpeg",
            };
            images.push(ImageInfo {
                id: format!("{:05}", i),
                href: name,
                mime_type: mime.to_string(),
            });
        }
    }

    images.sort_by(|a, b| a.href.cmp(&b.href));

    for (i, img) in images.iter_mut().enumerate() {
        img.id = format!("{:05}", i);
    }

    {
        let mut state = state.lock().unwrap();
        state.file_path = Some(file_path);
    }

    Ok(EpubImages { images })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(Mutex::new(EpubState { file_path: None }))
        .invoke_handler(tauri::generate_handler![
            open_epub_file,
            open_cbz_file,
            get_image_data,
            close_epub
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
