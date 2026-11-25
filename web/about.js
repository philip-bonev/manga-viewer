/*
 * Copyright 2026 Philip Bonev
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://apache.org
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

const { invoke } = window.__TAURI__.core;

async function fillAbout() {
  try {
    const info = await invoke('get_app_info');
    document.getElementById('about-name').textContent = info.name;
    document.getElementById('about-version').textContent = info.version;
    document.getElementById('about-build-date').textContent = info.build_date;
    document.getElementById('about-author').textContent = info.author;
    document.getElementById('about-license-name').textContent = info.license_name;
    const repository = document.getElementById('about-repository');
    repository.textContent = info.repository;
    repository.href = info.repository;
    document.getElementById('about-license-text').textContent = info.license_text;
  } catch (error) {
    console.error('Failed to load app info:', error);
  }
}

window.addEventListener('DOMContentLoaded', fillAbout);
