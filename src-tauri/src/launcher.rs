//! IDE 프로세스 실행 (Windows).
//!
//! 런처가 종료돼도 IDE 가 유지되도록 detached 로 spawn.

use std::os::windows::process::CommandExt;
use std::process::Command;

/// DETACHED_PROCESS — 부모(런처)와 분리, 콘솔 미연결.
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// 주어진 exe 를 실행. project_path 가 있으면 인자로 전달(프로젝트 열기).
/// 성공 시 Ok, 실패 시 사용자 표시용 에러 메시지.
pub fn launch(exe_path: &str, project_path: Option<&str>) -> Result<(), String> {
    if exe_path.is_empty() {
        return Err("실행 경로가 비어 있습니다.".to_string());
    }
    if !std::path::Path::new(exe_path).exists() {
        return Err(format!("실행 파일을 찾을 수 없습니다: {exe_path}"));
    }

    let mut cmd = Command::new(exe_path);
    if let Some(path) = project_path {
        cmd.arg(path);
    }
    cmd.creation_flags(DETACHED_PROCESS);

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("실행 실패: {e}"))
}
