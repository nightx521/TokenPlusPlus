# Token++ native compositor backdrop

Uses Windows HostBackdropBrush and a DesktopWindowTarget behind the transparent WebView. Windows owns backdrop sampling and blur; the application does not start a screen capture session or hold captured frames. It follows the actual window size and monitor DPI. `sources` counts intersecting monitors, not capture sources. The blur kernel is managed by Windows; liquid/frosted appearance is controlled by material tint.

Requires Windows 11 build 22000+ and system transparency effects. Windows may restrict effects under remote desktop or power settings. Failures are reported to the UI. The static wallpaper option remains available on older Windows versions.

The diagnostics feature paints a small controlled test window behind the actual widget and samples a few pixels within that fixture to check blur and temporal changes. It never saves screenshots. The normal release does not enable diagnostics.

MIT licensed under the repository LICENSE.
