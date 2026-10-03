//! The Hand tool's pointer as a real system cursor, so it moves with the mouse at the screen's
//! own rate instead of being drawn one frame late. macOS and Linux have open- and closed-hand
//! cursors; Windows has none (it shows four arrows for them), so there the program makes its own
//! from a drawing and hands it to Windows whenever the pointer is over the window.

/// Which hand the pointer shows this frame (call every frame the hand should be shown).
pub fn hand(ctx: &egui::Context, closed: bool) {
    #[cfg(windows)]
    if win::show(if closed { 2 } else { 1 }) {
        ctx.set_cursor_icon(egui::CursorIcon::Default);
        return;
    }
    ctx.set_cursor_icon(if closed {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    });
}

/// Called at the start of every frame: the hand stays only while `hand` keeps asking for it.
pub fn begin_frame() {
    #[cfg(windows)]
    win::begin_frame();
}

/// Installs the cursor hook on the program window (Windows; once).
pub fn install(frame: &eframe::Frame) {
    #[cfg(windows)]
    win::install(frame);
    #[cfg(not(windows))]
    let _ = frame;
}

/// Straight-alpha RGBA of the hand at `size` pixels (open or closed), with a dark rim.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn hand_rgba(size: u32, closed: bool) -> Vec<u8> {
    // the same shapes as the drawn icon, in a 24-unit box: (x0, y0, x1, y1, corner radius)
    let tops: [f32; 4] = if closed {
        [8.5, 7.5, 8.0, 9.0]
    } else {
        [3.5, 1.5, 2.5, 5.0]
    };
    let mut parts: Vec<[f32; 5]> = tops
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let x = 6.2 + i as f32 * 3.4;
            [x, *t, x + 3.0, 14.0, 1.5]
        })
        .collect();
    parts.push([6.0, 10.0, 19.6, 21.5, 4.0]);
    parts.push(if closed {
        [3.6, 12.0, 7.6, 16.0, 1.8]
    } else {
        [2.4, 11.0, 7.4, 14.2, 1.8]
    });
    // distance from a point to a rounded rectangle (negative inside)
    let sdf = |px: f32, py: f32, r: &[f32; 5]| {
        let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        let (hx, hy) = ((r[2] - r[0]) / 2.0 - r[4], (r[3] - r[1]) / 2.0 - r[4]);
        let (dx, dy) = ((px - cx).abs() - hx, (py - cy).abs() - hy);
        let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
        outside + dx.max(dy).min(0.0) - r[4]
    };
    let k = 24.0 / size as f32;
    let rim = 1.3;
    let mut out = vec![0u8; (size * size * 4) as usize];
    const SS: u32 = 4;
    for y in 0..size {
        for x in 0..size {
            let (mut fill, mut edge) = (0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = (x as f32 + (sx as f32 + 0.5) / SS as f32) * k;
                    let py = (y as f32 + (sy as f32 + 0.5) / SS as f32) * k;
                    let d = parts
                        .iter()
                        .map(|r| sdf(px, py, r))
                        .fold(f32::INFINITY, f32::min);
                    if d <= 0.0 {
                        fill += 1.0;
                    } else if d <= rim {
                        edge += 1.0;
                    }
                }
            }
            let n = (SS * SS) as f32;
            let (f, e) = (fill / n, edge / n);
            let a = f + e;
            if a > 0.0 {
                // white inside, black rim, mixed at the boundary
                let v = (250.0 * f / a) as u8;
                let i = ((y * size + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&[v, v, v.saturating_add(2), (a * 255.0) as u8]);
            }
        }
    }
    out
}

