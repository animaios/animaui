//! Dock CSS.
//!
//! Styles for the Hyprland-only dock mode: the centered bottom pill, its
//! launcher buttons, and the running-window indicator dots.

/// Return dock CSS.
pub fn css() -> String {
    r#"
/* ===== DOCK ===== */

/* Transparent window so only the pill itself is painted. */
.dock-window {
    background: transparent;
}

/* Centered bottom pill. */
.dock {
    background: var(--color-background-bar);
    border-radius: var(--radius-bar);
    padding: 6px;
}

/* Icon/content row inside the pill. */
.dock .content {
    gap: var(--dock-gap, 8px);
}

/* Launcher icon button. */
.launcher-button {
    border-radius: 9999px;
    padding: 4px;
    background: transparent;
}

.launcher-button:hover {
    background: rgba(255, 255, 255, 0.12);
}

/* Running-window indicator dot. */
.dock-running-dot {
    width: 4px;
    height: 4px;
    border-radius: 9999px;
    background: var(--color-accent, #fff);
    margin: 0 auto;
}

/* Reveal/hide transition, driven by `set_opacity`. */
.dock-window {
    transition: opacity 150ms ease;
}
"#
    .to_string()
}
