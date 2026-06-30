//! 메인 창을 화면 우측 하단(작업표시줄 제외 작업영역)에 고정.

use tauri::{PhysicalPosition, WebviewWindow};

/// 창과 작업영역 가장자리 사이 여백(px). 0 = 딱 붙임.
const MARGIN: i32 = 0;

/// 기본 모니터 작업영역(left, top, right, bottom). 작업표시줄 제외.
#[cfg(windows)]
fn work_area() -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut rect as *mut RECT).cast(), 0) };
    if ok == 0 {
        return None;
    }
    Some((rect.left, rect.top, rect.right, rect.bottom))
}

#[cfg(not(windows))]
fn work_area() -> Option<(i32, i32, i32, i32)> {
    None
}

/// 현재 창 크기 기준 우측 하단 목표 좌표.
pub fn target_position(window: &WebviewWindow) -> Option<PhysicalPosition<i32>> {
    let (_left, _top, right, bottom) = work_area()?;
    let size = window.outer_size().ok()?;
    Some(PhysicalPosition::new(
        right - size.width as i32 - MARGIN,
        bottom - size.height as i32 - MARGIN,
    ))
}

/// 창을 우측 하단으로 이동.
pub fn pin_bottom_right(window: &WebviewWindow) {
    if let Some(pos) = target_position(window) {
        let _ = window.set_position(pos);
    }
}
