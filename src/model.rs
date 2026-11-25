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

//! Serializable types exchanged with the frontend over Tauri IPC.

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub build_date: String,
    pub author: String,
    pub license_name: String,
    pub license_text: String,
    pub repository: String,
}

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
