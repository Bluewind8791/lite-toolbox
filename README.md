# Lite Toolbox

경량 JetBrains 런처 (Windows 전용). IDE 자동 탐지·실행 + 프로젝트 폴더 분류·검색.
JetBrains Toolbox 의 무거운 기능(다운로드/설치/업데이트/원격개발)은 제외.

## 스택

| 레이어 | 선택 |
|--------|------|
| 셸 | Tauri 2.x |
| 백엔드 | Rust (IDE 탐지, 프로세스 실행, 영속화) |
| 프론트 | SvelteKit + TypeScript (adapter-static) |
| 저장 | 로컬 JSON (`%APPDATA%\LiteToolbox`) |

## 데이터 모델

- `data.json` — `projects[]`, `folders[]`
- `config.json` — `manualIdes[]` (수동 등록 IDE exe 경로)

```ts
interface Folder { id; name; parentId: string | null; order }
interface Project {
  id; name; path; folderId: string | null;
  preferredIdeId?; lastOpenedAt?; order;
}
```

> `Project.favorite` 필드는 모델에 잔존하나 미사용(즐겨찾기 기능 철회, 폴더 트리로 분류).

## IDE 자동 탐지 — `src-tauri/src/ide_detect.rs`

3개 소스를 병합, **exe 경로(대소문자 무시)로 중복 제거**:

1. **Toolbox** — `%LOCALAPPDATA%\JetBrains\Toolbox\state.json`.
   `tools[].launchCommand` 가 실행파일 절대경로를 직접 제공 → 디렉토리 스캔 불필요.
2. **독립 설치** — `C:\Program Files\JetBrains\*`, `…\Android\*`(32/64비트 Program Files) 하위
   `product-info.json` 스캔 (`launcherPath` / `productCode` / `buildNumber`). 레지스트리 의존 없음.
3. **수동 등록** — `config.json` 의 exe 경로. 설치 루트(exe 부모의 부모)의
   `product-info.json` 으로 메타 보강, 실패 시 파일명 폴백(`source: "manual"`).

**경로 주의**: 최신 Toolbox(3.x)는 `%LOCALAPPDATA%\Programs\<Product>\bin\` 에 설치
(구버전 `apps\<tool>\ch-…` 가정 폐기).

**폴백 체인**: `state.json` 파싱 → 디렉토리 스캔(`product-info.json`) → 수동 등록.
포맷이 바뀌어도 fixture 테스트(`tests/fixtures/state.json`)가 회귀를 잡고, 폴백이 동작을 유지.

**아이콘**: `product-info.json` 의 `svgIconPath`(예: `bin/idea.svg`) → SVG 절대경로.
`ide_icon(path)` 명령이 SVG 텍스트 반환 → 프론트가 `data:image/svg+xml` URL 로 렌더(실패 시 productCode 뱃지).

## 프로젝트 실행 — `launcher.rs`

- `<ide_exe> "<project_path>"` detached spawn → 런처 종료해도 IDE 유지.
- IDE 결정: `preferredIdeId` 우선, 없으면 드롭다운 선택 → 사용된 IDE 를 `preferredIdeId` 로 저장.
- **최근 프로젝트 임포트**(`recent.rs`): 모든 `%APPDATA%\JetBrains\*\options\recentProjects.xml`
  스캔 → 경로/productionCode/최근시각 → 중복 제거(최신 우선) → productCode↔IDE 매칭으로 preferredIde 지정.

## UI

- **IDE 탭(설치됨)**: 아이콘/뱃지 + 버전, 실행.
- **프로젝트 탭**: 상단 검색(이름/경로 fuzzy), 폴더 트리(기본 접힘 · **평면** — 하위 폴더 없음),
  HTML5 DnD 로 프로젝트 ↔ 폴더 이동, 실제 디렉토리 없는 프로젝트는 회색 표시.
- **설정 탭**: 수동 IDE 등록/해제, 데이터 경로, IDE 재스캔.
- 창 우측 하단 고정 + 이동 스냅백 (`window_pos.rs`, 작업영역 기준).

> Tauri webview 기본 `dragDropEnabled` 를 `false` 로 꺼야 HTML5 DnD 가 동작 (`tauri.conf.json`).

## 빌드 / 테스트

```sh
npm run tauri dev       # 개발(HMR + debug 백엔드)
npm run tauri build     # NSIS 인스톨러(targets: nsis) + 단독 exe
cargo test              # ide_detect / store 단위 테스트
npm run build           # 프론트 정적 빌드
```

산출물: `src-tauri/target/release/bundle/nsis/Lite Toolbox_<ver>_x64-setup.exe`, `…/release/lite-toolbox.exe`.

## 리스크 / 메모

- Toolbox `state.json` · `product-info.json` 포맷은 비공개 → 버전업 시 변경 가능.
  fixture 테스트가 변경을 감지하고, 폴백 체인이 동작을 유지.
- 같은 제품 다중 버전 구분 키: `productCode` + `buildNumber`.
- 폴더 구조는 자체 메타데이터(파일시스템 폴더 아님) → 프로젝트 실제 경로와 분리.
