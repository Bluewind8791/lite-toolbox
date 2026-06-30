//! JetBrains IDE 자동 탐지 (Windows).
//!
//! 소스 3종을 병합(exe 경로로 중복 제거):
//! 1. Toolbox `state.json` (`%LOCALAPPDATA%\JetBrains\Toolbox\state.json`) — `launchCommand` 가 exe 절대경로 직접 제공.
//! 2. 독립 설치 — `C:\Program Files\JetBrains\*`, Android Studio 등 설치 루트의 `product-info.json` 스캔.
//! 3. 수동 등록 — `config.json` 의 exe 경로(설치 루트 product-info.json 보강, 실패 시 파일명 폴백).
//! 각 소스는 실패해도 에러가 아닌 빈 목록 → 폴백으로 동작 유지.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 프론트로 노출되는 탐지 결과. (PLAN 3.1 DetectedIde)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedIde {
    /// 안정적 식별자 (toolId + buildNumber). 같은 제품 다중 버전 구분.
    pub id: String,
    pub tool_name: String,
    pub product_code: String,
    /// 표시 버전. 일부 제품(Android Studio)은 비표준 문자열일 수 있음.
    pub version: String,
    pub build_number: String,
    pub channel: String,
    pub exe_path: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
}

/// Toolbox state.json 루트. 모르는 필드는 무시.
#[derive(Debug, Deserialize)]
struct ToolboxState {
    #[serde(default)]
    tools: Vec<ToolboxTool>,
}

/// state.json 의 tools[] 항목.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxTool {
    tool_id: String,
    product_code: String,
    display_name: String,
    display_version: String,
    build_number: String,
    launch_command: String,
}

/// Toolbox state.json 기본 경로.
fn toolbox_state_path() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("JetBrains")
            .join("Toolbox")
            .join("state.json"),
    )
}

/// state.json 문자열 → DetectedIde 목록.
fn parse_state(json: &str) -> Result<Vec<DetectedIde>, serde_json::Error> {
    let state: ToolboxState = serde_json::from_str(json)?;
    Ok(state.tools.into_iter().map(map_tool).collect())
}

/// Toolbox tool → DetectedIde. launchCommand 가 비어도 일단 매핑(상위에서 검증).
fn map_tool(t: ToolboxTool) -> DetectedIde {
    let icon_path = icon_for_exe(Path::new(&t.launch_command));
    DetectedIde {
        id: format!("{}_{}", t.tool_id, t.build_number),
        tool_name: t.display_name,
        product_code: t.product_code,
        version: t.display_version,
        build_number: t.build_number,
        // state.json 에 명시 채널 필드 없음. 현재는 release 가정.
        channel: "release".to_string(),
        exe_path: t.launch_command,
        source: "toolbox".to_string(),
        icon_path,
    }
}

/// Toolbox state.json 기반 탐지. 파일 없음/파싱 실패는 빈 목록.
fn toolbox_ides() -> Vec<DetectedIde> {
    let Some(path) = toolbox_state_path() else {
        return Vec::new();
    };
    let Ok(json) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_state(&json).unwrap_or_default()
}

/// 설치 루트의 `product-info.json` 항목. 모르는 필드 무시.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductInfo {
    name: String,
    version: String,
    build_number: String,
    product_code: String,
    #[serde(default)]
    svg_icon_path: Option<String>,
    #[serde(default)]
    launch: Vec<LaunchEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchEntry {
    #[serde(default)]
    os: String,
    launcher_path: String,
}

/// 안정적 해시(폴백 id 용).
fn stable_hash(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.to_lowercase().hash(&mut h);
    format!("{:016x}", h.finish())
}

/// 설치 루트 + svgIconPath(상대경로) → 존재하는 SVG 절대경로.
fn icon_abs(dir: &Path, svg_rel: &Option<String>) -> Option<String> {
    let rel = svg_rel.as_ref()?;
    let icon = dir.join(rel.replace('/', "\\"));
    icon.is_file().then(|| icon.to_string_lossy().into_owned())
}

/// exe(…\bin\xxx.exe) → 설치 루트의 product-info.json svgIconPath → SVG 절대경로.
fn icon_for_exe(exe: &Path) -> Option<String> {
    let root = exe.parent()?.parent()?;
    let json = std::fs::read_to_string(root.join("product-info.json")).ok()?;
    let info: ProductInfo = serde_json::from_str(&json).ok()?;
    icon_abs(root, &info.svg_icon_path)
}

