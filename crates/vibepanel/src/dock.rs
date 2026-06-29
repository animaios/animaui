//! Dock window implementation using GTK4 and layer-shell.
//!
//! The dock is a centered bottom "pill" that shows pinned launchers and running
//! windows. It auto-hides (Dash-to-Dock style) and reappears when the mouse hits
//! the bottom screen edge. Hyprland-only (gated in `create_bar_window`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib::{self, SourceId};
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, EventControllerMotion, Orientation,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use tracing::{debug, info, warn};
use vibepanel_core::Config;

use crate::services::bar_manager::BarManager;
use crate::services::compositor::CompositorManager;
use crate::widgets::BarState;

/// Build the dock content: a centered horizontal pill hosting the configured
/// widgets (launchers + taskbar).
pub(crate) fn build_dock_content(
    app: &Application,
    config: &Config,
    state: &mut BarState,
    output_id: Option<&str>,
) -> GtkBox {
    let content = GtkBox::new(Orientation::Horizontal, config.dock.gap as i32);
    content.add_css_class(crate::styles::class::DOCK);
    content.set_halign(Align::Center);
    content.set_valign(Align::End);
    content.set_margin_top(0);
    content.set_margin_bottom(0);

    // Build the configured widgets (launchers, taskbar, etc.) into the pill.
    // Reuse the bar's section factory so widget ordering/merging behaves identically.
    let qs = crate::widgets::QuickSettingsWindowHandle::new(
        app.clone(),
        crate::widgets::QuickSettingsConfig::default(),
    );
    let left_section = crate::bar::create_section(
        "left",
        config,
        state,
        &qs,
        output_id,
        Orientation::Horizontal,
        &Rc::new(RefCell::new(Vec::new())),
    );
    content.append(&left_section);

    if !config.widgets.resolved_center().is_empty() {
        let center_section = crate::bar::create_center_section(
            config,
            state,
            &qs,
            output_id,
            Orientation::Horizontal,
            &Rc::new(RefCell::new(Vec::new())),
        );
        content.append(&center_section);
    }

    let right_section = crate::bar::create_section(
        "right",
        config,
        state,
        &qs,
        output_id,
        Orientation::Horizontal,
        &Rc::new(RefCell::new(Vec::new())),
    );
    content.append(&right_section);

    content
}

/// Create and configure the dock window with layer-shell.
///
/// The `state` parameter stores widget handles, keeping them alive for the
/// lifetime of the dock. The `output_id` is the monitor connector name used
/// for per-monitor widget filtering.
pub fn create_dock_window(
    app: &Application,
    config: &Config,
    monitor: &gtk4::gdk::Monitor,
    output_id: &str,
    state: &mut BarState,
) -> ApplicationWindow {
    // Hyprland-only gating: fall back to a bottom bar on other compositors.
    if CompositorManager::global().backend_name() != "Hyprland" {
        warn!("bar.mode = \"dock\" requires Hyprland; falling back to a bottom bar");
        let mut cfg = config.clone();
        cfg.bar.mode = "bar".to_string();
        cfg.bar.position = "bottom".to_string();
        return crate::bar::create_bar_window(app, &cfg, monitor, output_id, state);
    }

    let dock_height = config.dock.icon_size + 12; // icon + vertical padding

    let window = ApplicationWindow::builder()
        .application(app)
        .title("vibepanel-dock")
        .decorated(false)
        .resizable(false)
        .default_height(dock_height as i32)
        .default_width(-1)
        .build();

    window.add_css_class(crate::styles::class::DOCK_WINDOW);

    // Initialize layer-shell
    window.init_layer_shell();
    window.set_namespace(Some("vibepanel-dock"));
    window.set_layer(Layer::Top);

    // Bind to specific monitor
    window.set_monitor(Some(monitor));
    debug!("Dock bound to monitor: {:?}", monitor.connector());

    // Anchor to the bottom edge and stretch across the full width.
    window.set_anchor(Edge::Top, false);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    // Reserve space only if the user explicitly pinned the dock to the edge.
    if config.dock.pin_to_edge {
        window.auto_exclusive_zone_enable();
    }

    // Gap from the bottom edge
    window.set_margin(Edge::Bottom, config.bar.screen_margin as i32);

    // Dock doesn't need keyboard input
    window.set_keyboard_mode(KeyboardMode::None);

    let content = build_dock_content(app, config, state, Some(output_id));
    window.set_child(Some(&content));

    // Set window width to the target monitor's width on map.
    let target_geometry = monitor.geometry();
    let target_width = target_geometry.width();

    window.connect_map(move |win| {
        win.set_default_size(target_width, dock_height as i32);
        debug!(
            "Set dock window size to target monitor width: {}px",
            target_width
        );
    });

    // Auto-hide wiring (Dash-to-Dock style).
    if !config.dock.always_visible && config.dock.autohide {
        install_dock_auto_hide(app, config, &window, monitor, state);
    } else {
        window.set_visible(true);
    }

    info!(
        "Dock window created: icon_size={}px, monitor={:?}, widgets={}",
        config.dock.icon_size,
        monitor.connector(),
        state.handle_count()
    );

    window
}

