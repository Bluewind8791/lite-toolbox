<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getVersion } from "@tauri-apps/api/app";
  import { onMount, onDestroy } from "svelte";

  const minimizeWindow = () => getCurrentWindow().minimize();
  const closeWindow = () => getCurrentWindow().close();

  interface DetectedIde {
    id: string;
    toolName: string;
    productCode: string;
    version: string;
    buildNumber: string;
    channel: string;
    exePath: string;
    source: string;
    iconPath?: string;
  }

  interface Project {
    id: string;
    name: string;
    path: string;
    folderId: string | null;
    favorite: boolean;
    preferredIdeId?: string;
    lastOpenedAt?: string;
    order: number;
  }

  interface Folder {
    id: string;
    name: string;
    parentId: string | null;
    order: number;
  }

  let ides = $state<DetectedIde[]>([]);
  let projects = $state<Project[]>([]);
  let folders = $state<Folder[]>([]);
  // 실제 디렉토리가 없는 프로젝트 id.
  let missing = $state<Set<string>>(new Set());
  let loading = $state(false);
  let error = $state("");
  let launching = $state<string | null>(null);
  let opening = $state<string | null>(null);
  let importing = $state(false);
  let addingIde = $state(false);
  let dataDir = $state("");
  let appVersion = $state("");
  // IDE id → 아이콘 data URL.
  let iconCache = $state<Record<string, string>>({});
  let tab = $state<"project" | "ide" | "settings">("project");
  let search = $state("");
  // 탭바 검색 입력 펼침 상태.
  let searchOpen = $state(false);
  function openSearch() {
    searchOpen = true;
  }
  // 비어 있을 때만 접음 (blur).
  function closeSearchIfEmpty() {
    if (!search.trim()) searchOpen = false;
  }
  // 폴더 접힘 상태 (미지정 = 펼침).
  let collapsed = $state<Record<string, boolean>>({});
  // 인라인 이름편집 중인 폴더.
  let editing = $state<{ id: string; value: string } | null>(null);
  // 드래그 중 프로젝트 id (폴더는 항상 루트, 드래그 불가).
  let dragItem = $state<string | null>(null);
  let dropTarget = $state<string | null | undefined>(undefined);
  // 이 프로젝트 카드 "앞"에 삽입 예정임을 표시.
  let dropBeforeId = $state<string | null>(null);

  function childFolders(parentId: string | null): Folder[] {
    return folders
      .filter((f) => (f.parentId ?? null) === parentId)
      .sort((a, b) => a.order - b.order);
  }
  function projectsIn(folderId: string | null): Project[] {
    return projects
      .filter((p) => (p.folderId ?? null) === folderId)
      .sort((a, b) => a.order - b.order);
  }

  // 부분열(subsequence) fuzzy 매칭.
  function fuzzy(q: string, text: string): boolean {
    if (!q) return true;
    const t = text.toLowerCase();
    let i = 0;
    for (const ch of t) {
      if (ch === q[i]) i++;
      if (i === q.length) return true;
    }
    return false;
  }

  // 검색 결과: 이름/경로 fuzzy → 이름순.
  let results = $derived.by(() => {
    const q = search.trim().toLowerCase();
    if (!q) return [];
    return projects
      .filter((p) => fuzzy(q, p.name) || fuzzy(q, p.path))
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  async function scan() {
    loading = true;
    error = "";
    try {
      ides = await invoke<DetectedIde[]>("detect_ides");
      loadIcons();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // 각 IDE 의 SVG 아이콘을 지연 로드해 data URL 로 캐시.
  async function loadIcons() {
    for (const ide of ides) {
      if (!ide.iconPath || iconCache[ide.id]) continue;
      try {
        const svg = await invoke<string | null>("ide_icon", { path: ide.iconPath });
        if (svg) {
          iconCache[ide.id] = "data:image/svg+xml;utf8," + encodeURIComponent(svg);
        }
      } catch {
        // 아이콘 실패는 무시 — productCode 뱃지로 폴백.
      }
    }
  }

  async function reload() {
    try {
      projects = await invoke<Project[]>("list_projects");
      folders = await invoke<Folder[]>("list_folders");
      missing = new Set(await invoke<string[]>("missing_project_ids"));
    } catch (e) {
      error = String(e);
    }
  }

  // 수동 등록 IDE (설정 화면에서 관리).
  let manualIdes = $derived(ides.filter((i) => i.source === "manual"));

  async function addManualIde() {
    error = "";
    addingIde = true;
    try {
      await invoke("set_ignore_blur", { ignore: true });
      let exe;
      try {
        exe = await open({
          title: "IDE 실행 파일 선택 (*.exe)",
          filters: [{ name: "실행 파일", extensions: ["exe"] }],
        });
      } finally {
        await invoke("set_ignore_blur", { ignore: false });
      }
      if (!exe) return;
      await invoke("add_manual_ide", { path: exe });
      await scan();
    } catch (e) {
      error = String(e);
    } finally {
      addingIde = false;
    }
  }

  async function removeManualIde(ide: DetectedIde) {
    error = "";
    try {
      await invoke("remove_manual_ide", { path: ide.exePath });
      await scan();
    } catch (e) {
      error = String(e);
    }
  }

  async function launch(ide: DetectedIde) {
    launching = ide.id;
    error = "";
    try {
      await invoke("launch_ide", { ideId: ide.id });
    } catch (e) {
      error = String(e);
    } finally {
      launching = null;
    }
  }

  async function addProject() {
    error = "";
    try {
      await invoke("set_ignore_blur", { ignore: true });
      let dir;
      try {
        dir = await open({ directory: true, title: "프로젝트 폴더 선택" });
      } finally {
        await invoke("set_ignore_blur", { ignore: false });
      }
      if (!dir) return;
      await invoke("add_project", { path: dir });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  // silent=true: 자동 10초 폴링용 — 스피너/에러 표시 없음.
  async function importRecent(silent = false) {
    if (!silent) {
      importing = true;
      error = "";
    }
    try {
      await invoke<number>("import_recent_projects");
      await reload();
    } catch (e) {
      if (!silent) error = String(e);
    } finally {
      if (!silent) importing = false;
    }
  }

  async function removeProject(p: Project) {
    error = "";
    try {
      await invoke("remove_project", { id: p.id });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  // 프로젝트가 열릴 IDE — 히스토리(preferred) 우선, 없으면 폴백.
  // preferred 가 탐지 목록에 없으면(옛 id 형식 `{toolId}_{build}`):
  //   1) toolId 부분(첫 `_` 앞) 으로 같은 제품 매칭 → 제품 유지
  //   2) 그래도 없으면 첫 탐지 IDE
  function ideFor(p: Project): string | undefined {
    const pref = p.preferredIdeId;
    if (!pref) return ides[0]?.id;
    if (ides.some((i) => i.id === pref)) return pref;
    const product = pref.split("_")[0];
    return ides.find((i) => i.id === product)?.id ?? ides[0]?.id;
  }
  function ideById(id: string | undefined): DetectedIde | undefined {
    return id ? ides.find((i) => i.id === id) : undefined;
  }

  async function openProject(p: Project) {
    const ideId = ideFor(p);
    if (!ideId) {
      error = "열 IDE 가 없습니다. IDE 를 먼저 탐지하세요.";
      return;
    }
    opening = p.id;
    error = "";
    try {
      await invoke("open_project", { id: p.id, ideId });
      await reload();
    } catch (e) {
      error = String(e);
    } finally {
      opening = null;
    }
  }

  // --- 폴더 ---

  // '새 폴더' 자동 추가. 이름 중복 시 " 2", " 3" … 접미사.
  async function addQuickFolder() {
    error = "";
    const base = "새 폴더";
    const taken = new Set(childFolders(null).map((f) => f.name));
    let name = base;
    for (let i = 2; taken.has(name); i++) name = `${base} ${i}`;
    try {
      await invoke("add_folder", { name, parentId: null });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  function startEdit(f: Folder) {
    editing = { id: f.id, value: f.name };
  }

  async function commitEdit() {
    if (!editing) return;
    const { id, value } = editing;
    const name = value.trim();
    editing = null;
    if (!name) return;
    error = "";
    try {
      await invoke("rename_folder", { id, name });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  async function removeFolder(f: Folder) {
    error = "";
    try {
      await invoke("remove_folder", { id: f.id });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  // 폴더 기본값은 접힘(undefined → 접힘). 한 번이라도 펼치면 false 저장.
  function isCollapsed(id: string): boolean {
    return collapsed[id] ?? true;
  }
  function toggle(id: string) {
    collapsed[id] = !isCollapsed(id);
  }

  // --- 드래그앤드롭 ---

  function startDrag(e: DragEvent, id: string) {
    dragItem = id;
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }
  function allowDrop(e: DragEvent, target: string | null) {
    if (!dragItem) return;
    e.preventDefault();
    dropTarget = target;
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }
  function clearDrop() {
    dropTarget = undefined;
  }
  async function dropOn(e: DragEvent, folderId: string | null) {
    e.preventDefault();
    e.stopPropagation();
    dropTarget = undefined;
    const projectId = dragItem;
    dragItem = null;
    if (!projectId) return;
    error = "";
    try {
      // beforeId 없음 = 폴더 맨 끝에 배치.
      await invoke("move_project", { id: projectId, folderId, beforeId: null });
      await reload();
    } catch (e2) {
      error = String(e2);
    }
  }

  // 카드 위로 드래그: 그 카드 앞에 삽입 예정 표시. (검색 중엔 비활성)
  function allowBefore(e: DragEvent, p: Project) {
    if (!dragItem || dragItem === p.id || search.trim()) return;
    e.preventDefault();
    e.stopPropagation();
    dropBeforeId = p.id;
    dropTarget = undefined;
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }
  function clearBefore() {
    dropBeforeId = null;
  }
  // 카드 위에 드롭: 같은 폴더의 그 카드 앞으로 이동(재정렬).
  async function dropBeforeCard(e: DragEvent, p: Project) {
    if (search.trim()) return;
    e.preventDefault();
    e.stopPropagation();
    const projectId = dragItem;
    dragItem = null;
    dropBeforeId = null;
    if (!projectId || projectId === p.id) return;
    error = "";
    try {
      await invoke("move_project", {
        id: projectId,
        folderId: p.folderId ?? null,
        beforeId: p.id,
      });
      await reload();
    } catch (e2) {
      error = String(e2);
    }
  }

  let refreshTimer: ReturnType<typeof setInterval> | undefined;
  let unlistenFocus: (() => void) | undefined;

  onMount(async () => {
    await scan();
    await reload();
    await importRecent(true); // 시작 시 1회 자동 갱신
    refreshTimer = setInterval(() => importRecent(true), 10000);
    // 트레이 앱 — 창이 파괴되지 않아 onMount 는 1회뿐. 창을 다시 열 때(포커스)
    // 재탐지해 IDE 업데이트 후 버전 표기를 갱신.
    unlistenFocus = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) scan();
    });
    try {
      dataDir = await invoke<string>("data_dir");
      appVersion = await getVersion();
    } catch (e) {
      error = String(e);
    }
  });

  onDestroy(() => {
    clearInterval(refreshTimer);
    unlistenFocus?.();
  });
</script>

<main class="container">
  <div class="glow"></div>

  <header class="topbar" data-tauri-drag-region>
    <div class="brand">
      <img class="brand-logo" src="/tray-icon.png" alt="logo" />
      <div class="brand-text">
        <span class="brand-sub">Lite</span>
        <span class="brand-title">Toolbox</span>
      </div>
    </div>
    <div class="win-controls">
      <button class="win-btn" onclick={minimizeWindow} title="최소화" aria-label="최소화">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="6" y1="12" x2="18" y2="12" /></svg>
      </button>
      <button class="win-btn win-close" onclick={closeWindow} title="닫기" aria-label="닫기">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18" /><line x1="18" y1="6" x2="6" y2="18" /></svg>
      </button>
    </div>
  </header>

  {#if error}
    <p class="error">
      <span>{error}</span>
      <button class="dismiss" onclick={() => (error = "")} title="닫기">✕</button>
    </p>
  {/if}

  <nav class="tabbar">
    <button class="tab" class:on={tab === "ide"} onclick={() => (tab = "ide")}>
      도구
    </button>
    <button class="tab" class:on={tab === "project"} onclick={() => (tab = "project")}>
      프로젝트
    </button>
    <button class="tab" class:on={tab === "settings"} onclick={() => (tab = "settings")}>
      설정
    </button>

    {#if tab === "project"}
      <div class="tab-actions">
        {#if !searchOpen}
          <button
            class="tab-icon"
            onclick={addQuickFolder}
            title="폴더 추가"
            aria-label="폴더 추가"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" /><line x1="12" y1="11" x2="12" y2="17" /><line x1="9" y1="14" x2="15" y2="14" /></svg>
          </button>
          <button class="tab-icon" onclick={addProject} title="프로젝트 추가" aria-label="프로젝트 추가">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" /><line x1="12" y1="18" x2="12" y2="12" /><line x1="9" y1="15" x2="15" y2="15" /></svg>
          </button>
        {/if}
        {#if searchOpen}
          <div class="searchbox">
            <svg viewBox="0 0 24 24" fill="none" stroke="#9a96a0" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7" /><line x1="21" y1="21" x2="16.5" y2="16.5" /></svg>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              autofocus
              placeholder="검색"
              bind:value={search}
              spellcheck="false"
              onblur={closeSearchIfEmpty}
              onkeydown={(e) => {
                if (e.key === "Escape") {
                  search = "";
                  searchOpen = false;
                }
              }}
            />
          </div>
        {:else}
          <button class="tab-icon" onclick={openSearch} title="검색" aria-label="검색">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7" /><line x1="21" y1="21" x2="16.5" y2="16.5" /></svg>
          </button>
        {/if}
      </div>
    {/if}
  </nav>

  <section hidden={tab !== "ide"}>
    <div class="section-head">
      <h2>설치됨</h2>
      <button
        class="tab-icon"
        onclick={scan}
        disabled={loading}
        title={loading ? "스캔 중…" : "재스캔"}
        aria-label="재스캔"
      >
        <svg
          class:spin={loading}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        ><polyline points="23 4 23 10 17 10" /><polyline points="1 20 1 14 7 14" /><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" /></svg>
      </button>
    </div>
    {#if loading && ides.length === 0}
      <p class="muted">IDE 탐지 중…</p>
    {:else if ides.length === 0}
      <p class="muted">
        탐지된 IDE 가 없습니다. 설정 탭에서 실행 파일을 수동 등록할 수 있습니다.
      </p>
    {:else}
      <ul class="ide-list">
        {#each ides as ide (ide.id)}
          <li
            class="ide-card clickable"
            class:busy={launching === ide.id}
            role="button"
            tabindex="0"
            onclick={() => launch(ide)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                launch(ide);
              }
            }}
          >
            {#if iconCache[ide.id]}
              <img class="ide-icon" src={iconCache[ide.id]} alt={ide.productCode} />
            {:else}
              <span class="badge">{ide.productCode}</span>
            {/if}
            <div class="info">
              <div class="name">{ide.toolName}</div>
              <div class="version">{ide.version}</div>
            </div>
            {#if launching === ide.id}
              <span class="launch-state muted">실행 중…</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="project-pane" hidden={tab !== "project"}>
    <div class="proj-scroll">
    {#if search.trim()}
      {#if results.length === 0}
        <p class="muted">검색 결과 없음.</p>
      {:else}
        <ul class="children">
          {#each results as p (p.id)}
            {@render projectCard(p, 0)}
          {/each}
        </ul>
      {/if}
    {:else if projects.length === 0 && folders.length === 0}
      <p class="muted">
        등록된 프로젝트가 없습니다. 설정 탭의 "수동으로 최근 프로젝트 가져오기"로 IDE
        기록을 불러올 수 있습니다.
      </p>
    {:else}
      <ul class="tree">
        {#each childFolders(null) as f (f.id)}
          {@render folderNode(f, 0)}
        {/each}

        <li
          class="unfiled"
          class:drop={dropTarget === null}
          ondragover={(e) => allowDrop(e, null)}
          ondragleave={clearDrop}
          ondrop={(e) => dropOn(e, null)}
        >
          <div class="folder-head" style="padding-left:0">
            <button
              class="chevron"
              class:open={!isCollapsed("__unfiled__")}
              onclick={() => toggle("__unfiled__")}
              aria-label={isCollapsed("__unfiled__") ? "펼치기" : "접기"}
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18" /></svg>
            </button>
            <span class="folder-name">미분류</span>
            <span class="folder-count">{projectsIn(null).length}</span>
          </div>
          {#if !isCollapsed("__unfiled__")}
            <ul class="children">
              {#each projectsIn(null) as p (p.id)}
                {@render projectCard(p, 0)}
              {/each}
            </ul>
          {/if}
        </li>
      </ul>
    {/if}
    </div>
  </section>

  <section class="settings-pane" hidden={tab !== "settings"}>
    <div class="setting-group">
      <div class="section-head">
        <h2>수동 IDE 등록</h2>
        <button
          class="tab-icon"
          onclick={addManualIde}
          disabled={addingIde}
          title="IDE 실행파일 추가"
          aria-label="IDE 실행파일 추가"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" /><line x1="12" y1="18" x2="12" y2="12" /><line x1="9" y1="15" x2="15" y2="15" /></svg>
        </button>
      </div>
      {#if manualIdes.length === 0}
        <p class="muted small">
          자동 탐지되지 않는 IDE 는 우측 아이콘으로 <code>*64.exe</code> 를 직접 등록하세요.
        </p>
      {:else}
        <ul class="ide-list">
          {#each manualIdes as ide (ide.exePath)}
            <li class="ide-card">
              <span class="badge">{ide.productCode}</span>
              <div class="info">
                <div class="name">{ide.toolName}</div>
                <div class="path">{ide.exePath}</div>
              </div>
              <button class="row-icon remove" onclick={() => removeManualIde(ide)} title="등록 해제" aria-label="등록 해제">✕</button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="setting-group">
      <h2>최근 프로젝트</h2>
      <p class="muted small">10초마다 자동 갱신됩니다.</p>
      <button class="wide" onclick={() => importRecent()} disabled={importing}>
        {importing ? "가져오는 중…" : "수동으로 최근 프로젝트 가져오기"}
      </button>
    </div>

    <div class="setting-group">
      <h2>데이터</h2>
      <p class="muted small">저장 위치</p>
      <p class="path-box">{dataDir || "(확인 불가)"}</p>
      <button class="wide" onclick={scan} disabled={loading}>
        {loading ? "스캔 중…" : "IDE 재스캔"}
      </button>
    </div>

    <div class="setting-group">
      <h2>정보</h2>
      <div class="app-version">
        <span>Lite Toolbox</span>
        <span class="ver">v{appVersion || "…"}</span>
      </div>
    </div>
  </section>
</main>

{#snippet folderNode(folder: Folder, depth: number)}
  <li class="folder">
    <div
      class="folder-head"
      class:drop={dropTarget === folder.id}
      ondragover={(e) => allowDrop(e, folder.id)}
      ondragleave={clearDrop}
      ondrop={(e) => dropOn(e, folder.id)}
      style="padding-left:0"
    >
      <button
        class="chevron"
        class:open={!isCollapsed(folder.id)}
        onclick={() => toggle(folder.id)}
        aria-label={isCollapsed(folder.id) ? "펼치기" : "접기"}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18" /></svg>
      </button>
      {#if editing?.id === folder.id}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="rename"
          autofocus
          bind:value={editing.value}
          onblur={commitEdit}
          onkeydown={(e) => {
            if (e.key === "Enter") commitEdit();
            if (e.key === "Escape") editing = null;
          }}
        />
      {:else}
        <span class="folder-name" ondblclick={() => startEdit(folder)}>
          {folder.name}
        </span>
        <span class="folder-count">{projectsIn(folder.id).length}</span>
      {/if}
      <span class="folder-actions">
        <button onclick={() => startEdit(folder)} title="이름 변경">✎</button>
        <button onclick={() => removeFolder(folder)} title="삭제">✕</button>
      </span>
    </div>
    {#if !isCollapsed(folder.id)}
      <ul class="children">
        {#each childFolders(folder.id) as cf (cf.id)}
          {@render folderNode(cf, depth + 1)}
        {/each}
        {#each projectsIn(folder.id) as p (p.id)}
          {@render projectCard(p, depth + 1)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

{#snippet projectCard(p: Project, depth: number)}
  {@const ide = ideById(ideFor(p))}
  <li
    class="proj-card"
    class:drop-before={dropBeforeId === p.id}
    draggable="true"
    ondragstart={(e) => startDrag(e, p.id)}
    ondragover={(e) => allowBefore(e, p)}
    ondragleave={clearBefore}
    ondrop={(e) => dropBeforeCard(e, p)}
    ondblclick={() => openProject(p)}
  >
    {#if ide && iconCache[ide.id]}
      <img class="proj-badge img" src={iconCache[ide.id]} alt={ide.productCode} title={ide.toolName} />
    {:else if ide}
      <span class="proj-badge" title={ide.toolName}>{ide.productCode}</span>
    {:else}
      <span class="proj-badge none" title="IDE 미지정">?</span>
    {/if}
    <div class="proj-info" class:missing={missing.has(p.id)}>
      <div class="proj-name" title={missing.has(p.id) ? "디렉토리 없음" : p.name}>
        {p.name}
      </div>
      <div class="proj-path">{p.path}</div>
    </div>
    <button
      class="row-icon launch"
      onclick={() => openProject(p)}
      disabled={opening === p.id}
      title={opening === p.id ? "여는 중…" : "열기"}
      aria-label="열기"
    >
      {opening === p.id ? "⏳" : "▶"}
    </button>
    <button class="row-icon remove" onclick={() => removeProject(p)} title="제거" aria-label="제거">✕</button>
  </li>
{/snippet}

<style>
  :root {
    font-family: Inter, system-ui, Avenir, Helvetica, Arial, sans-serif;
    color: #e8e6ea;
    background-color: #12181c;
  }

  :global(html),
  :global(body) {
    height: 100%;
    margin: 0;
    background-color: #12181c;
  }

  :global(::-webkit-scrollbar) {
    width: 8px;
  }
  :global(::-webkit-scrollbar-thumb) {
    background: #ffffff1a;
    border-radius: 8px;
  }
  :global(::-webkit-scrollbar-track) {
    background: transparent;
  }

  .container {
    position: relative;
    max-width: 640px;
    margin: 0 auto;
    padding: 0 0.75rem 0.75rem;
    height: 100vh;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    color: #e8e6ea;
    background: #12181c;
    overflow: hidden;
  }

  /* 상단 앰비언트 글로우. */
  .glow {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 180px;
    background: radial-gradient(120% 100% at 8% 0%, #1f5f7a 0%, #183c44 45%, #12181c 100%);
    pointer-events: none;
    z-index: 0;
  }
  .container > *:not(.glow) {
    position: relative;
    z-index: 1;
  }

  /* 보이는 탭 섹션이 남은 높이를 채움. 기본은 섹션 전체 스크롤(IDE/설정). */
  main > section:not([hidden]) {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  /* 프로젝트 탭: 폴더추가는 고정, 트리만 스크롤. */
  section.project-pane:not([hidden]) {
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .proj-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.4rem 0 1rem;
  }

  /* --- 상단바 / 브랜드 --- */
  .topbar {
    display: flex;
    align-items: center;
    height: 64px;
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 11px;
  }
  .win-controls {
    margin-left: auto;
    display: flex;
    gap: 2px;
  }
  .win-btn {
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: #b8b4bd;
    border-radius: 7px;
    cursor: pointer;
    padding: 0;
  }
  .win-btn svg {
    width: 16px;
    height: 16px;
  }
  .win-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #fff;
  }
  .win-close:hover {
    background: #c0392b;
    color: #fff;
  }
  .brand-logo {
    width: 36px;
    height: 36px;
    object-fit: contain;
  }
  .brand-text {
    display: flex;
    flex-direction: column;
    line-height: 1.05;
  }
  .brand-sub {
    font-size: 11px;
    font-weight: 500;
    color: #b8b4bd;
  }
  .brand-title {
    font-size: 18px;
    font-weight: 700;
    letter-spacing: -0.3px;
    color: #fff;
  }

  /* --- 탭바 --- */
  .tabbar {
    display: flex;
    align-items: center;
    gap: 26px;
    height: 46px;
    border-bottom: 1px solid #ffffff10;
    flex: none;
  }
  .tab {
    height: 46px;
    padding: 0;
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    box-shadow: none;
    border-radius: 0;
    font-size: 14px;
    font-weight: 500;
    color: #8f8b96;
    cursor: pointer;
  }
  .tab.on {
    font-weight: 600;
    color: #fff;
    box-shadow: inset 0 -2px 0 #ffffff;
  }
  .tab:hover:not(.on) {
    color: #c4c0ca;
  }
  .tab-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tab-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: none;
    border-radius: 7px;
    background: transparent;
    box-shadow: none;
    color: #9a96a0;
    font-size: 15px;
    cursor: pointer;
  }
  .tab-icon svg {
    width: 17px;
    height: 17px;
  }
  .tab-icon:hover:not(:disabled) {
    background: #ffffff12;
    color: #e8e6ea;
  }
  .tab-icon .spin {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 150px;
    height: 34px;
  }
  .searchbox svg {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  .searchbox input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: #e8e6ea;
    font-family: inherit;
    font-size: 14px;
    padding: 0;
  }

  h2 {
    font-size: 0.9rem;
    margin: 1rem 0 0.5rem;
    color: #c4c0ca;
    font-weight: 600;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
  }

  /* --- 설정 탭 --- */
  .settings-pane:not([hidden]) {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 12px;
  }
  .setting-group {
    background: #ffffff08;
    border: 1px solid #ffffff0d;
    border-radius: 10px;
    padding: 12px 14px;
  }
  .setting-group h2 {
    margin: 0 0 6px;
  }
  .setting-group .section-head {
    margin-bottom: 6px;
  }
  .setting-group .section-head h2 {
    margin: 0;
  }
  .setting-group .ide-list {
    margin-top: 4px;
  }
  .setting-group .ide-card {
    background: #ffffff0d;
  }
  button.wide {
    width: 100%;
    margin-top: 8px;
  }

  .ide-list {
    list-style: none;
    margin: 0.4rem 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .tree,
  .children {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .children {
    margin: 2px 0 2px 15px;
    padding-left: 9px;
    border-left: 1px solid #ffffff12;
  }

  /* --- 폴더 헤더 --- */
  .folder-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 8px;
    border-radius: 8px;
    cursor: pointer;
    user-select: none;
  }
  .folder-head:hover {
    background: #ffffff08;
  }
  .folder-head.drop {
    outline: 2px dashed #4fd0c0;
    background: #4fd0c014;
  }
  .chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    background: transparent;
    box-shadow: none;
    color: #8f8b96;
    cursor: pointer;
  }
  .chevron svg {
    width: 12px;
    height: 12px;
    transition: transform 0.15s ease;
  }
  .chevron.open svg {
    transform: rotate(90deg);
  }
  .folder-name {
    font-size: 12px;
    font-weight: 600;
    color: #c4c0ca;
    letter-spacing: 0.1px;
    cursor: default;
  }
  .folder-count {
    font-size: 11px;
    font-weight: 500;
    color: #6f6b77;
  }
  .rename {
    flex: 1;
    padding: 0.15em 0.4em;
    border-radius: 5px;
    border: 1px solid #4fd0c0;
    background: #ffffff0d;
    color: #e8e6ea;
    font-family: inherit;
    font-size: 12px;
  }
  .rename:focus {
    outline: none;
  }
  .folder-actions {
    display: flex;
    gap: 0.2rem;
    margin-left: auto;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .folder-head:hover .folder-actions {
    opacity: 1;
  }
  .folder-actions button {
    padding: 0.1em 0.4em;
    box-shadow: none;
    background: transparent;
    border: none;
    color: #8f8b96;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .folder-actions button:hover {
    color: #e8e6ea;
  }

  .unfiled.drop {
    outline: 2px dashed #4fd0c0;
    border-radius: 8px;
  }

  /* --- IDE 카드 --- */
  .ide-card {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 10px;
    background: #ffffff08;
    border: 1px solid #ffffff0d;
    border-radius: 10px;
  }
  .ide-card.clickable {
    cursor: pointer;
  }
  .ide-card.clickable:hover {
    background: #ffffff0d;
    border-color: #4fd0c040;
  }
  .ide-card.busy {
    opacity: 0.6;
    cursor: default;
  }
  .launch-state {
    flex: none;
    font-size: 0.8rem;
  }
  .ide-icon {
    flex: none;
    width: 34px;
    height: 34px;
    object-fit: contain;
  }
  .badge {
    flex: none;
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 0.8rem;
    color: #fff;
    background: linear-gradient(135deg, #1f6f7a, #2a8f8a);
    border-radius: 9px;
  }

  /* --- 프로젝트 카드 --- */
  .proj-card {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 5px 10px;
    border-radius: 9px;
    cursor: grab;
    position: relative;
  }
  .proj-card:hover {
    background: #ffffff0d;
  }
  .proj-card.drop-before::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: -2px;
    height: 2px;
    background: #4fd0c0;
    border-radius: 2px;
  }
  .proj-badge {
    flex: none;
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 0.65rem;
    color: #fff;
    background: linear-gradient(135deg, #1f6f7a, #2a8f8a);
    border-radius: 8px;
    object-fit: contain;
  }
  .proj-badge.img {
    background: #ffffff0d;
    padding: 3px;
  }
  .proj-badge.none {
    background: #ffffff14;
    color: #8b8792;
  }

  .info {
    flex: 1;
    min-width: 0;
  }
  .proj-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .proj-info.missing .proj-name,
  .proj-info.missing .proj-path {
    color: #6f6b77;
  }
  .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .proj-name {
    font-size: 13.5px;
    font-weight: 600;
    color: #f0eef2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .version {
    font-size: 0.8rem;
    color: #9a96a0;
  }
  .path,
  .proj-path {
    font-size: 11px;
    color: #8b8792;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* --- 버튼 (일반) --- */
  button {
    border-radius: 8px;
    border: 1px solid #ffffff1a;
    padding: 0.4em 1em;
    font-size: 0.85em;
    font-weight: 500;
    font-family: inherit;
    color: #e8e6ea;
    background-color: #ffffff0d;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    border-color: #4fd0c0;
    color: #7ee3d6;
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  /* 카드 우측 아이콘 버튼. */
  .row-icon {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: #7e7a86;
    font-size: 0.85rem;
    line-height: 1;
  }
  .row-icon:hover:not(:disabled) {
    background: #ffffff14;
    color: #e8e6ea;
    border: none;
  }
  .row-icon.remove:hover:not(:disabled) {
    color: #f04986;
  }

  .error {
    color: #ff9aa6;
    background: #f0498618;
    border: 1px solid #f0498633;
    padding: 0.5rem 0.75rem;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin: 0.5rem 0 0;
  }
  .error .dismiss {
    flex: none;
    padding: 0.1em 0.45em;
    color: #ff9aa6;
    background: transparent;
    border: none;
    box-shadow: none;
  }

  .muted {
    color: #8b8792;
  }
  .small {
    font-size: 0.75rem;
    margin-bottom: 0.2rem;
  }

  .path-box {
    font-size: 0.78rem;
    font-family: ui-monospace, monospace;
    background: #ffffff0d;
    border: 1px solid #ffffff1a;
    border-radius: 8px;
    padding: 0.4rem 0.5rem;
    word-break: break-all;
    margin: 0 0 0.75rem;
    color: #c4c0ca;
  }

  .app-version {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.82rem;
    color: #c4c0ca;
  }
  .app-version .ver {
    font-family: ui-monospace, monospace;
    color: #4fd6d6;
  }

  code {
    background: #ffffff12;
    border-radius: 4px;
    padding: 0.05em 0.35em;
    font-size: 0.85em;
  }
</style>
