//! The widget is a small child of Explorer's desktop host, above its icon view.
//! Only our own window is reparented; the shell's windows and wallpaper are not changed.
use crate::{model::Settings, AppState};
use base64::Engine;
use serde::Serialize;
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMNCRP_USEWINDOWSTYLE,
            DWMWA_NCRENDERING_POLICY,
        },
        Graphics::Gdi::{
            ClientToScreen, CreateRectRgn, CreateRoundRectRgn, DeleteObject, GetWindowRgn,
            ScreenToClient, SetWindowRgn,
        },
        UI::WindowsAndMessaging::*,
    },
};

#[derive(Default)]
pub struct DesktopState {
    pub attached: Mutex<bool>,
    live: Mutex<Option<token_glass_runtime::Renderer>>,
    original_styles: Mutex<Option<(isize, isize)>>,
    geometry: Mutex<Option<WidgetGeometry>>,
    pub wallpaper: Mutex<Option<WallpaperCache>>,
}
#[derive(Clone, Copy, Serialize, Debug)]
struct WidgetGeometry {
    width: f64,
    height: f64,
    radius: f64,
}
fn win10_compatibility() -> bool {
    windows_version::OsVersion::current().build < 22000
}
fn nonclient_policy(hwnd: HWND, desktop: bool) -> Result<(), String> {
    let policy = if desktop {
        DWMNCRP_DISABLED
    } else {
        DWMNCRP_USEWINDOWSTYLE
    };
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            &policy as *const _ as _,
            std::mem::size_of_val(&policy) as u32,
        )
    };
    if result < 0 {
        return Err("无法更新系统边框绘制策略".into());
    }
    Ok(())
}
#[cfg(test)]
fn clean_styles(style: isize, ex: isize) -> (isize, isize) {
    (
        (style & !((WS_POPUP | WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME) as isize))
            | WS_CHILD as isize,
        ex & !((WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE)
            as isize),
    )
}
fn region_dimensions(g: WidgetGeometry, scale: f64) -> (i32, i32, i32) {
    (
        (g.width * scale).ceil() as i32,
        (g.height * scale).ceil() as i32,
        (g.radius * scale * 2.).round() as i32,
    )
}
fn set_region(
    hwnd: HWND,
    geometry: WidgetGeometry,
    scale: f64,
    enabled: bool,
) -> Result<(), String> {
    unsafe {
        if !enabled {
            SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
            return Ok(());
        }
        let (width, height, diameter) = region_dimensions(geometry, scale);
        let mut window = RECT::default();
        let mut origin = POINT::default();
        if GetWindowRect(hwnd, &mut window) == 0 || ClientToScreen(hwnd, &mut origin) == 0 {
            return Err("无法读取圆角边界".into());
        }
        let x = origin.x - window.left;
        let y = origin.y - window.top;
        let region = CreateRoundRectRgn(x, y, x + width, y + height, diameter, diameter);
        if region.is_null() {
            return Err("无法建立桌面圆角区域".into());
        }
        if SetWindowRgn(hwnd, region, 1) == 0 {
            DeleteObject(region);
            return Err("无法应用桌面圆角区域".into());
        }
        // On success the system owns region; never delete it here.
    }
    Ok(())
}
pub fn sync_region(w: &tauri::WebviewWindow) -> Result<(), String> {
    if let Some(geometry) = *w
        .app_handle()
        .state::<DesktopState>()
        .geometry
        .lock()
        .map_err(|_| "桌面尺寸暂不可用")?
    {
        set_region(
            handle(w)?,
            geometry,
            w.scale_factor().map_err(|_| "无法读取显示缩放")?,
            win10_compatibility()
                || w.app_handle()
                    .state::<DesktopState>()
                    .live
                    .lock()
                    .map(|g| g.is_some())
                    .unwrap_or(false),
        )?;
    }
    Ok(())
}