#[cfg(windows)]
mod win {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
        DeleteObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, CreateIconIndirect, GWLP_WNDPROC, GetSystemMetrics, HCURSOR, HTCLIENT,
        ICONINFO, SM_CXCURSOR, SetCursor, SetWindowLongPtrW, WM_SETCURSOR, WNDPROC,
    };

    /// 0 none, 1 open hand, 2 closed hand.
    static WANT: AtomicU8 = AtomicU8::new(0);
    static OLD_PROC: AtomicIsize = AtomicIsize::new(0);
    static CURSORS: OnceLock<Option<[usize; 2]>> = OnceLock::new();

    fn cursors() -> Option<[usize; 2]> {
        *CURSORS.get_or_init(|| {
            // SAFETY: plain system metric query
            let size = unsafe { GetSystemMetrics(SM_CXCURSOR) }.clamp(32, 128) as u32;
            Some([make(size, false)?, make(size, true)?])
        })
    }

    /// A cursor from the hand drawing, hot spot in the middle of the palm.
    fn make(size: u32, closed: bool) -> Option<usize> {
        let rgba = super::hand_rgba(size, closed);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                // negative: rows top to bottom
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [unsafe { std::mem::zeroed() }],
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        // SAFETY: a 32-bit DIB of size x size; `bits` points to its pixels, written below
        // within those bounds; the handles are released after the cursor copies them
        unsafe {
            let color = CreateDIBSection(
                std::ptr::null_mut(),
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            if color.is_null() || bits.is_null() {
                return None;
            }
            let px = std::slice::from_raw_parts_mut(bits as *mut u8, (size * size * 4) as usize);
            for (d, s) in px
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(rgba.as_chunks::<4>().0)
            {
                // premultiplied BGRA
                let a = s[3] as u32;
                d[0] = (s[2] as u32 * a / 255) as u8;
                d[1] = (s[1] as u32 * a / 255) as u8;
                d[2] = (s[0] as u32 * a / 255) as u8;
                d[3] = s[3];
            }
            let mask = CreateBitmap(size as i32, size as i32, 1, 1, std::ptr::null());
            let icon = ICONINFO {
                fIcon: 0,
                xHotspot: size / 2,
                yHotspot: size / 2,
                hbmMask: mask,
                hbmColor: color,
            };
            let cursor = CreateIconIndirect(&icon);
            DeleteObject(color);
            DeleteObject(mask);
            (!cursor.is_null()).then_some(cursor as usize)
        }
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        let want = WANT.load(Ordering::Relaxed);
        if msg == WM_SETCURSOR
            && (lp & 0xFFFF) as u32 == HTCLIENT
            && want != 0
            && let Some(c) = cursors()
        {
            // SAFETY: a cursor handle made by `make`, alive for the program's life
            unsafe { SetCursor(c[want as usize - 1] as HCURSOR) };
            return 1;
        }
        // SAFETY: the window procedure that was installed before ours
        unsafe {
            let old: WNDPROC = std::mem::transmute(OLD_PROC.load(Ordering::Relaxed));
            CallWindowProcW(old, hwnd, msg, wp, lp)
        }
    }

    pub fn install(frame: &eframe::Frame) {
        if OLD_PROC.load(Ordering::Relaxed) != 0 || cursors().is_none() {
            return;
        }
        let Ok(handle) = frame.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return;
        };
        let hwnd = h.hwnd.get() as HWND;
        // SAFETY: replaces the window procedure of our own window with one that forwards every
        // message to the previous one, except the cursor request while the hand is wanted
        let old = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, proc as *const () as isize) };
        OLD_PROC.store(old, Ordering::Relaxed);
        log::debug!("hand cursor hook installed");
    }

    pub fn begin_frame() {
        // the hand stays while the timeline asks for it again this frame
        WANT.store(0, Ordering::Relaxed);
    }

    /// Shows a hand (1 open, 2 closed) now; false when the system cursor is not available.
    pub fn show(which: u8) -> bool {
        if OLD_PROC.load(Ordering::Relaxed) == 0 {
            return false;
        }
        let Some(c) = cursors() else { return false };
        WANT.store(which, Ordering::Relaxed);
        // SAFETY: a cursor handle made by `make`
        unsafe { SetCursor(c[which as usize - 1] as HCURSOR) };
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_hand_is_opaque_in_the_palm_and_clear_around_it() {
        for closed in [false, true] {
            let s = 32;
            let px = super::hand_rgba(s, closed);
            let at = |x: u32, y: u32| px[((y * s + x) * 4 + 3) as usize];
            // the palm, a corner, and the rim is dark while the inside is light
            assert_eq!(at(s / 2, s * 2 / 3), 255);
            assert_eq!(at(0, s - 1), 0);
            assert!(px[((s * 2 / 3 * s + s / 2) * 4) as usize] > 200);
        }
    }
}
