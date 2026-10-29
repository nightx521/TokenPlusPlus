use super::*;
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};
use windows::Win32::System::LibraryLoader::*;
static PHASE: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_DPICHANGED => {
            let r = &*(l.0 as *const RECT);
            let _ = SetWindowPos(
                h,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            if GetWindowLongPtrW(h, GWLP_USERDATA) != 1 {
                let _ = EndPaint(h, &ps);
                return LRESULT(0);
            }
            let mut r = RECT::default();
            let _ = GetClientRect(h, &mut r);
            let phase = PHASE.load(Ordering::Relaxed);
            for x in (0..r.right).step_by(4) {
                let bright = (x / 4) % 2 == 0;
                let value = if bright { 220 } else { 25 };
                let color = if phase % 2 == 0 {
                    COLORREF(value | ((value / 2) << 8))
                } else {
                    COLORREF((value << 16) | ((value / 2) << 8))
                };
                let b = CreateSolidBrush(color);
                FillRect(
                    dc,
                    &RECT {
                        left: x,
                        top: 0,
                        right: x + 4,
                        bottom: r.bottom,
                    },
                    b,
                );
                let _ = DeleteObject(b.into());
            }
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

pub fn verify(hwnd: isize) -> Result<bool> {
    unsafe {
        let h = HWND(hwnd as _);
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(None)?;
        let class = w!("TokenNativeWidgetFixture");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: inst.into(),
            lpszClassName: class,
            ..Default::default()
        });
        let mut r = RECT::default();
        GetWindowRect(h, &mut r)?;
        let bg = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("Token controlled fixture"),
            WS_POPUP,
            r.left - 30,
            r.top - 30,
            r.right - r.left + 60,
            r.bottom - r.top + 60,
            None,
            None,
            Some(inst.into()),
            None,
        )?;
        SetWindowLongPtrW(bg, GWLP_USERDATA, 1);
        SetWindowPos(
            bg,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )?;
        SetWindowPos(
            h,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
        let dc = GetDC(None);
        let mut values = Vec::new();
        let mut smooth = 0;
        for phase in 0..6 {
            PHASE.store(phase, Ordering::Relaxed);
            let _ = InvalidateRect(Some(bg), None, false);
            let _ = UpdateWindow(bg);
            let start = Instant::now();
            let mut msg = MSG::default();
            while start.elapsed() < Duration::from_millis(350) {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                thread::sleep(Duration::from_millis(10));
            }
            let colors: Vec<u32> = (0..16)
                .map(|i| GetPixel(dc, (r.left + r.right) / 2 + i, r.bottom - 12).0)
                .collect();
            let reds: Vec<i32> = colors.iter().map(|v| (v & 255) as i32).collect();
            let range = reds.iter().max().unwrap() - reds.iter().min().unwrap();
            if range < 30 {
                smooth += 1;
            }
            values.push(reds.iter().sum::<i32>() / 16);
        }
        let _ = ReleaseDC(None, dc);
        let _ = DestroyWindow(bg);
        let _ = SetWindowPos(
            h,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        SetThreadDpiAwarenessContext(previous);
        eprintln!("native widget diagnostic samples={values:?} smoothed={smooth}/6");
        Ok(values.iter().max().unwrap() - values.iter().min().unwrap() > 15 && smooth >= 5)
    }
}
