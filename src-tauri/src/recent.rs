//! JetBrains IDE 최근 프로젝트 임포트.
//! `%APPDATA%\JetBrains\<ProductVersion>\options\recentProjects.xml` 파싱.
//! Rider 는 같은 구조를 `recentSolutions.xml` 에 저장 → 두 파일명 모두 수집.
//! 경로에 쓰이는 JetBrains 매크로(`$USER_HOME$` 등)는 해석 후 실존하는 것만 남김.

use std::path::PathBuf;

/// 최근 프로젝트 1건.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentProject {
    pub path: String,         // 프로젝트 디렉토리 (정방향 슬래시 그대로)
    pub product_code: String, // "IU" / "PY" / "WS" / "DB" ... (없으면 빈 문자열)
    pub last_opened: String,  // epoch millis 문자열 (activation 우선)
}

/// 최근 프로젝트가 기록되는 파일명들. (Rider 만 recentSolutions.xml)
const RECENT_FILES: [&str; 2] = ["recentProjects.xml", "recentSolutions.xml"];

/// `%APPDATA%\JetBrains` 하위 모든 최근 프로젝트 XML 수집 → 매크로 해석/실존 확인 →
/// 경로 중복 제거(최신 우선).
pub fn recent_projects() -> Vec<RecentProject> {
    let mut all: Vec<RecentProject> = Vec::new();
    for xml in recent_xml_files() {
        if let Ok(text) = std::fs::read_to_string(&xml) {
            all.extend(parse_recent(&text));
        }
    }
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    dedup_latest(resolve_paths(all, &home))
}

/// 경로 매크로를 해석하고 실존하는 항목만 남김.
fn resolve_paths(items: Vec<RecentProject>, home: &str) -> Vec<RecentProject> {
    items
        .into_iter()
        .filter_map(|mut it| {
            it.path = resolve_macro(&it.path, home)?;
            std::path::Path::new(&it.path).exists().then_some(it)
        })
        .collect()
}

/// `$USER_HOME$` 만 해석. 그 외 매크로는 None —
/// `$APPLICATION_CONFIG_DIR$/light-edit` 는 IDE 설정 폴더의 LightEdit 항목이라
/// 경로가 실존해도 프로젝트가 아니고, 미지의 매크로는 해석할 수 없다.
fn resolve_macro(raw: &str, home: &str) -> Option<String> {
    if let Some(rest) = raw.strip_prefix("$USER_HOME$") {
        if home.is_empty() {
            return None;
        }
        return Some(format!("{}{}", home.replace('\\', "/"), rest));
    }
    if raw.starts_with('$') {
        return None;
    }
    Some(raw.to_string())
}

/// JetBrains 설정 폴더들의 최근 프로젝트 XML 경로 목록.
fn recent_xml_files() -> Vec<PathBuf> {
    let Ok(appdata) = std::env::var("APPDATA") else {
        return Vec::new();
    };
    let root = PathBuf::from(appdata).join("JetBrains");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .flat_map(|e| {
            let options = e.path().join("options");
            RECENT_FILES.iter().map(move |f| options.join(f))
        })
        .filter(|p| p.is_file())
        .collect()
}

/// XML 문자열 파싱 → RecentProject 목록.
fn parse_recent(xml: &str) -> Vec<RecentProject> {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // additionalInfo > map > entry(key=경로) > value > RecentProjectMetaInfo(option...)
    for entry in doc.descendants().filter(|n| n.has_tag_name("entry")) {
        let Some(path) = entry.attribute("key") else {
            continue;
        };
        // 부모 map 의 부모가 additionalInfo 인 entry 만 (다른 map 의 entry 배제)
        let in_additional = entry
            .parent()
            .and_then(|m| m.parent())
            .map(|o| o.attribute("name") == Some("additionalInfo"))
            .unwrap_or(false);
        if !in_additional {
            continue;
        }
        let meta = entry
            .descendants()
            .find(|n| n.has_tag_name("RecentProjectMetaInfo"));
        let (mut product_code, mut activation, mut open_ts) =
            (String::new(), String::new(), String::new());
        if let Some(meta) = meta {
            for opt in meta.children().filter(|n| n.has_tag_name("option")) {
                match opt.attribute("name") {
                    Some("productionCode") => {
                        product_code = opt.attribute("value").unwrap_or("").to_string()
                    }
                    Some("activationTimestamp") => {
                        activation = opt.attribute("value").unwrap_or("").to_string()
                    }
                    Some("projectOpenTimestamp") => {
                        open_ts = opt.attribute("value").unwrap_or("").to_string()
                    }
                    _ => {}
                }
            }
        }
        let last_opened = if !activation.is_empty() {
            activation
        } else {
            open_ts
        };
        out.push(RecentProject {
            path: path.to_string(),
            product_code,
            last_opened,
        });
    }
    out
}

/// 경로(대소문자 무시) 중복 제거 — last_opened 큰 값 유지.
fn dedup_latest(items: Vec<RecentProject>) -> Vec<RecentProject> {
    use std::collections::HashMap;
    let mut map: HashMap<String, RecentProject> = HashMap::new();
    for it in items {
        let key = it.path.to_lowercase();
        match map.get(&key) {
            Some(existing) if ts(&existing.last_opened) >= ts(&it.last_opened) => {}
            _ => {
                map.insert(key, it);
            }
        }
    }
    let mut v: Vec<RecentProject> = map.into_values().collect();
    v.sort_by(|a, b| ts(&b.last_opened).cmp(&ts(&a.last_opened)));
    v
}

