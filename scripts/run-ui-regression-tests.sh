#!/usr/bin/env sh
set -eu

if ! command -v xvfb-run >/dev/null 2>&1; then
    printf '%s\n' "error: xvfb-run is required for GTK UI regression tests"
    printf '%s\n' "install Xvfb so UI regression tests can run without presenting desktop windows"
    exit 1
fi

# Run only the ignored GTK UI regression wrappers. Internal runners are invoked
# by those wrappers and layer-shell contracts still need a real Wayland compositor.
# Use Cairo under Xvfb to avoid hardware-dependent Vulkan device loss failures.
: "${GSK_RENDERER:=cairo}"
export GSK_RENDERER
echo '+xvfb-run -a env -u WAYLAND_DISPLAY GDK_BACKEND=x11 GSK_RENDERER='"$GSK_RENDERER"' VIBEPANEL_UI_REGRESSION_REQUIRED=1 cargo test -p vibepanel test_ui_regression_ -- --ignored --test-threads=1'
xvfb-run -a env -u WAYLAND_DISPLAY GDK_BACKEND=x11 GSK_RENDERER="$GSK_RENDERER" VIBEPANEL_UI_REGRESSION_REQUIRED=1 cargo test -p vibepanel test_ui_regression_ -- --ignored --test-threads=1
