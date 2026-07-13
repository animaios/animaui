# Repository Guidelines

This document is for AI assistants working with the VibePanel codebase — a Rust GTK4 Wayland panel/status bar.

## Project Overview

VibePanel is a **batteries-included Wayland status bar** written in pure Rust. It replaces the status bar, notification daemon, and OSD with a single binary. It works with Hyprland, Niri, Sway, MangoWC/DWL, and other compositors.

- **Language:** Rust (edition 2024)
- **Binary:** `vibepanel` (single binary, no runtime dependencies)
- **Version:** 0.15.0 (pre-1.0, actively developed)
- **License:** MIT

## Architecture & Data Flow

```
vibepanel (binary crate)
├── src/main.rs          — CLI entry (clap), GTK app init, monitor hotplug, IPC listener
├── src/bar.rs           — Bar window (GTK4 layer-shell, edge click targets, popover anchoring)
├── src/dock.rs          — Dock window (auto-hide, icon magnification)
├── src/sectioned_bar.rs — Custom GTK widget for left/center/right layout allocation
├── src/popover_registry.rs  — Global popover open/close dispatch by name
├── src/popover_tracker.rs  — Tracks which popover is currently active
├── src/widgets/         — ~25 widget modules (clock, battery, workspaces, media, tray, …)
├── src/services/        — Singleton services with callback-based state updates
│   ├── compositor/      — Hyprland, Niri, Sway, MangoWC backends via raw socket + JSON
│   ├── network/         — NetworkManager + IWD for wifi/mobile/vpn
│   └── *.rs             — audio, battery, bluetooth, brightness, gpu, media, notifications, …
└── src/styles.rs        — CSS class/state constants

vibepanel-core (library crate)
├── src/config.rs   — TOML config parsing, validation, defaults
├── src/theme.rs    — ThemePalette (Material Design colors, luminance, wallpaper extraction)
├── src/error.rs    — thiserror-based Error enum
├── src/logging.rs  — tracing subscriber init
└── tests/config_integration.rs — Config parsing integration tests
```

### Key Patterns

**Singleton services** — Most services use `Rc<Self>` + `Rc::new(Self::new())` for global access. They register callbacks to notify UI when state changes. Pattern:

```rust
fn new() -> Rc<Self> { ... }

pub fn global() -> Rc<Self> {
    static INSTANCE: OnceLock<Rc<AudioService>> = OnceLock::new();
    INSTANCE.get_or_init(|| Self::new()).clone()
}
```

**Widget factory** — `WidgetFactory::build(entry, qs_handle, output_id)` constructs widgets from `WidgetEntry` config (match on `entry.name`). Each widget returns a `BuiltWidget` with a root `gtk4::Widget` and an optional `EdgeInteraction` for edge-click popovers.

**Layer-shell positioning** — Bar uses `gtk4_layer_shell` to anchor to screen edges. `BarPosition` enum (`Top`, `Bottom`, `Left`, `Right`) drives both bar placement and popover anchor direction.

**Compositor abstraction** — `CompositorBackend` trait + `BackendKind` enum (Hyprland, Niri, Sway, Mango, Dwl). Detected via `WAYLAND_DISPLAY` env + IPC socket detection. Hyprland uses raw socket IPC; others use JSON over socket.

**Callback registry** — `services/callbacks.rs` provides a generic `CallbackRegistry<T>` for service → widget state updates. Services hold a registry; widgets subscribe on construction.

## Key Directories

| Directory | Purpose |
|---|---|
| `crates/vibepanel/src/` | Main binary crate source |
| `crates/vibepanel/src/widgets/` | Individual widget modules |
| `crates/vibepanel/src/widgets/css/` | Per-widget CSS blocks (strings compiled into providers) |
| `crates/vibepanel/src/widgets/quick_settings/` | Quick settings panel components |
| `crates/vibepanel/src/services/` | Singleton services |
| `crates/vibepanel/src/services/compositor/` | Compositor backend implementations |
| `crates/vibepanel/src/services/network/` | Network service + NetworkManager/IWD |
| `crates/vibepanel-core/src/` | Core library (no GTK deps) |
| `crates/vibepanel-core/tests/` | Config integration tests |
| `scripts/` | UI regression runner, font subset script |
| `docs/` | Architecture doc, UI regression test guide |
| `assets/fonts/` | Subsetted Material Symbols Rounded font |