pub struct WallpaperCache {
    path: PathBuf,
    modified: Option<std::time::SystemTime>,
    pub revision: String,
    data: String,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn find_host(hwnd: HWND, result: LPARAM) -> BOOL {
    let view = FindWindowExW(
        hwnd,
        std::ptr::null_mut(),
        wide("SHELLDLL_DefView").as_ptr(),
        std::ptr::null(),
    );
    if !view.is_null() {
        *(result as *mut HWND) = hwnd;
        return 0;
    }
    1
}
pub unsafe fn desktop_host() -> HWND {
    let mut host = std::ptr::null_mut();
    EnumWindows(Some(find_host), &mut host as *mut HWND as LPARAM);
    host
}
fn handle(w: &tauri::WebviewWindow) -> Result<HWND, String> {
    Ok(w.hwnd().map_err(|_| "无法取得窗口句柄")?.0 as HWND)
}

pub fn is_attached(w: &tauri::WebviewWindow) -> bool {
    let Ok(hwnd) = handle(w) else {
        return false;
    };
    unsafe {
        let parent = GetParent(hwnd);
        !parent.is_null() && IsWindow(parent) != 0 && parent == desktop_host()
    }
}
fn pin_live(w: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = handle(w)?;
    unsafe {
        let host = desktop_host();
        if host.is_null() {
            return Err("桌面层暂不可用".into());
        }
        let mut above = GetWindow(host, GW_HWNDPREV);
        while above == hwnd {
            above = GetWindow(above, GW_HWNDPREV);
        }
        SetWindowPos(
            hwnd,
            if above.is_null() { HWND_TOP } else { above },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    Ok(())
}
fn attach_live(w: &tauri::WebviewWindow) -> Result<(), String> {
    attach_owned(w, true)
}
fn attach_owned(w: &tauri::WebviewWindow, native: bool) -> Result<(), String> {
    let hwnd = handle(w)?;
    let state = w.app_handle().state::<DesktopState>();
    unsafe {
        let host = desktop_host();
        if host.is_null() {
            return Err("未找到 Windows 桌面层".into());
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let mut affinity = 0;
        GetWindowDisplayAffinity(hwnd, &mut affinity);
        if GetWindow(hwnd, GW_OWNER) == host
            && style & ((WS_CHILD | WS_CAPTION | WS_THICKFRAME | WS_CLIPCHILDREN) as isize) == 0
            && affinity == 0
            && (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_NOREDIRECTIONBITMAP as isize != 0)
                == native
        {
            return pin_live(w);
        }
        let mut saved = state
            .original_styles
            .lock()
            .map_err(|_| "桌面样式暂不可用")?;
        if saved.is_none() {
            *saved = Some((
                GetWindowLongPtrW(hwnd, GWL_STYLE),
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
            ));
        }
        if GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_CHILD as isize != 0 {
            SetParent(hwnd, std::ptr::null_mut());
        }
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            (GetWindowLongPtrW(hwnd, GWL_STYLE)
                & !((WS_CHILD | WS_CAPTION | WS_THICKFRAME | WS_CLIPCHILDREN) as isize))
                | WS_POPUP as isize,
        );
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (GetWindowLongPtrW(hwnd, GWL_EXSTYLE)
                & !((WS_EX_APPWINDOW | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE) as isize))
                | ((WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) as isize)
                | if native {
                    WS_EX_NOREDIRECTIONBITMAP as isize
                } else {
                    0
                },
        );
        SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, host as isize);
        SetWindowDisplayAffinity(hwnd, 0);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    pin_live(w)
}
#[tauri::command]
pub fn desktop_live_status(app: tauri::AppHandle) -> serde_json::Value {
    let state = app.state::<DesktopState>();
    let live = state.live.lock().unwrap();
    if let Some(r) = live.as_ref() {
        let s = r.status();
        serde_json::json!({"active":true,"backend":"native-compositor","capturing":false,"ready":s.ready,"sources":s.sources,"error":s.error})
    } else {
        serde_json::json!({"active":false,"backend":"native-compositor","capturing":false,"ready":false,"sources":0,"error":null})
    }
}
#[tauri::command]
pub async fn desktop_probe_pixels(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(feature = "glass-probe")]
    {
        let w = app.get_webview_window("main").ok_or("窗口不存在")?;
        let raw = handle(&w)? as isize;
        let result = tauri::async_runtime::spawn_blocking(move || {
            token_glass_runtime::test_visible_pattern(raw).map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "像素验证退出")?;
        pin_live(&w)?;
        result
    }
    #[cfg(not(feature = "glass-probe"))]
    {
        let _ = app;
        Err("此构建未启用像素验证".into())
    }
}

pub fn attach(w: &tauri::WebviewWindow) -> Result<(), String> {
    attach_owned(w, false)
}
pub fn detach(w: &tauri::WebviewWindow) -> Result<(), String> {
    let renderer = w
        .app_handle()
        .state::<DesktopState>()
        .live
        .lock()
        .map_err(|_| "实时背景不可用")?
        .take();
    drop(renderer);
    let hwnd = handle(w)?;
    unsafe {
        SetWindowDisplayAffinity(hwnd, 0);
        if GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_CHILD as isize == 0 {
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
        }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect);
        SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
        if GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_CHILD as isize != 0 {
            SetParent(hwnd, std::ptr::null_mut());
        }
        if win10_compatibility() {
            nonclient_policy(hwnd, false)?;
        }
        let state = w.app_handle().state::<DesktopState>();
        let saved = state
            .original_styles
            .lock()
            .map_err(|_| "桌面样式暂不可用")?
            .take();
        let style = saved
            .map(|s| s.0)
            .unwrap_or_else(|| GetWindowLongPtrW(hwnd, GWL_STYLE));
        if let Some((_, ex)) = saved {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        }
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            (style & !(WS_CHILD as isize)) | WS_POPUP as isize,
        );
        SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            rect.left,
            rect.top,
            0,
            0,
            SWP_NOSIZE | SWP_FRAMECHANGED | SWP_NOACTIVATE,
        );
    }
    if let Ok(mut a) = w.app_handle().state::<DesktopState>().attached.lock() {
        *a = false;
    }
    Ok(())
}

