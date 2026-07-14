# Lite Toolbox — 프로젝트 메모

경량 JetBrains 런처 (Tauri 2 + SvelteKit). 상세는 [README.md](./README.md).

## 버전 / 릴리즈

**버전 출처**: `package.json` 이 단일 출처. `tauri.conf.json` 의 `version` 은
`"../package.json"` 참조라 앱/번들 버전을 자동 상속. `Cargo.toml` 은 Rust 크레이트
버전(별개) — 앱 버전과 맞추려면 같이 올릴 것. 프론트는 `getVersion()` 런타임 조회
(하드코딩 없음, 설정 탭 '정보'에 표시).

**새 버전 올리는 절차** (예: `0.1.1` → `0.1.2`):

```sh
# 1. 버전 수정 — package.json + src-tauri/Cargo.toml 둘 다 X.Y.Z 로
#    (tauri.conf.json 은 package.json 을 참조하므로 수정 불필요)

# 2. 빌드 + exe 배치 (실행 앱 종료 → 빌드 → 포터블 exe 를 repo root 로)
build.bat

# 3. 커밋 + 푸시
git add -A
git commit -m "vX.Y.Z: <요약>"
git push

# 4. GitHub 릴리즈 생성 (포터블 + NSIS 설치본 첨부)
gh release create vX.Y.Z \
  lite-toolbox.exe \
  "src-tauri/target/release/bundle/nsis/Lite Toolbox_X.Y.Z_x64-setup.exe" \
  --title "vX.Y.Z" --notes "..."
```

- `build.bat`: 실행 중 `lite-toolbox.exe` 종료 → `npm run tauri build` → 포터블 exe 를
  root 로 이동. 설치본(`bundle/nsis/`)은 릴리즈 업로드용으로 보존.
- `bundle/nsis/` 에 구버전 설치본이 누적됨 — 4번에서 **올릴 버전 파일명** 확인.
- 커밋 시 root `lite-toolbox.exe`(추적됨)도 포함 — 재빌드 시 비결정적 바이너리
  diff 가 생기니 실제 코드 변경 없으면 `git checkout -- lite-toolbox.exe` 로 되돌릴 것.

## 빌드 / 테스트

```sh
npm run tauri dev    # 개발 (HMR)
cargo test           # ide_detect / store 단위 테스트 (src-tauri 에서)
npm run check        # 프론트 타입 체크
```
