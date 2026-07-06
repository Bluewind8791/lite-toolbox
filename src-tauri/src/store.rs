//! 프로젝트/폴더 영속화. `%APPDATA%\LiteToolbox\data.json` 읽기/쓰기.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_ide_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<String>,
    #[serde(default)]
    pub order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub version: u32,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub projects: Vec<Project>,
    // 기존 데이터의 order 를 이름순으로 1회 시드했는지 표시.
    #[serde(default)]
    pub name_seeded: bool,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            version: SCHEMA_VERSION,
            folders: Vec::new(),
            projects: Vec::new(),
            name_seeded: false,
        }
    }
}

/// data.json 절대경로. `%APPDATA%\LiteToolbox\data.json`.
fn data_path() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA 환경변수 없음".to_string())?;
    Ok(PathBuf::from(appdata).join("LiteToolbox").join("data.json"))
}

/// 디스크에서 로드. 파일 없거나 파싱 실패 시 기본(빈) Store.
pub fn load() -> Store {
    let Ok(path) = data_path() else {
        return Store::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Store::default();
    };
    let mut store: Store = serde_json::from_str(&text).unwrap_or_default();
    // 최초 1회: 기존 프로젝트 order 를 이름순으로 재배치(기본 정렬 = 이름순).
    if !store.name_seeded {
        store
            .projects
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        for (i, p) in store.projects.iter_mut().enumerate() {
            p.order = i as i64;
        }
        store.name_seeded = true;
        let _ = save(&store);
    }
    store
}

/// 디스크에 저장. 디렉토리 자동 생성.
pub fn save(store: &Store) -> Result<(), String> {
    let path = data_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("디렉토리 생성 실패: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(store).map_err(|e| format!("직렬화 실패: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("저장 실패: {e}"))
}

/// 경로 기반 안정적 id 생성.
fn id_from_path(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_lowercase().hash(&mut hasher);
    format!("p{:016x}", hasher.finish())
}

/// 경로 마지막 구성요소를 표시명으로.
fn name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

/// 프로젝트 추가. 중복 경로면 기존 항목 반환(추가 안 함).
pub fn add_project(path: &str) -> Result<Project, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("경로가 비어 있습니다.".to_string());
    }
    let mut store = load();
    let id = id_from_path(path);
    if let Some(existing) = store.projects.iter().find(|p| p.id == id) {
        return Ok(existing.clone());
    }
    let order = store.projects.len() as i64;
    let project = Project {
        id,
        name: name_from_path(path),
        path: path.to_string(),
        folder_id: None,
        favorite: false,
        preferred_ide_id: None,
        last_opened_at: None,
        order,
    };
    store.projects.push(project.clone());
    save(&store)?;
    Ok(project)
}

/// 임포트 1건 (recentProjects.xml 유래).
pub struct ImportItem {
    pub path: String,
    pub preferred_ide_id: Option<String>,
    pub last_opened_at: Option<String>,
}

