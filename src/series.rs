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

//! Series-aware neighbour issue lookup (e.g. `Berserk-001.cbz` -> `Berserk-002.cbz`).

fn extract_series_info(file_stem: &str) -> Option<(String, f64)> {
    static SERIES_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = SERIES_RE.get_or_init(|| {
        regex::Regex::new(r"^(.*?)[-_ ](\d+(?:\.\d+)?)").expect("static series regex is valid")
    });
    let captures = re.captures(file_stem)?;
    let series = captures.get(1)?.as_str().trim().to_lowercase();
    let num: f64 = captures.get(2)?.as_str().parse().ok()?;
    if series.is_empty() {
        return None;
    }
    Some((series, num))
}

pub(crate) fn find_sibling_file(path: &str, forward: bool) -> Option<String> {
    let file_path = std::path::PathBuf::from(path);
    let parent = file_path.parent()?;
    let file_name = file_path.file_stem()?.to_string_lossy().to_string();
    let extension = file_path.extension()?.to_string_lossy().to_lowercase();

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
            let Some(entry_stem) = entry_path.file_stem() else {
                continue;
            };
            let Some((series, num)) = extract_series_info(&entry_stem.to_string_lossy()) else {
                continue;
            };
            if series == current_series {
                candidates.push((num, entry_path));
            }
        }
    }

    candidates.sort_by(|a, b| {
        let ord = if forward {
            a.0.partial_cmp(&b.0)
        } else {
            b.0.partial_cmp(&a.0)
        };
        ord.unwrap_or(std::cmp::Ordering::Equal)
    });

    for (num, sibling) in candidates {
        if (forward && num > current_num) || (!forward && num < current_num) {
            return sibling.to_str().map(|s| s.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_series_info_parses_name_and_number() {
        assert_eq!(
            extract_series_info("Berserk-001"),
            Some(("berserk".to_string(), 1.0))
        );
        assert_eq!(
            extract_series_info("solo leveling 012"),
            Some(("solo leveling".to_string(), 12.0))
        );
        assert_eq!(
            extract_series_info("chapter-1.5"),
            Some(("chapter".to_string(), 1.5))
        );
        assert_eq!(extract_series_info("no-numbers-here"), None);
        assert_eq!(extract_series_info("123"), None);
    }

    #[test]
    fn find_sibling_file_orders_by_issue_number() {
        let dir = std::env::temp_dir().join(format!("manga-viewer-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        for name in ["Berserk-001.cbz", "Berserk-002.cbz", "Berserk-010.cbz", "Other-001.cbz"] {
            std::fs::write(dir.join(name), b"").expect("write placeholder");
        }

        let current = dir.join("Berserk-002.cbz");
        let next = find_sibling_file(current.to_str().unwrap(), true).expect("next exists");
        assert!(next.ends_with("Berserk-010.cbz"));
        let prev = find_sibling_file(current.to_str().unwrap(), false).expect("prev exists");
        assert!(prev.ends_with("Berserk-001.cbz"));

        // Uppercase extensions match case-insensitively.
        let current_up = dir.join("Berserk-002.CBZ");
        std::fs::write(&current_up, b"").expect("write placeholder");
        assert!(find_sibling_file(current_up.to_str().unwrap(), true).is_some());

        std::fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }
}