## Development Commands

### Build & Run

```bash
# Build
cargo build -p vibepanel

# Debug build + run (logs to /tmp/vibepanel-debug.log)
./run-debug.sh

# Release build
cargo build --release -p vibepanel
```

### Testing

```bash
# Unit tests
cargo test --verbose

# Clippy + tests (CI target)
cargo clippy --all-targets -- -D warnings
cargo test --verbose

# UI regression tests (requires Xvfb)
xvfb-run -a env -u GDK_BACKEND=x11 GSK_RENDERER=cairo \
  VIBEPANEL_UI_REGRESSION_REQUIRED=1 \
  cargo test -p vibepanel test_ui_regression_ -- --ignored --test-threads=1

# Or via script:
./scripts/run-ui-regression-tests.sh
```

UI regression tests are `#[test]` functions prefixed `test_ui_regression_` with `#[ignore]`, spawned as a subprocess (file-locked, Xvfb, Cairo software renderer). They compare rendered pixel output against known fixtures.

### Linting & Formatting

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

No custom clippy or rustfmt config files — uses defaults.

### Config Validation

```bash
vibepanel --check-config
vibepanel --print-example-config
```

### Font Management

```bash
# Subset Material Symbols font to used glyphs only
./scripts/subset-font.sh

# Check glyph manifest is up to date
./scripts/subset-font.sh --check
```

## Code Conventions

### Error Handling
- `vibepanel-core/src/error.rs` — `thiserror` enum `Error` with `Result<T> = std::result::Result<T, Error>`
- Binary crate uses `anyhow::Result<T>` for fallible operations that don't need typed errors
- Never use `unwrap()` in production code (use `?` or `.context()`)

### Logging
- `tracing` crate throughout (info, debug, warn, error)
- `vibepanel-core/src/logging.rs::init(verbosity: u8)` — sets up env-filter subscriber
- Verbosity: `-v` = info, `-vv` = debug, `-vvv` = trace

### GTK Patterns
- GTK4 only, with `gtk4_layer_shell` for Wayland layer-shell
- Custom `SectionedBar` widget handles left/center/right allocation
- `BaseWidget` in `widgets/base.rs` provides the common root `gtk4::Box` with CSS classes
- `MenuHandle` wraps `LayerShellPopover` (not plain GTK `Popover`) for proper keyboard focus and ESC/click-outside dismiss
- Widget configs implement `From<&WidgetEntry>` trait

### Module Visibility
- `pub` = public API (core crate types, widget constructors)
- `pub(crate)` = crate-internal (widget internals, service internals)
- `pub mod` only for `launcher`, `layer_shell_popover`, `css`, `quick_settings` (used across modules)
- No `mod` visibility in main crate (everything is either `pub(crate)` or fully public)

### Naming
- Widget module names match config widget names: `clock.rs` → `"clock"`, `quick_settings.rs` → `"quick_settings"`
- Service structs: `FooService` (e.g., `BatteryService`, `CompositorManager`)
- Config structs: `FooConfig` (e.g., `ClockConfig`, `TrayConfig`)
- Test functions: `snake_case` with descriptive names, prefixed `test_` (standard Rust)

### Threading
- GTK operations on main thread only
- Service IPC/background threads communicate via `parking_lot::Mutex`, `std::sync::RwLock`, `std::sync::atomic`
- No `async/await` for service communication (channels are sync)
- `async-channel` for Cava (audio visualizer) subprocess communication

### Dependency Declaration
- All shared deps in `[workspace.dependencies]` in root `Cargo.toml`
- Child crates use `depname = { workspace = true }`
- Version metadata in `[workspace.package]` — single source of truth

## Important Files

