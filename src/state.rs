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

//! Shared application state guarded by a mutex.

use crate::archive::OpenArchive;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

pub(crate) struct AppState {
    pub file_path: Option<PathBuf>,
    pub archive: Option<OpenArchive>,
    pub cli_file: Option<String>,
}

pub(crate) fn lock_state(state: &Mutex<AppState>) -> MutexGuard<'_, AppState> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}
