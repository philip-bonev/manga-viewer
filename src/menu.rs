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

//! Application menu: About (opens the dedicated About window) and Quit.

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const ABOUT_WINDOW: &str = "about";

pub(crate) fn setup(app: &tauri::App) -> tauri::Result<()> {
    let app_name = app.package_info().name.clone();
    let about = MenuItemBuilder::with_id("about", format!("About {app_name}")).build(app)?;
    let app_menu = SubmenuBuilder::new(app, &app_name)
        .item(&about)
        .separator()
        .quit()
        .build()?;
    let menu = MenuBuilder::new(app).items(&[&app_menu]).build()?;
    app.set_menu(menu)?;

    app.on_menu_event(|app, event| {
        if event.id() == "about" {
            open_about_window(app);
        }
    });
    Ok(())
}

fn open_about_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(ABOUT_WINDOW) {
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, ABOUT_WINDOW, WebviewUrl::App("about.html".into()))
        .title("About Manga Viewer")
        .inner_size(800.0, 640.0)
        .build();
}
