//! JetBrains IDE 최근 프로젝트 임포트.
//! `%APPDATA%\JetBrains\<ProductVersion>\options\recentProjects.xml` 파싱.

use std::path::PathBuf;

/// 최근 프로젝트 1건.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentProject {
    pub path: String,         // 프로젝트 디렉토리 (정방향 슬래시 그대로)
    pub product_code: String, // "IU" / "PY" / "WS" / "DB" ... (없으면 빈 문자열)
    pub last_opened: String,  // epoch millis 문자열 (activation 우선)
}

/// `%APPDATA%\JetBrains` 하위 모든 recentProjects.xml 수집 → 경로 중복 제거(최신 우선).
pub fn recent_projects() -> Vec<RecentProject> {
    let mut all: Vec<RecentProject> = Vec::new();
    for xml in recent_xml_files() {
        if let Ok(text) = std::fs::read_to_string(&xml) {
            all.extend(parse_recent(&text));
        }
    }
    dedup_latest(all)
}

/// JetBrains 설정 폴더들의 recentProjects.xml 경로 목록.
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
        .map(|e| e.path().join("options").join("recentProjects.xml"))
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
}