pub fn screen_bounds(w: &tauri::WebviewWindow) -> Result<(i32, i32, u32, u32), String> {
    let hwnd = handle(w)?;
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
        return Err("无法读取窗口位置".into());
    }
    Ok((
        r.left,
        r.top,
        (r.right - r.left).max(1) as u32,
        (r.bottom - r.top).max(1) as u32,
    ))
}
fn put_position(w: &tauri::WebviewWindow, x: i32, y: i32) -> Result<(), String> {
    let hwnd = handle(w)?;
    unsafe {
        let mut point = POINT { x, y };
        let parent = GetParent(hwnd);
        if !parent.is_null() && GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_CHILD as isize != 0 {
            ScreenToClient(parent, &mut point);
        }
        if SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            point.x,
            point.y,
            0,
            0,
            SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
        ) == 0
        {
            return Err("移动窗口失败".into());
        }
    }
    Ok(())
}
fn visible_position(w: &tauri::WebviewWindow, pos: Option<(i32, i32)>) -> (i32, i32) {
    if let Ok(monitors) = w.available_monitors() {
        if let Some((x, y)) = pos {
            if monitors.iter().any(|m| {
                let p = m.position();
                let s = m.size();
                x >= p.x
                    && y >= p.y
                    && x + 80 < p.x + s.width as i32
                    && y + 60 < p.y + s.height as i32
            }) {
                return (x, y);
            }
        }
        if let Some(m) = monitors.first() {
            return (m.position().x + 60, m.position().y + 80);
        }
    }
    (60, 80)
}
fn panel_bounds(
    w: &tauri::WebviewWindow,
    bounds: Option<(i32, i32, u32, u32)>,
) -> (i32, i32, u32, u32) {
    let monitors = w.available_monitors().unwrap_or_default();
    let desired = bounds.unwrap_or((80, 80, 460, 760));
    let chosen = monitors
        .iter()
        .find(|m| {
            let p = m.position();
            let s = m.size();
            desired.0 >= p.x
                && desired.1 >= p.y
                && desired.0 < p.x + s.width as i32
                && desired.1 < p.y + s.height as i32
        })
        .or_else(|| monitors.first());
    if let Some(m) = chosen {
        let p = m.position();
        let s = m.size();
        let scale = m.scale_factor();
        let width = desired.2.max((380. * scale) as u32).min(s.width);
        let height = desired.3.max((570. * scale) as u32).min(s.height);
        (
            desired.0.clamp(p.x, p.x + (s.width - width) as i32),
            desired.1.clamp(p.y, p.y + (s.height - height) as i32),
            width,
            height,
        )
    } else {
        (80, 80, 460, 760)
    }
}
pub fn apply(w: &tauri::WebviewWindow, old: &Settings, new: &Settings) -> Result<(), String> {
    let entering = new.desktop_mode
        && (!old.desktop_mode || old.live_background != new.live_background || !is_attached(w));
    if new.desktop_mode {
        // Framework setters may reconstruct native styles. Finish them before reparenting.
        if entering {
            if is_attached(w) {
                detach(w)?;
            }
            w.set_always_on_top(false).map_err(|_| "无法设置桌面层级")?;
            w.set_skip_taskbar(true).map_err(|_| "无法隐藏任务栏按钮")?;
            w.set_resizable(false).map_err(|_| "无法调整窗口模式")?;
            w.set_shadow(false).map_err(|_| "无法调整窗口阴影")?;
            let geometry = *w
                .app_handle()
                .state::<DesktopState>()
                .geometry
                .lock()
                .map_err(|_| "桌面尺寸不可用")?;
            let (width, height) = geometry
                .map(|g| (g.width, g.height.max(180.)))
                .unwrap_or((350., 280.));
            w.set_size(tauri::LogicalSize::new(width, height))
                .map_err(|_| "无法调整桌面尺寸")?;
        }
        let webview: &tauri::Webview = w.as_ref();
        webview
            .set_background_color(Some(tauri::utils::config::Color(0, 0, 0, 0)))
            .map_err(|_| "无法设置透明背景")?;
        w.show().map_err(|_| "无法显示桌面组件")?;
        // Last writer: show()/tray restoration can otherwise reintroduce native frames.
        if let Err(error) = if new.live_background {
            attach_live(w)
        } else {
            attach(w)
        } {
            let _ = detach(w);
            let _ = w.set_skip_taskbar(false);
            let _ = w.set_resizable(true);
            let _ = w.set_shadow(true);
            let scale = w.scale_factor().unwrap_or(1.);
            let (x, y, width, height) =
                new.panel_bounds
                    .unwrap_or((80, 80, (460. * scale) as u32, (760. * scale) as u32));
            let _ = w.set_size(tauri::PhysicalSize::new(width, height));
            let _ = put_position(w, x, y);
            let _ = w.set_always_on_top(old.always_on_top);
            return Err(error);
        }
        if entering {
            let (x, y) = visible_position(w, new.desktop_position);
            put_position(w, x, y)?;
        }
        if new.live_background {
            let native_state = w.app_handle().state::<DesktopState>();
            let mut renderer = native_state.live.lock().map_err(|_| "实时背景不可用")?;
            let blur = if new.glass == "liquid" { 8. } else { 18. };
            if let Some(r) = renderer.as_ref() {
                r.set_blur(blur);
            } else {
                *renderer = Some(token_glass_runtime::Renderer::start(
                    handle(w)? as isize,
                    blur,
                ));
            }
        }
        sync_region(w)?;
    } else {
        if old.desktop_mode || is_attached(w) {
            detach(w)?;
            w.set_resizable(true).map_err(|_| "无法恢复窗口")?;
            let (x, y, width, height) = panel_bounds(w, new.panel_bounds);
            w.set_size(tauri::PhysicalSize::new(width, height))
                .map_err(|_| "无法恢复面板尺寸")?;
            let (x, y) = visible_position(w, Some((x, y)));
            put_position(w, x, y)?;
        }
        w.set_skip_taskbar(false)
            .map_err(|_| "无法恢复任务栏按钮")?;
        w.set_shadow(true).map_err(|_| "无法恢复窗口阴影")?;
        w.set_always_on_top(new.always_on_top)
            .map_err(|_| "无法设置窗口置顶")?;
    }
    Ok(())
}