/// 설치 루트 디렉토리 → DetectedIde. product-info.json 파싱 + launcher exe 존재 확인.
fn ide_from_install_dir(dir: &Path, source: &str) -> Option<DetectedIde> {
    let json = std::fs::read_to_string(dir.join("product-info.json")).ok()?;
    let info: ProductInfo = serde_json::from_str(&json).ok()?;
    let launcher = info
        .launch
        .iter()
        .find(|l| l.os.eq_ignore_ascii_case("Windows"))
        .or_else(|| info.launch.first())?;
    let exe = dir.join(launcher.launcher_path.replace('/', "\\"));
    if !exe.is_file() {
        return None;
    }
    let icon_path = icon_abs(dir, &info.svg_icon_path);
    Some(DetectedIde {
        id: format!("{}_{}", info.product_code, info.build_number),
        tool_name: info.name,
        product_code: info.product_code,
        version: info.version,
        build_number: info.build_number,
        channel: "release".to_string(),
        exe_path: exe.to_string_lossy().into_owned(),
        source: source.to_string(),
        icon_path,
    })
}

/// 독립 설치 스캔 루트들 (JetBrains + Android Studio, 32/64비트 Program Files).
fn standalone_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        let Some(base) = std::env::var_os(var) else {
            continue;
        };
        for sub in ["JetBrains", "Android"] {
            let p = PathBuf::from(&base).join(sub);
            if seen.insert(p.to_string_lossy().to_lowercase()) {
                roots.push(p);
            }
        }
    }
    roots
}

/// 독립 설치 IDE 탐지. 각 루트의 하위 디렉토리에서 product-info.json 을 찾음.
fn standalone_ides() -> Vec<DetectedIde> {
    let mut out = Vec::new();
    for root in standalone_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                if let Some(ide) = ide_from_install_dir(&dir, "standalone") {
                    out.push(ide);
                }
            }
        }
    }
    out
}

/// 수동 등록 exe 경로 → DetectedIde. 설치 루트 product-info.json 보강, 실패 시 파일명 폴백.
pub fn ide_from_exe(exe_path: &str) -> DetectedIde {
    let exe = Path::new(exe_path);
    // bin\idea64.exe → 설치 루트 = exe 의 부모의 부모.
    if let Some(root) = exe.parent().and_then(|p| p.parent()) {
        if let Some(mut ide) = ide_from_install_dir(root, "manual") {
            ide.exe_path = exe_path.to_string(); // 사용자가 지정한 exe 유지
            return ide;
        }
    }
    let name = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("IDE")
        .to_string();
    DetectedIde {
        id: format!("manual_{}", stable_hash(exe_path)),
        tool_name: name,
        product_code: "?".to_string(),
        version: String::new(),
        build_number: String::new(),
        channel: "manual".to_string(),
        exe_path: exe_path.to_string(),
        source: "manual".to_string(),
        icon_path: None,
    }
}

/// 수동 등록 IDE 목록.
fn manual_ides() -> Vec<DetectedIde> {
    crate::config::load()
        .manual_ides
        .iter()
        .map(|p| ide_from_exe(p))
        .collect()
}

/// 설치된 IDE 탐지. Toolbox → 독립설치 → 수동 순으로 병합, exe 경로(대소문자 무시)로 중복 제거.
pub fn detect_ides() -> Vec<DetectedIde> {
    let mut out: Vec<DetectedIde> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for ide in toolbox_ides()
        .into_iter()
        .chain(standalone_ides())
        .chain(manual_ides())
    {
        if seen.insert(ide.exe_path.to_lowercase()) {
            out.push(ide);
        }
    }
    out
}

