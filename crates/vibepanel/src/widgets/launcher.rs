//! Launcher widget — pinned app launchers for the Hyprland dock.
//!
//! Shows a row of icon buttons for the apps listed in the `[widgets.launcher]`
//! config section. Each button launches its `exec` command with `sh -c` on click,
//! or focuses an already-running window whose `app_id` matches the pinned entry.
//! A small running indicator dot appears beneath the icon while a matching
//! window is open.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Image, Orientation};
use tracing::{debug, warn};
use vibepanel_core::config::WidgetEntry;

use crate::services::callbacks::CallbackId;
use crate::services::compositor::WindowListSnapshot;
use crate::services::config_manager::ConfigManager;
use crate::services::icons::get_app_icon_name;
use crate::services::window_list::WindowListService;
use crate::styles::widget;
use crate::styles::{button, icon};
use crate::widgets::WidgetConfig;
use crate::widgets::base::BaseWidget;
use crate::widgets::warn_unknown_options;

/// A single pinned app entry.
#[derive(Debug, Clone)]
pub struct PinnedApp {
    /// Display name (used as the button tooltip).
    pub name: String,
    /// Shell command to launch, run via `sh -c`.
    pub exec: String,
    /// Logical icon name, resolved through the icon service.
    pub icon: String,
    /// Lowercase matching id compared against `Window.app_id`.
    pub app_id: String,
}

/// Configuration for the launcher widget.
///
/// Parsed from the `[widgets.launcher]` TOML section. The `apps` key holds an
/// array of inline tables, each describing a pinned app:
///
/// ```toml
/// [widgets.launcher]
/// apps = [
///   { name = "Firefox", exec = "firefox", icon = "firefox" },
///   { name = "Terminal", exec = "kitty", icon = "utilities-terminal" },
/// ]
/// ```
#[derive(Debug, Clone, Default)]
pub struct LauncherConfig {
    /// Pinned apps to show as launcher buttons.
    pub apps: Vec<PinnedApp>,
    /// Icon size in pixels. `None` means "use theme default" (pixmap_icon_size).
    /// Resolved to a concrete value in `LauncherWidget::new()`.
    pub icon_size: Option<i32>,
}

