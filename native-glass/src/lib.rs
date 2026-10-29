//! Native Windows compositor backdrop. No screen capture or application-owned pixel buffers.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::WinRT::{Composition::*, *},
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    UI::Composition::*,
};
#[derive(Clone, Default)]
pub struct Status {
    pub ready: bool,
    pub sources: usize,
    pub error: Option<String>,
}
pub struct Renderer {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
    worker: Option<JoinHandle<()>>,
}
impl Renderer {
    pub fn start(hwnd: isize, _blur: f32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(Status::default()));
        let (signal, stats) = (stop.clone(), status.clone());
        let worker = thread::spawn(move || unsafe {
            let result = (|| -> Result<()> {
                RoInitialize(RO_INIT_SINGLETHREADED)?;
                let previous =
                    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                let _queue = CreateDispatcherQueueController(DispatcherQueueOptions {
                    dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                    threadType: DQTYPE_THREAD_CURRENT,
                    apartmentType: DQTAT_COM_STA,
                })?;
                let hwnd = HWND(hwnd as _);
                let enabled = BOOL(1);
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_HOSTBACKDROPBRUSH,
                    &enabled as *const _ as _,
                    4,
                )?;
                let compositor = Compositor::new()?;
                let interop: ICompositorDesktopInterop = compositor.cast()?;
                let target = interop.CreateDesktopWindowTarget(hwnd, false)?;
                let sprite = compositor.CreateSpriteVisual()?;
                let brush = compositor.CreateHostBackdropBrush()?;
                sprite.SetBrush(&brush)?;
                target.SetRoot(&sprite)?;
                let mut last = (0, 0);
                while !signal.load(Ordering::Relaxed) && IsWindow(Some(hwnd)).as_bool() {
                    let mut msg = MSG::default();
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let mut r = RECT::default();
                    GetClientRect(hwnd, &mut r)?;
                    if last != (r.right, r.bottom) {
                        sprite.SetSize(windows_numerics::Vector2 {
                            X: r.right as f32,
                            Y: r.bottom as f32,
                        })?;
                        last = (r.right, r.bottom);
                    }
                    let mut bounds = RECT::default();
                    GetWindowRect(hwnd, &mut bounds)?;
                    let mut count = 0usize;
                    if IsWindowVisible(hwnd).as_bool() {
                        EnumDisplayMonitors(
                            None,
                            Some(&bounds),
                            Some(count_monitor),
                            LPARAM(&mut count as *mut usize as isize),
                        )
                        .ok()?;
                    }
                    *stats.lock().unwrap() = Status {
                        ready: true,
                        sources: count,
                        error: None,
                    };
                    thread::sleep(Duration::from_millis(50));
                }
                target.SetRoot(None)?;
                let disabled = BOOL(0);
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_HOSTBACKDROPBRUSH,
                    &disabled as *const _ as _,
                    4,
                );
                SetThreadDpiAwarenessContext(previous);
                Ok(())
            })();
            if let Err(e) = result {
                stats.lock().unwrap().error = Some(format!(
                    "原生毛玻璃不可用（需要 Windows 11 及系统透明效果）: {:08x}",
                    e.code().0
                ));
            }
        });
        Self {
            stop,
            status,
            worker: Some(worker),
        }
    }
    // The system owns the native blur kernel. Material tint is controlled by the UI.
    pub fn set_blur(&self, _value: f32) {}
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
unsafe extern "system" fn count_monitor(_: HMONITOR, _: HDC, _: *mut RECT, value: LPARAM) -> BOOL {
    *(value.0 as *mut usize) += 1;
    TRUE
}
pub fn intersect(a: [i32; 4], b: [i32; 4]) -> Option<[i32; 4]> {
    let r = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}
pub fn drag_origin(cursor: (i32, i32), logical_grab: (f64, f64), scale: f64) -> (i32, i32) {
    (
        cursor.0 - (logical_grab.0 * scale).round() as i32,
        cursor.1 - (logical_grab.1 * scale).round() as i32,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_dpi_grab_stays_at_cursor() {
        assert_eq!(drag_origin((2000, 200), (20., 10.), 1.5), (1970, 185));
        assert_eq!(drag_origin((-300, 200), (20., 10.), 1.), (-320, 190));
    }
    #[test]
    fn split_background_at_negative_monitor_boundary() {
        assert_eq!(
            intersect([-100, 20, 200, 200], [-1920, 0, 0, 1080]),
            Some([-100, 20, 0, 200])
        );
        assert_eq!(
            intersect([-100, 20, 200, 200], [0, 0, 1920, 1080]),
            Some([0, 20, 200, 200])
        );
        assert_eq!(intersect([0, 0, 10, 10], [10, 0, 20, 10]), None);
    }
}

#[cfg(feature = "diagnostics")]
mod diagnostics;
#[cfg(feature = "diagnostics")]
pub use diagnostics::verify as test_visible_pattern;
