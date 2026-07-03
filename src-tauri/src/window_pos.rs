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

/// 현재 마우스 커서가 창의 외곽 사각형 안에 있는지.
/// 리사이즈/이동 드래그 중(테두리 잡음) 발생하는 blur 를 창 밖 클릭과 구분하는 용도.
#[cfg(windows)]
pub fn cursor_in_window(window: &WebviewWindow) -> bool {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut pt = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut pt) } == 0 {
        return false;
    }
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return false;
    };
    pt.x >= pos.x
        && pt.x < pos.x + size.width as i32
        && pt.y >= pos.y
        && pt.y < pos.y + size.height as i32
}

#[cfg(not(windows))]
pub fn cursor_in_window(_window: &WebviewWindow) -> bool {
    false
}