impl WidgetConfig for LauncherConfig {
    fn from_entry(entry: &WidgetEntry) -> Self {
        warn_unknown_options("launcher", entry, &["apps", "icon_size"]);

        let icon_size = entry
            .options
            .get("icon_size")
            .and_then(|v| v.as_integer())
            .map(|v| (v as i32).max(8));

        let apps = entry
            .options
            .get("apps")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let table = item.as_table()?;
                        let name = table.get("name").and_then(|v| v.as_str())?.to_string();
                        let exec = table.get("exec").and_then(|v| v.as_str())?.to_string();
                        let icon = table
                            .get("icon")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| name.to_lowercase());
                        // Match id defaults to the lowercased name when omitted.
                        let app_id = table
                            .get("app_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_lowercase())
                            .unwrap_or_else(|| name.to_lowercase());
                        Some(PinnedApp {
                            name,
                            exec,
                            icon,
                            app_id,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self { apps, icon_size }
    }
}

/// Per-app running indicator handle. The dot's visibility is flipped on each
/// window-list snapshot.
struct LauncherDot {
    dot: GtkBox,
    app_id: String,
}

pub struct LauncherWidget {
    base: BaseWidget,
    window_list_callback_id: CallbackId,
}

impl LauncherWidget {
    pub fn new(mut config: LauncherConfig, output_id: Option<String>) -> Self {
        let sizes = ConfigManager::global().theme_sizes();

        // Resolve icon_size: user value wins, otherwise theme default. Clamp to >= 8px.
        config.icon_size = Some(
            config
                .icon_size
                .unwrap_or(sizes.pixmap_icon_size as i32)
                .max(8),
        );
        let effective_icon = config.icon_size.unwrap();

        let base = BaseWidget::new(&[widget::LAUNCHER]);
        let content = base.content().clone();

        let dots: Rc<RefCell<Vec<LauncherDot>>> = Rc::new(RefCell::new(Vec::new()));

        // Snapshot cache for the click handler — updated by the WindowListService callback.
        let snapshot: Rc<RefCell<WindowListSnapshot>> =
            Rc::new(RefCell::new(WindowListSnapshot::default()));

        for app in &config.apps {
            let button_box = GtkBox::new(Orientation::Vertical, 2);
            button_box.set_valign(Align::Center);

            let btn = Button::new();
            btn.add_css_class(widget::LAUNCHER_BUTTON);
            btn.add_css_class(button::RESET);
            btn.add_css_class(button::COMPACT);
            btn.set_valign(Align::Center);
            btn.set_halign(Align::Center);
            btn.set_tooltip_text(Some(&app.name));

            let icon_name = get_app_icon_name(&app.icon);
            let image = Image::from_icon_name(&icon_name);
            image.add_css_class(icon::TEXT);
            image.set_pixel_size(effective_icon);
            image.set_halign(Align::Center);
            image.set_valign(Align::Center);
            image.set_tooltip_text(Some(&app.name));
            btn.set_child(Some(&image));

            // Running indicator dot — hidden until a matching window is open.
            let dot = GtkBox::new(Orientation::Horizontal, 0);
            dot.add_css_class(widget::DOCK_RUNNING_DOT);
            dot.set_halign(Align::Center);
            dot.set_size_request(4, 4);
            dot.set_visible(false);

            button_box.append(&btn);
            button_box.append(&dot);
            content.append(&button_box);

            let exec = app.exec.clone();
            let app_id = app.app_id.clone();
            let snap = snapshot.clone();
            btn.connect_clicked(move |_| {
                handle_launcher_click(&app_id, &exec, &snap);
            });

            dots.borrow_mut().push(LauncherDot {
                dot,
                app_id: app.app_id.clone(),
            });
        }

        let dots_for_cb = dots.clone();
        let snapshot_for_cb = snapshot.clone();
        let window_list_callback_id = WindowListService::global().connect(move |snap| {
            *snapshot_for_cb.borrow_mut() = snap.clone();
            update_running_dots(&dots_for_cb, snap);
        });

        debug!(
            "LauncherWidget created (output_id: {:?}, apps: {})",
            output_id,
            config.apps.len()
        );

        Self {
            base,
            window_list_callback_id,
        }
    }
    pub fn widget(&self) -> &GtkBox {
        self.base.widget()
    }
}

impl Drop for LauncherWidget {
    fn drop(&mut self) {
        WindowListService::global().disconnect(self.window_list_callback_id);
    }
}

/// On click: focus the first running window whose `app_id` matches the pinned
/// entry, otherwise spawn the configured `exec`.
fn handle_launcher_click(app_id: &str, exec: &str, snapshot: &Rc<RefCell<WindowListSnapshot>>) {
    let snapshot = snapshot.borrow();
    if let Some(window) = snapshot
        .windows
        .iter()
        .find(|w| !w.app_id.is_empty() && w.app_id.to_lowercase() == app_id.to_lowercase())
    {
        debug!(
            "launcher: focusing running window {:?} (id {}) for app_id {}",
            window.title, window.id, app_id
        );
        WindowListService::global().focus_window(window.id);
        return;
    }
    debug!("launcher: spawning {:?}", exec);
    match std::process::Command::new("sh").arg("-c").arg(exec).spawn() {
        Ok(mut child) => {
            // Reap the child asynchronously to avoid zombie processes.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => {
            warn!("launcher: failed to spawn {:?}: {e}", exec);
        }
    }
}

/// Flip each launcher dot's visibility based on whether a running window
/// matches its pinned `app_id`.
fn update_running_dots(dots: &Rc<RefCell<Vec<LauncherDot>>>, snapshot: &WindowListSnapshot) {
    let running: Vec<String> = snapshot
        .windows
        .iter()
        .filter(|w| !w.app_id.is_empty())
        .map(|w| w.app_id.to_lowercase())
        .collect();
    for entry in dots.borrow().iter() {
        let visible = running.iter().any(|id| id == &entry.app_id);
        entry.dot.set_visible(visible);
    }
}
