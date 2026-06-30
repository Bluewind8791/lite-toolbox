//! 사용자 설정 영속화. `%APPDATA%\LiteToolbox\config.json`.
//! 현재는 수동 등록 IDE(exe 경로) 목록만.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// 수동 등록한 IDE 실행파일 절대경로.
    #[serde(default)]
    pub manual_ides: Vec<String>,
}

/// config.json 절대경로. `%APPDATA%\LiteToolbox\config.json`.
fn config_path() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA 환경변수 없음".to_string())?;
    Ok(PathBuf::from(appdata)
        .join("LiteToolbox")
        .join("config.json"))
}

/// 데이터 디렉토리 `%APPDATA%\LiteToolbox`.
pub fn data_dir() -> String {
    std::env::var("APPDATA")
        .map(|a| PathBuf::from(a).join("LiteToolbox").to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 로드. 파일 없거나 파싱 실패 시 기본(빈) Config.
pub fn load() -> Config {
    let Ok(path) = config_path() else {
        return Config::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 저장. 디렉토리 자동 생성.
fn save(config: &Config) -> Result<(), String> {
    let path = config_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("디렉토리 생성 실패: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| format!("직렬화 실패: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("저장 실패: {e}"))
}

/// 수동 IDE 추가. 실행파일 존재 검증, 중복(대소문자 무시) 제외.
pub fn add_manual_ide(exe: &str) -> Result<(), String> {
    let exe = exe.trim();
    if exe.is_empty() {
        return Err("경로가 비어 있습니다.".to_string());
    }
    if !std::path::Path::new(exe).is_file() {
        return Err("실행 파일이 존재하지 않습니다.".to_string());
    }
    let mut config = load();
    if !config.manual_ides.iter().any(|p| p.eq_ignore_ascii_case(exe)) {
        config.manual_ides.push(exe.to_string());
        save(&config)?;
    }
    Ok(())
}

/// 수동 IDE 제거(대소문자 무시).
pub fn remove_manual_ide(exe: &str) -> Result<(), String> {
    let mut config = load();
    config.manual_ides.retain(|p| !p.eq_ignore_ascii_case(exe));
    save(&config)
}
