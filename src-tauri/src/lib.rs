mod config;
mod ide_detect;
mod launcher;
mod recent;
mod store;
mod window_pos;

use ide_detect::DetectedIde;
use store::{Folder, Project};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

/// 설치된 IDE 목록 반환 (Toolbox state.json 기반).
#[tauri::command]
fn detect_ides() -> Vec<DetectedIde> {
    ide_detect::detect_ides()
}

/// IDE 실행. project_path 가 있으면 해당 프로젝트를 열고, 없으면 IDE 단독 실행.
#[tauri::command]
fn launch_ide(ide_id: String, project_path: Option<String>) -> Result<(), String> {
    let ide = ide_detect::find_by_id(&ide_id)
        .ok_or_else(|| format!("IDE 를 찾을 수 없습니다: {ide_id}"))?;
    launcher::launch(&ide.exe_path, project_path.as_deref())
}

/// 수동 IDE 등록 (exe 경로).
#[tauri::command]
fn add_manual_ide(path: String) -> Result<(), String> {
    config::add_manual_ide(&path)
}

/// 수동 IDE 제거 (exe 경로).
#[tauri::command]
fn remove_manual_ide(path: String) -> Result<(), String> {
    config::remove_manual_ide(&path)
}

/// 데이터 저장 디렉토리 경로(`%APPDATA%\LiteToolbox`).
#[tauri::command]
fn data_dir() -> String {
    config::data_dir()
}

/// IDE 아이콘 SVG 텍스트 반환. 경로 없거나 읽기 실패 시 None.
#[tauri::command]
fn ide_icon(path: String) -> Option<String> {
    std::fs::read_to_string(&path).ok()
}

/// 저장된 프로젝트 목록.
#[tauri::command]
fn list_projects() -> Vec<Project> {
    store::load().projects
}

/// 실제 디렉토리가 없는 프로젝트 id 목록.
#[tauri::command]
fn missing_project_ids() -> Vec<String> {
    store::load()
        .projects
        .into_iter()
        .filter(|p| !std::path::Path::new(&p.path).is_dir())
        .map(|p| p.id)
        .collect()
}

/// 프로젝트 추가 (폴더 경로). 중복이면 기존 항목 반환.
#[tauri::command]
fn add_project(path: String) -> Result<Project, String> {
    store::add_project(&path)
}

/// 프로젝트 제거.
#[tauri::command]
fn remove_project(id: String) -> Result<(), String> {
    store::remove_project(&id)
}

/// 저장된 폴더 목록.
#[tauri::command]
fn list_folders() -> Vec<Folder> {
    store::load().folders
}

/// 폴더 추가. parent_id 생략 시 루트.
#[tauri::command]
fn add_folder(name: String, parent_id: Option<String>) -> Result<Folder, String> {
    store::add_folder(&name, parent_id)
}

/// 폴더 이름 변경.
#[tauri::command]
fn rename_folder(id: String, name: String) -> Result<(), String> {
    store::rename_folder(&id, &name)
}

/// 폴더 제거. 자식·소속 프로젝트는 부모로 이동.
#[tauri::command]
fn remove_folder(id: String) -> Result<(), String> {
    store::remove_folder(&id)
}

/// 폴더 이동(재부모).
#[tauri::command]
fn move_folder(id: String, parent_id: Option<String>) -> Result<(), String> {
    store::move_folder(&id, parent_id)
}

/// 프로젝트를 폴더에 배정 + 위치 지정. folder_id 생략 시 미분류.
/// before_id 지정 시 해당 프로젝트 앞에, 생략 시 폴더 맨 끝에 배치.
#[tauri::command]
fn move_project(
    id: String,
    folder_id: Option<String>,
    before_id: Option<String>,
) -> Result<(), String> {
    store::move_project(&id, folder_id, before_id)
}

/// JetBrains 최근 프로젝트(recentProjects.xml) 일괄 임포트. 신규 추가 건수 반환.
/// productionCode 를 탐지된 IDE 와 매칭해 preferredIdeId 로 지정.
#[tauri::command]
fn import_recent_projects() -> Result<usize, String> {
    let ides = ide_detect::detect_ides();
    let items = recent::recent_projects()
        .into_iter()
        .map(|r| {
            let preferred = ides
                .iter()
                .find(|i| i.product_code == r.product_code)
                .map(|i| i.id.clone());
            store::ImportItem {
                path: r.path,
                preferred_ide_id: preferred,
                last_opened_at: if r.last_opened.is_empty() {
                    None
                } else {
                    Some(r.last_opened)
                },
            }
        })
        .collect();
    store::import_projects(items)
}

/// 프로젝트 열기. ide_id 가 없으면 preferredIdeId 사용. 둘 다 없으면 에러.
/// 사용된 IDE 를 preferredIdeId 로 저장.
#[tauri::command]
fn open_project(id: String, ide_id: Option<String>) -> Result<(), String> {
    let project = store::find_project(&id).ok_or_else(|| format!("프로젝트 없음: {id}"))?;
    let chosen = ide_id
        .or(project.preferred_ide_id.clone())
        .ok_or("열 IDE 가 지정되지 않았습니다.".to_string())?;
    let ide = ide_detect::find_by_id(&chosen)
        .ok_or_else(|| format!("IDE 를 찾을 수 없습니다: {chosen}"))?;
    launcher::launch(&ide.exe_path, Some(&project.path))?;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    store::mark_opened(&id, &chosen, millis)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window_pos::pin_bottom_right(&window);
                // 이동 시 우측 하단으로 스냅백 (항상 고정).
                let w = window.clone();
                window.on_window_event(move |event| match event {
                    // 닫기 요청 → 종료 막고 트레이로 숨김
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                    // 이동 시 우측 하단으로 스냅백 (항상 고정).
                    tauri::WindowEvent::Moved(_) => {
                        if let (Some(target), Ok(cur)) =
                            (window_pos::target_position(&w), w.outer_position())
                        {
                            if cur.x != target.x || cur.y != target.y {
                                let _ = w.set_position(target);
                            }
                        }
                    }
                    _ => {}
                });
            }

            // 시스템 트레이: 좌클릭=창 호출, 우클릭 메뉴=열기/종료
            let open_item = MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(tauri::include_image!("icons/tray-icon.png"))
                .tooltip("Lite Toolbox")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            detect_ides,
            launch_ide,
            add_manual_ide,
            remove_manual_ide,
            data_dir,
            ide_icon,
            list_projects,
            missing_project_ids,
            add_project,
            remove_project,
            list_folders,
            add_folder,
            rename_folder,
            remove_folder,
            move_folder,
            move_project,
            import_recent_projects,
            open_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