#[tauri::command]
pub fn resize_widget(
    app: tauri::AppHandle,
    height: f64,
    width: f64,
    radius: Option<f64>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    if !state
        .store
        .lock()
        .map_err(|_| "设置不可用")?
        .settings
        .desktop_mode
    {
        return Ok(());
    }
    let w = app.get_webview_window("main").ok_or("窗口不存在")?;
    let radius = radius.unwrap_or(20.);
    if !height.is_finite()
        || !width.is_finite()
        || !radius.is_finite()
        || !(0.0..=64.0).contains(&radius)
    {
        return Err("桌面尺寸无效".into());
    }
    let geometry = WidgetGeometry {
        width: width.clamp(320., 520.),
        height: height.clamp(1., 900.),
        radius,
    };
    *app.state::<DesktopState>()
        .geometry
        .lock()
        .map_err(|_| "桌面尺寸暂不可用")? = Some(geometry);
    w.set_size(tauri::LogicalSize::new(
        geometry.width,
        geometry.height.max(180.),
    ))
    .map_err(|_| "调整组件尺寸失败")?;
    // set_size normally keeps style bits; reassert the desktop boundary if the framework changed them.
    let live = state
        .store
        .lock()
        .map_err(|_| "设置不可用")?
        .settings
        .live_background;
    if live {
        attach_live(&w)?;
    } else {
        attach(&w)?;
    }
    sync_region(&w)
}