/// epoch millis 문자열 → u64 (빈/파싱실패 = 0).
fn ts(s: &Option<String>) -> u64 {
    s.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// 여러 프로젝트 일괄 임포트. 신규 추가 건수 반환.
/// 기존 프로젝트도 xml 이 더 최신(last_opened)이면 preferred/last_opened 갱신.
/// 배치 내 중복은 건너뜀.
pub fn import_projects(items: Vec<ImportItem>) -> Result<usize, String> {
    let mut store = load();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut added = 0usize;
    let mut changed = false;
    let mut order = store.projects.len() as i64;
    for it in items {
        let path = it.path.trim();
        if path.is_empty() {
            continue;
        }
        let id = id_from_path(path);
        if !seen.insert(id.clone()) {
            continue; // 배치 내 중복
        }
        if let Some(existing) = store.projects.iter_mut().find(|p| p.id == id) {
            // 기존 항목: xml 이 더 최신일 때만 갱신(앱-열기 기록 역행 방지).
            if ts(&it.last_opened_at) > ts(&existing.last_opened_at) {
                if it.preferred_ide_id.is_some() {
                    existing.preferred_ide_id = it.preferred_ide_id;
                }
                existing.last_opened_at = it.last_opened_at;
                changed = true;
            }
            continue;
        }
        store.projects.push(Project {
            id,
            name: name_from_path(path),
            path: path.to_string(),
            folder_id: None,
            favorite: false,
            preferred_ide_id: it.preferred_ide_id,
            last_opened_at: it.last_opened_at,
            order,
        });
        order += 1;
        added += 1;
        changed = true;
    }
    if changed {
        save(&store)?;
    }
    Ok(added)
}

/// 폴더 id 생성. 이름+부모+현재 폴더 수로 안정 유니크.
fn folder_id(name: &str, parent_id: Option<&str>, salt: usize) -> String {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    parent_id.unwrap_or("").hash(&mut hasher);
    salt.hash(&mut hasher);
    format!("f{:016x}", hasher.finish())
}

/// 폴더 추가. parent_id None = 루트.
pub fn add_folder(name: &str, parent_id: Option<String>) -> Result<Folder, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("폴더명이 비어 있습니다.".to_string());
    }
    let mut store = load();
    if let Some(pid) = &parent_id {
        if !store.folders.iter().any(|f| &f.id == pid) {
            return Err(format!("부모 폴더 없음: {pid}"));
        }
    }
    let id = folder_id(name, parent_id.as_deref(), store.folders.len());
    let order = store.folders.len() as i64;
    let folder = Folder {
        id,
        name: name.to_string(),
        parent_id,
        order,
    };
    store.folders.push(folder.clone());
    save(&store)?;
    Ok(folder)
}

/// 폴더 이름 변경.
pub fn rename_folder(id: &str, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("폴더명이 비어 있습니다.".to_string());
    }
    let mut store = load();
    let f = store
        .folders
        .iter_mut()
        .find(|f| f.id == id)
        .ok_or_else(|| format!("폴더 없음: {id}"))?;
    f.name = name.to_string();
    save(&store)
}

/// 폴더 제거. 자식 폴더·소속 프로젝트는 삭제 폴더의 부모로 이동(루트면 미분류).
pub fn remove_folder(id: &str) -> Result<(), String> {
    let mut store = load();
    let parent = store
        .folders
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| format!("폴더 없음: {id}"))?
        .parent_id
        .clone();
    store.folders.retain(|f| f.id != id);
    for f in store.folders.iter_mut() {
        if f.parent_id.as_deref() == Some(id) {
            f.parent_id = parent.clone();
        }
    }
    for p in store.projects.iter_mut() {
        if p.folder_id.as_deref() == Some(id) {
            p.folder_id = parent.clone();
        }
    }
    save(&store)
}

/// id 가 ancestor 의 자손(또는 자신)인지. 사이클 방지용.
fn is_descendant(folders: &[Folder], id: &str, ancestor: &str) -> bool {
    let mut cur = Some(id.to_string());
    while let Some(c) = cur {
        if c == ancestor {
            return true;
        }
        cur = folders
            .iter()
            .find(|f| f.id == c)
            .and_then(|f| f.parent_id.clone());
    }
    false
}

/// 폴더 이동(재부모). 자기 자신/자손으로 이동 금지(사이클).
pub fn move_folder(id: &str, parent_id: Option<String>) -> Result<(), String> {
    let mut store = load();
    if !store.folders.iter().any(|f| f.id == id) {
        return Err(format!("폴더 없음: {id}"));
    }
    if let Some(pid) = &parent_id {
        if pid == id || is_descendant(&store.folders, pid, id) {
            return Err("자기 자신 또는 하위 폴더로 이동할 수 없습니다.".to_string());
        }
        if !store.folders.iter().any(|f| &f.id == pid) {
            return Err(format!("대상 폴더 없음: {pid}"));
        }
    }
    let f = store.folders.iter_mut().find(|f| f.id == id).unwrap();
    f.parent_id = parent_id;
    save(&store)
}