/// id 로 탐지된 IDE 1개 조회.
pub fn find_by_id(id: &str) -> Option<DetectedIde> {
    detect_ides().into_iter().find(|i| i.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/state.json");

    #[test]
    fn parses_all_tools() {
        let ides = parse_state(FIXTURE).expect("fixture should parse");
        assert_eq!(ides.len(), 5);
    }

    #[test]
    fn maps_fields_correctly() {
        let ides = parse_state(FIXTURE).unwrap();
        let idea = ides
            .iter()
            .find(|i| i.product_code == "IU")
            .expect("IntelliJ present");
        assert_eq!(idea.tool_name, "IntelliJ IDEA");
        assert_eq!(idea.version, "2026.1.3");
        assert_eq!(idea.build_number, "261.25134.95");
        assert_eq!(
            idea.exe_path,
            "C:\\Users\\castu\\AppData\\Local\\Programs\\IntelliJ IDEA\\bin\\idea64.exe"
        );
        assert_eq!(idea.source, "toolbox");
        assert_eq!(idea.id, "IDEA-U_261.25134.95");
    }

    #[test]
    fn handles_nonstandard_version_string() {
        // Android Studio displayVersion = "Quail 1 2026.1.1 Patch 2" — 비표준.
        // 파싱이 깨지지 않고 원문 그대로 보존되어야 함.
        let ides = parse_state(FIXTURE).unwrap();
        let studio = ides
            .iter()
            .find(|i| i.product_code == "AI")
            .expect("Android Studio present");
        assert_eq!(studio.version, "Quail 1 2026.1.1 Patch 2");
    }

    #[test]
    fn ids_are_unique() {
        let ides = parse_state(FIXTURE).unwrap();
        let mut ids: Vec<&String> = ides.iter().map(|i| &i.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 5, "all ids unique");
    }

    #[test]
    #[ignore = "실제 설치 환경 의존 — 수동 확인용 (cargo test -- --ignored --nocapture)"]
    fn live_detection_smoke() {
        let ides = detect_ides();
        println!("탐지된 IDE {}개:", ides.len());
        for i in &ides {
            println!("  [{}] {} {} -> {}", i.product_code, i.tool_name, i.version, i.exe_path);
        }
        assert!(!ides.is_empty(), "라이브 환경에서 최소 1개 탐지 기대");
    }

    #[test]
    fn empty_tools_yields_empty() {
        let ides = parse_state(r#"{"version":1,"tools":[]}"#).unwrap();
        assert!(ides.is_empty());
    }

    #[test]
    fn missing_tools_field_yields_empty() {
        let ides = parse_state(r#"{"version":1}"#).unwrap();
        assert!(ides.is_empty());
    }

    /// 고유 임시 디렉토리(테스트 격리용).
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("litetb_test_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        dir
    }

    const PRODUCT_INFO: &str = r#"{
        "name": "WebStorm",
        "version": "2025.1.2",
        "buildNumber": "251.23774.456",
        "productCode": "WS",
        "launch": [
            { "os": "Windows", "arch": "amd64", "launcherPath": "bin/webstorm64.exe" },
            { "os": "Linux", "launcherPath": "bin/webstorm.sh" }
        ]
    }"#;

    #[test]
    fn parses_product_info_and_picks_windows_launcher() {
        let dir = temp_dir("pinfo");
        std::fs::write(dir.join("product-info.json"), PRODUCT_INFO).unwrap();
        std::fs::write(dir.join("bin").join("webstorm64.exe"), b"x").unwrap();

        let ide = ide_from_install_dir(&dir, "standalone").expect("should parse");
        assert_eq!(ide.product_code, "WS");
        assert_eq!(ide.version, "2025.1.2");
        assert_eq!(ide.id, "WS_251.23774.456");
        assert_eq!(ide.source, "standalone");
        assert!(ide.exe_path.ends_with("webstorm64.exe"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn product_info_missing_exe_yields_none() {
        let dir = temp_dir("noexe");
        std::fs::write(dir.join("product-info.json"), PRODUCT_INFO).unwrap();
        // launcher exe 미생성 → is_file() 실패 → None.
        assert!(ide_from_install_dir(&dir, "standalone").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn manual_exe_falls_back_to_filename_when_no_product_info() {
        let ide = ide_from_exe(r"C:\tools\RustRover\bin\rustrover64.exe");
        assert_eq!(ide.tool_name, "rustrover64");
        assert_eq!(ide.source, "manual");
        assert_eq!(ide.product_code, "?");
        assert!(ide.id.starts_with("manual_"));
        assert_eq!(ide.exe_path, r"C:\tools\RustRover\bin\rustrover64.exe");
    }

    #[test]
    fn manual_exe_uses_product_info_but_keeps_user_exe() {
        let dir = temp_dir("manual");
        std::fs::write(dir.join("product-info.json"), PRODUCT_INFO).unwrap();
        std::fs::write(dir.join("bin").join("webstorm64.exe"), b"x").unwrap();
        // bin\foo.exe → 부모의 부모 = dir → product-info 보강, exe 는 사용자 지정 유지.
        let user_exe = dir.join("bin").join("foo.exe");
        std::fs::write(&user_exe, b"x").unwrap();
        let ide = ide_from_exe(user_exe.to_str().unwrap());
        assert_eq!(ide.product_code, "WS");
        assert_eq!(ide.source, "manual");
        assert_eq!(ide.exe_path, user_exe.to_string_lossy());
        std::fs::remove_dir_all(&dir).ok();
    }
}