#[tauri::command]
pub fn desktop_frame(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let w = app.get_webview_window("main").ok_or("窗口不存在")?;
    let hwnd = handle(&w)?;
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);

        let r = CreateRectRgn(0, 0, 0, 0);
        let region = GetWindowRgn(hwnd, r) > 0;
        DeleteObject(r);
        let mut affinity = 0;
        GetWindowDisplayAffinity(hwnd, &mut affinity);
        Ok(
            serde_json::json!({"attached":is_attached(&w),"frameClean":style & ((WS_CHILD|WS_CAPTION|WS_THICKFRAME|WS_CLIPCHILDREN) as isize)==0,"nativeRegion":region,"win10Compatibility":win10_compatibility(),"nativeDpi":windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd),"tauriScale":w.scale_factor().unwrap_or(1.),"bounds":screen_bounds(&w).ok(),"awareness":windows_sys::Win32::UI::HiDpi::GetAwarenessFromDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::GetWindowDpiAwarenessContext(hwnd)),"captureExcluded":affinity==0x11}),
        )
    }
}

#[tauri::command]
pub fn desktop_drag(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state
        .store
        .lock()
        .map_err(|_| "设置不可用")?
        .settings
        .clone();
    if !settings.desktop_mode || settings.position_locked {
        return Ok(());
    }
    let w = app.get_webview_window("main").ok_or("窗口不存在")?;
    // A native screen-coordinate drag remains reliable for an Explorer child window.
    let hwnd = handle(&w)?;
    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor);
    }
    let (x, y, _, _) = screen_bounds(&w)?;
    let scale = w.scale_factor().unwrap_or(1.);
    let offset = ((cursor.x - x) as f64 / scale, (cursor.y - y) as f64 / scale);
    let hwnd_id = hwnd as usize;
    let handle = app.clone();
    std::thread::spawn(move || {
        for frame in 0..3000 {
            unsafe {
                if windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(1) >= 0 {
                    break;
                }
                GetCursorPos(&mut cursor);
            }
            let native_hwnd = hwnd_id as HWND;
            unsafe {
                let mut p = POINT {
                    x: token_glass_runtime::drag_origin(
                        (cursor.x, cursor.y),
                        offset,
                        windows_sys::Win32::UI::HiDpi::GetDpiForWindow(native_hwnd) as f64 / 96.,
                    )
                    .0,
                    y: token_glass_runtime::drag_origin(
                        (cursor.x, cursor.y),
                        offset,
                        windows_sys::Win32::UI::HiDpi::GetDpiForWindow(native_hwnd) as f64 / 96.,
                    )
                    .1,
                };
                let parent = GetParent(native_hwnd);
                if !parent.is_null()
                    && GetWindowLongPtrW(native_hwnd, GWL_STYLE) & WS_CHILD as isize != 0
                {
                    ScreenToClient(parent, &mut p);
                }
                SetWindowPos(
                    native_hwnd,
                    std::ptr::null_mut(),
                    p.x,
                    p.y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            if frame % 2 == 0 {
                if let Some(window) = handle.get_webview_window("main") {
                    if let Ok(placement) = placement(&window) {
                        let _ = handle.emit("desktop-placement", placement);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        if let Some(window) = handle.get_webview_window("main") {
            if let Ok(placement) = placement(&window) {
                let _ = handle.emit("desktop-placement", placement);
            }
        }
        let _ = handle.emit("desktop-moved", ());
    });
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
fn placement(w: &tauri::WebviewWindow) -> Result<Placement, String> {
    let (x, y, _, _) = screen_bounds(w)?;
    let monitor = w
        .current_monitor()
        .map_err(|_| "无法读取显示器")?
        .ok_or("未找到显示器")?;
    let sf = w.scale_factor().unwrap_or(1.);
    Ok(Placement {
        x: (x - monitor.position().x) as f64 / sf,
        y: (y - monitor.position().y) as f64 / sf,
        width: monitor.size().width as f64 / sf,
        height: monitor.size().height as f64 / sf,
    })
}
#[tauri::command]
pub fn desktop_placement(app: tauri::AppHandle) -> Result<Placement, String> {
    placement(&app.get_webview_window("main").ok_or("窗口不存在")?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Background {
    revision: String,
    data_url: Option<String>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    pub attached: bool,
}
#[tauri::command]
pub fn desktop_background(
    app: tauri::AppHandle,
    known_revision: String,
) -> Result<Background, String> {
    let w = app.get_webview_window("main").ok_or("窗口不存在")?;
    let (x, y, _, _) = screen_bounds(&w)?;
    let monitor = w
        .current_monitor()
        .map_err(|_| "无法读取显示器")?
        .ok_or("未找到显示器")?;
    let sf = w.scale_factor().unwrap_or(1.);
    let state = app.state::<DesktopState>();
    let mut cached = state.wallpaper.lock().map_err(|_| "壁纸缓存不可用")?;
    let mut pathbuf = [0u16; 32768];
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            pathbuf.len() as u32,
            pathbuf.as_mut_ptr() as _,
            0,
        );
    }
    let path = PathBuf::from(String::from_utf16_lossy(
        &pathbuf[..pathbuf.iter().position(|n| *n == 0).unwrap_or(0)],
    ));
    let metadata = std::fs::metadata(&path).ok();
    let modified = metadata.as_ref().and_then(|m| m.modified().ok());
    if cached
        .as_ref()
        .is_none_or(|c| c.path != path || c.modified != modified)
    {
        let bytes = if metadata.is_some_and(|m| m.len() <= 32 * 1024 * 1024) {
            std::fs::read(&path).unwrap_or_default()
        } else {
            vec![]
        };
        let mime = if bytes.starts_with(b"\x89PNG") {
            "image/png"
        } else if bytes.starts_with(b"BM") {
            "image/bmp"
        } else {
            "image/jpeg"
        };
        let data = if bytes.is_empty() {
            String::new()
        } else {
            format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            )
        };
        let revision = format!("{:?}:{:?}", path, modified);
        *cached = Some(WallpaperCache {
            path,
            modified,
            revision,
            data,
        });
    }
    let cache = cached.as_ref().unwrap();
    Ok(Background {
        revision: cache.revision.clone(),
        data_url: if known_revision == cache.revision {
            None
        } else {
            Some(cache.data.clone())
        },
        x: (x - monitor.position().x) as f64 / sf,
        y: (y - monitor.position().y) as f64 / sf,
        width: monitor.size().width as f64 / sf,
        height: monitor.size().height as f64 / sf,
        attached: is_attached(&w),
    })
}

#[cfg(test)]
mod frame_tests {
    use super::*;
    #[test]
    fn desktop_styles_remove_frame_without_losing_unrelated_flags() {
        let style = (WS_POPUP | WS_CAPTION | WS_THICKFRAME | WS_VISIBLE | WS_CLIPCHILDREN) as isize;
        let ex = (WS_EX_CLIENTEDGE | WS_EX_WINDOWEDGE | WS_EX_ACCEPTFILES) as isize;
        let (clean, clean_ex) = clean_styles(style, ex);
        assert_eq!(clean & (WS_CAPTION | WS_THICKFRAME | WS_POPUP) as isize, 0);
        assert_ne!(clean & WS_CHILD as isize, 0);
        assert_ne!(clean & WS_VISIBLE as isize, 0);
        assert_ne!(clean & WS_CLIPCHILDREN as isize, 0);
        assert_eq!(clean_ex, WS_EX_ACCEPTFILES as isize);
        assert_eq!(clean_styles(clean, clean_ex), (clean, clean_ex));
    }
    #[test]
    fn clip_geometry_uses_physical_pixels_and_css_radius() {
        let geometry = WidgetGeometry {
            width: 350.,
            height: 280.5,
            radius: 23.,
        };
        assert_eq!(region_dimensions(geometry, 1.), (350, 281, 46));
        assert_eq!(region_dimensions(geometry, 1.25), (438, 351, 58));
        assert_eq!(region_dimensions(geometry, 1.5), (525, 421, 69));
    }
    #[test]
    fn actual_native_region_clips_corners_and_clears_for_panel() {
        unsafe {
            // Own hidden test window; no user window or Explorer state is modified.
            let hwnd = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Token frame test").as_ptr(),
                WS_POPUP,
                0,
                0,
                350,
                300,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            struct OwnedWindow(HWND);
            impl Drop for OwnedWindow {
                fn drop(&mut self) {
                    unsafe {
                        DestroyWindow(self.0);
                    }
                }
            }
            let _window = OwnedWindow(hwnd);
            nonclient_policy(hwnd, true).unwrap();
            nonclient_policy(hwnd, false).unwrap();
            for radius in [20., 23., 20., 23.] {
                set_region(
                    hwnd,
                    WidgetGeometry {
                        width: 350.,
                        height: 280.,
                        radius,
                    },
                    1.,
                    true,
                )
                .unwrap();
                let region = CreateRectRgn(0, 0, 0, 0);
                assert!(GetWindowRgn(hwnd, region) > 0);
                assert_eq!(
                    windows_sys::Win32::Graphics::Gdi::PtInRegion(region, 0, 0),
                    0
                );
                assert_ne!(
                    windows_sys::Win32::Graphics::Gdi::PtInRegion(region, 175, 120),
                    0
                );
                assert_eq!(
                    windows_sys::Win32::Graphics::Gdi::PtInRegion(region, 175, 285),
                    0
                );
                DeleteObject(region);
            }
            set_region(
                hwnd,
                WidgetGeometry {
                    width: 350.,
                    height: 280.,
                    radius: 20.,
                },
                1.,
                false,
            )
            .unwrap();
            let region = CreateRectRgn(0, 0, 0, 0);
            assert_eq!(GetWindowRgn(hwnd, region), 0);
            DeleteObject(region);
        }
    }
}