| File | Why it matters |
|---|---|
| `Cargo.toml` | All dep versions, workspace members, edition 2024 |
| `crates/vibepanel/src/main.rs` | CLI (clap), app init, monitor hotplug, IPC listener |
| `crates/vibepanel/src/bar.rs` | Bar window creation, layer-shell setup, edge clicks |
| `crates/vibepanel/src/dock.rs` | Dock window, auto-hide, magnification |
| `crates/vibepanel/src/widgets/mod.rs` | WidgetFactory, WidgetConfig trait, all widget exports |
| `crates/vibepanel/src/widgets/base.rs` | BaseWidget, MenuHandle, common widget helpers |
| `crates/vibepanel-core/src/config.rs` | Config load/validate/defaults, WidgetEntry parsing |
| `crates/vibepanel-core/src/theme.rs` | ThemePalette, material color extraction, CSS var generation |
| `crates/vibepanel/src/services/compositor/factory.rs` | `BackendKind` enum, compositor detection |
| `crates/vibepanel/src/services/config_manager.rs` | Global config access, CSS hot-reload, theme callbacks |
| `crates/vibepanel/src/popover_registry.rs` | Global popover dispatch by string name |
| `config.toml` | Example config (also the real dev config) |
| `.github/workflows/ci.yml` | CI pipeline — fmt, clippy, test, ui-regression, font-check |

## Runtime & Tooling Preferences

### Rust
- **Edition:** 2024 (unstable edition, not 2021)
- **No MSRV pinned** — CI uses `rust:trixie` container (rolling stable)
- **No `rust-toolchain` file**

### System Dependencies (for build)
```
libgtk-4-dev, libgtk4-layer-shell-dev, libpulse-dev, libudev-dev, libdbus-1-dev
```
On Arch: `gtk4`, `gtk4-layer-shell`, `pulseaudio`, `udev`, `dbus`
On Debian/Ubuntu: listed above
On Fedora: `gtk4-devel`, `gtk4-layer-shell-devel`, `pulseaudio-libs-devel`, `systemd-devel`, `dbus-devel`

### Runtime Dependencies (for run)
- Wayland compositor (Hyprland, Niri, Sway, etc.)
- D-Bus session bus (for battery, bluetooth, network, notifications)
- PulseAudio or PipeWire (for audio control)
- udev (for brightness/backlight discovery)

### Tool Versions (CI-confirmed working)
- Rust: latest stable (from `rust:trixie`)
- GTK4: 0.10
- gtk4-layer-shell: 0.7
- libpulse-binding: 2.28

### Build Targets
- x86_64-unknown-linux-gnu (native)
- aarch64-unknown-linux-gnu (cross-compiled)

## Testing & QA

### Test Types

1. **Unit tests** — `#[test]` functions in `mod tests { ... }` blocks or `#[cfg(test)]` modules
   - Run: `cargo test --verbose`
   - Examples: `layout_math.rs`, `sectioned_bar_tests.rs`, `bar_tests.rs`, `config_integration.rs`

2. **UI regression tests** — `#[test]` functions prefixed `test_ui_regression_` with `#[ignore]`
   - Spawned as subprocesses under Xvfb with `GSK_RENDERER=cairo`
   - Pixel-snapshot comparison against fixture images
   - Run: `cargo test -p vibepanel test_ui_regression_ -- --ignored --test-threads=1` (with Xvfb)

3. **Contract tests** — `#[test]` functions prefixed `run_layer_shell_*_contract` (in `bar_tests.rs`, `osd_tests.rs`)
   - Verify layer-shell positioning and edge-click behavior
   - Also run as ignored subprocesses

### CI Pipeline

```
fmt (cargo fmt --check)
  → clippy + test (cargo clippy --all-targets -- -D warnings && cargo test --verbose)
    → ui-regression-tests (Xvfb + script)
      → font-check (scripts/subset-font.sh --check)
```

RUSTFLAGS: `-Dwarnings` (warnings become errors in CI)

### Pre-commit Hooks
`.cargo-husky/hooks/pre-commit` runs: tests, clippy, fmt, ui-regression (if xvfb available)