fn ts(s: &str) -> u64 {
    s.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<application>
  <component name="RecentProjectsManager">
    <option name="additionalInfo">
      <map>
        <entry key="D:/yk/proj-a">
          <value>
            <RecentProjectMetaInfo frameTitle="proj-a">
              <option name="activationTimestamp" value="1770629900409" />
              <option name="productionCode" value="IU" />
              <option name="projectOpenTimestamp" value="1770620819528" />
            </RecentProjectMetaInfo>
          </value>
        </entry>
        <entry key="D:/yk/proj-b">
          <value>
            <RecentProjectMetaInfo frameTitle="proj-b">
              <option name="productionCode" value="PY" />
              <option name="projectOpenTimestamp" value="1770000000000" />
            </RecentProjectMetaInfo>
          </value>
        </entry>
      </map>
    </option>
  </component>
</application>"#;

    #[test]
    fn parses_entries_with_code_and_ts() {
        let r = parse_recent(SAMPLE);
        assert_eq!(r.len(), 2);
        let a = r.iter().find(|x| x.path == "D:/yk/proj-a").unwrap();
        assert_eq!(a.product_code, "IU");
        assert_eq!(a.last_opened, "1770629900409"); // activation 우선
        let b = r.iter().find(|x| x.path == "D:/yk/proj-b").unwrap();
        assert_eq!(b.product_code, "PY");
        assert_eq!(b.last_opened, "1770000000000"); // activation 없으면 open_ts
    }

    #[test]
    fn dedup_keeps_latest() {
        let items = vec![
            RecentProject {
                path: "D:/X".into(),
                product_code: "IU".into(),
                last_opened: "100".into(),
            },
            RecentProject {
                path: "d:/x".into(),
                product_code: "PY".into(),
                last_opened: "200".into(),
            },
        ];
        let d = dedup_latest(items);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].last_opened, "200");
        assert_eq!(d[0].product_code, "PY");
    }

    #[test]
    fn ignores_non_additional_entries() {
        // additionalInfo 밖의 entry 는 무시.
        let xml = r#"<application><component>
          <option name="other"><map>
            <entry key="D:/ignore"><value><RecentProjectMetaInfo/></value></entry>
          </map></option>
        </component></application>"#;
        assert!(parse_recent(xml).is_empty());
    }

    #[test]
    fn malformed_xml_yields_empty() {
        assert!(parse_recent("<not closed").is_empty());
    }

    /// Rider recentSolutions.xml — 컴포넌트명이 RiderRecentProjectsManager 이고
    /// entry key 가 .sln 파일 경로. 구조는 동일하므로 같은 파서로 처리돼야 함.
    #[test]
    fn parses_rider_recent_solutions() {
        let xml = r#"<application>
  <component name="RiderRecentProjectsManager">
    <option name="additionalInfo">
      <map>
        <entry key="D:/yk/YKSecurity_windows/YKSecure.sln">
          <value>
            <RecentProjectMetaInfo displayName="YKSecure" opened="true">
              <option name="activationTimestamp" value="1786578344743" />
              <option name="build" value="RD-262.8665.400" />
              <option name="productionCode" value="RD" />
              <option name="projectOpenTimestamp" value="1786423391910" />
            </RecentProjectMetaInfo>
          </value>
        </entry>
      </map>
    </option>
    <option name="lastOpenedProject" value="D:/yk/YKSecurity_windows/YKSecure.sln" />
  </component>
</application>"#;
        let r = parse_recent(xml);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "D:/yk/YKSecurity_windows/YKSecure.sln");
        assert_eq!(r[0].product_code, "RD");
        assert_eq!(r[0].last_opened, "1786578344743");
    }

    #[test]
    fn expands_user_home_macro() {
        assert_eq!(
            resolve_macro("$USER_HOME$/PyCharmMiscProject", r"C:\Users\castu").as_deref(),
            Some("C:/Users/castu/PyCharmMiscProject")
        );
        // 홈을 모르면 해석 불가.
        assert_eq!(resolve_macro("$USER_HOME$/x", ""), None);
    }

    #[test]
    fn drops_unresolvable_macros() {
        // LightEdit — 폴더는 실존하지만 프로젝트가 아님.
        assert_eq!(
            resolve_macro("$APPLICATION_CONFIG_DIR$/light-edit", r"C:\Users\castu"),
            None
        );
        assert_eq!(resolve_macro("$UNKNOWN$/x", r"C:\Users\castu"), None);
        // 일반 경로는 그대로.
        assert_eq!(
            resolve_macro("D:/yk/proj-a", r"C:\Users\castu").as_deref(),
            Some("D:/yk/proj-a")
        );
    }

    #[test]
    fn keeps_only_existing_paths() {
        let items = vec![
            RecentProject {
                path: env!("CARGO_MANIFEST_DIR").replace('\\', "/"),
                product_code: "IU".into(),
                last_opened: "100".into(),
            },
            RecentProject {
                path: "D:/definitely/not/here/xyz".into(),
                product_code: "PY".into(),
                last_opened: "200".into(),
            },
            RecentProject {
                path: "$APPLICATION_CONFIG_DIR$/light-edit".into(),
                product_code: String::new(),
                last_opened: "300".into(),
            },
        ];
        let kept = resolve_paths(items, r"C:\Users\castu");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].product_code, "IU");
    }

    #[test]
    #[ignore = "실제 설치 환경 의존 — 수동 확인용 (cargo test -- --ignored --nocapture)"]
    fn live_recent_smoke() {
        for f in recent_xml_files() {
            println!("  xml: {}", f.display());
        }
        for p in recent_projects() {
            println!("  [{}] {} ({})", p.product_code, p.path, p.last_opened);
        }
    }

    #[test]
    fn collects_both_recent_file_names() {
        assert!(RECENT_FILES.contains(&"recentProjects.xml"));
        assert!(RECENT_FILES.contains(&"recentSolutions.xml"));
    }
}