/// Install auto-hide behavior: a thin hotzone at the bottom edge reveals the dock
/// on mouse enter; the dock hides after the mouse leaves for ~200ms.
fn install_dock_auto_hide(
    app: &Application,
    config: &Config,
    dock: &ApplicationWindow,
    monitor: &gtk4::gdk::Monitor,
    state: &mut BarState,
) {
    // Start hidden.
    dock.set_opacity(0.0);
    dock.set_visible(false);

    // --- Hotzone: a thin transparent surface at the bottom edge that catches
    // mouse-enter to reveal the dock.
    let hotzone = ApplicationWindow::builder()
        .application(app)
        .title("vibepanel-dock-hotzone")
        .decorated(false)
        .resizable(false)
        .default_height(3)
        .default_width(-1)
        .build();
    hotzone.init_layer_shell();
    hotzone.set_namespace(Some("vibepanel-dock-hotzone"));
    hotzone.set_layer(Layer::Top);
    hotzone.set_monitor(Some(monitor));
    hotzone.set_anchor(Edge::Top, false);
    hotzone.set_anchor(Edge::Bottom, true);
    hotzone.set_anchor(Edge::Left, true);
    hotzone.set_anchor(Edge::Right, true);
    hotzone.set_margin(Edge::Bottom, config.bar.screen_margin as i32);
    hotzone.set_keyboard_mode(KeyboardMode::None);
    hotzone.set_opacity(0.01); // near-invisible but still receives input
    hotzone.add_css_class(crate::styles::class::DOCK_WINDOW);

    let hide_timer: Rc<Cell<Option<SourceId>>> = Rc::new(Cell::new(None));
    let hide_timer_for_hotzone = Rc::clone(&hide_timer);
    let dock_for_hotzone = dock.clone();

    // Hotzone enter → reveal dock.
    let hotzone_motion = EventControllerMotion::new();
    hotzone_motion.connect_enter(move |_, _, _| {
        // Don't reveal if bars are IPC-hidden.
        if BarManager::global().is_hidden() {
            return;
        }
        if let Some(src) = hide_timer_for_hotzone.take() {
            src.remove();
        }
        dock_for_hotzone.set_visible(true);
        dock_for_hotzone.set_opacity(1.0);
    });
    hotzone.add_controller(hotzone_motion);
    hotzone.set_visible(true);

    // Dock enter → cancel pending hide.
    let dock_for_enter = dock.clone();
    let hide_timer_for_enter = Rc::clone(&hide_timer);
    let dock_motion = EventControllerMotion::new();
    dock_motion.connect_enter(move |_, _, _| {
        if let Some(src) = hide_timer_for_enter.take() {
            src.remove();
        }
        dock_for_enter.set_opacity(1.0);
    });

    // Dock leave → schedule hide after 200ms.
    let dock_for_leave = dock.clone();
    let hide_timer_for_leave = Rc::clone(&hide_timer);
    dock_motion.connect_leave(move |_| {
        // Cancel any existing timer before scheduling a new one.
        if let Some(src) = hide_timer_for_leave.take() {
            src.remove();
        }
        let dock = dock_for_leave.clone();
        let timer = Rc::clone(&hide_timer_for_leave);
        let src = glib::timeout_add_local_once(Duration::from_millis(200), move || {
            timer.set(None);
            dock.set_opacity(0.0);
            dock.set_visible(false);
        });
        hide_timer_for_leave.set(Some(src));
    });
    dock.add_controller(dock_motion);

    // Keep the hotzone + timer alive in state.
    state.add_handle(Box::new(hotzone));
    state.add_handle(Box::new(DockAutoHideState(hide_timer)));
}

/// Opaque handle that keeps the auto-hide timer alive for the dock's lifetime.
#[allow(dead_code)]
struct DockAutoHideState(Rc<Cell<Option<SourceId>>>);

// Ensure the handle is Send + Sync for BarState's Vec<Box<dyn Any>>.
unsafe impl Send for DockAutoHideState {}
unsafe impl Sync for DockAutoHideState {}