/// 프로젝트를 폴더에 배정 + 위치 지정. folder_id None = 미분류.
/// before_id Some 이면 대상 폴더에서 해당 프로젝트 바로 앞에 삽입, None 이면 맨 끝.
/// 대상 폴더 형제들의 order 를 0..n 으로 재할당.
pub fn move_project(
    id: &str,
    folder_id: Option<String>,
    before_id: Option<String>,
) -> Result<(), String> {
    let mut store = load();
    if let Some(fid) = &folder_id {
        if !store.folders.iter().any(|f| &f.id == fid) {
            return Err(format!("폴더 없음: {fid}"));
        }
    }
    if !store.projects.iter().any(|p| p.id == id) {
        return Err(format!("프로젝트 없음: {id}"));
    }
    // 폴더 갱신.
    store
        .projects
        .iter_mut()
        .find(|p| p.id == id)
        .unwrap()
        .folder_id = folder_id.clone();

    // 대상 폴더 형제 id 를 order 순으로.
    let mut ids: Vec<(i64, String)> = store
        .projects
        .iter()
        .filter(|p| p.folder_id == folder_id)
        .map(|p| (p.order, p.id.clone()))
        .collect();
    ids.sort_by_key(|(o, _)| *o);
    let mut ids: Vec<String> = ids.into_iter().map(|(_, id)| id).collect();

    // 대상을 빼고 원하는 위치에 재삽입.
    ids.retain(|x| x != id);
    let pos = match &before_id {
        Some(bid) => ids.iter().position(|x| x == bid).unwrap_or(ids.len()),
        None => ids.len(),
    };
    ids.insert(pos, id.to_string());

    // order 재할당.
    for (i, pid) in ids.iter().enumerate() {
        if let Some(p) = store.projects.iter_mut().find(|p| &p.id == pid) {
            p.order = i as i64;
        }
    }
    save(&store)
}

/// 프로젝트 제거.
pub fn remove_project(id: &str) -> Result<(), String> {
    let mut store = load();
    let before = store.projects.len();
    store.projects.retain(|p| p.id != id);
    if store.projects.len() == before {
        return Err(format!("프로젝트 없음: {id}"));
    }
    save(&store)
}

/// 프로젝트 단건 조회.
pub fn find_project(id: &str) -> Option<Project> {
    load().projects.into_iter().find(|p| p.id == id)
}

/// 열기 시각 + preferredIde 갱신.
pub fn mark_opened(id: &str, ide_id: &str, epoch_millis: u128) -> Result<(), String> {
    let mut store = load();
    let p = store
        .projects
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("프로젝트 없음: {id}"))?;
    p.preferred_ide_id = Some(ide_id.to_string());
    p.last_opened_at = Some(epoch_millis.to_string());
    save(&store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_stable_and_case_insensitive() {
        assert_eq!(id_from_path(r"C:\Dev\Foo"), id_from_path(r"c:\dev\foo"));
        assert_ne!(id_from_path(r"C:\Dev\Foo"), id_from_path(r"C:\Dev\Bar"));
    }

    #[test]
    fn name_is_last_component() {
        assert_eq!(name_from_path(r"C:\Dev\my-proj"), "my-proj");
    }

    #[test]
    fn default_store_is_empty_v1() {
        let s = Store::default();
        assert_eq!(s.version, 1);
        assert!(s.projects.is_empty());
        assert!(s.folders.is_empty());
    }

    #[test]
    fn descendant_detection_blocks_cycle() {
        let folders = vec![
            Folder { id: "a".into(), name: "A".into(), parent_id: None, order: 0 },
            Folder { id: "b".into(), name: "B".into(), parent_id: Some("a".into()), order: 1 },
            Folder { id: "c".into(), name: "C".into(), parent_id: Some("b".into()), order: 2 },
        ];
        // c 는 a 의 자손 → a 를 c 밑으로 이동 금지.
        assert!(is_descendant(&folders, "c", "a"));
        assert!(!is_descendant(&folders, "a", "c"));
    }

    #[test]
    fn folder_id_unique_per_salt() {
        assert_ne!(folder_id("X", None, 0), folder_id("X", None, 1));
    }

    #[test]
    fn deserializes_partial_project() {
        // 옛/부분 데이터도 serde default 로 견딤.
        let json = r#"{"version":1,"projects":[{"id":"p1","name":"X","path":"C:\\X"}]}"#;
        let s: Store = serde_json::from_str(json).unwrap();
        assert_eq!(s.projects.len(), 1);
        assert_eq!(s.projects[0].favorite, false);
        assert!(s.projects[0].folder_id.is_none());
    }
}
