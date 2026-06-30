<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

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
  // IDE id → 아이콘 data URL.
  let iconCache = $state<Record<string, string>>({});
  let tab = $state<"project" | "ide" | "settings">("project");
  let showFolderAdd = $state(false);
  let search = $state("");
  // 프로젝트별 선택 IDE (preferred 없을 때 사용자 선택).
  let pickedIde = $state<Record<string, string>>({});
  // 폴더 접힘 상태 (미지정 = 펼침).
  let collapsed = $state<Record<string, boolean>>({});
  // 인라인 이름편집 중인 폴더.
  let editing = $state<{ id: string; value: string } | null>(null);
  // 루트 폴더 새 이름 입력.
  let newRoot = $state("");
  // 드래그 중 프로젝트 id (폴더는 항상 루트, 드래그 불가).
  let dragItem = $state<string | null>(null);
  let dropTarget = $state<string | null | undefined>(undefined);

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
      const exe = await open({
        title: "IDE 실행 파일 선택 (*.exe)",
        filters: [{ name: "실행 파일", extensions: ["exe"] }],
      });
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
      const dir = await open({ directory: true, title: "프로젝트 폴더 선택" });
      if (!dir) return;
      await invoke("add_project", { path: dir });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  async function importRecent() {
    importing = true;
    error = "";
    try {
      const added = await invoke<number>("import_recent_projects");
      await reload();
      if (added === 0) error = "새로 가져온 프로젝트가 없습니다.";
    } catch (e) {
      error = String(e);
    } finally {
      importing = false;
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

  function ideFor(p: Project): string | undefined {
    return pickedIde[p.id] ?? p.preferredIdeId ?? ides[0]?.id;
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

  async function addRootFolder() {
    const name = newRoot.trim();
    if (!name) return;
    error = "";
    try {
      await invoke("add_folder", { name, parentId: null });
      newRoot = "";
      showFolderAdd = false;
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
      await invoke("move_project", { id: projectId, folderId });
      await reload();
    } catch (e2) {
      error = String(e2);
    }
  }

  onMount(async () => {
    await scan();
    await reload();
    try {
      dataDir = await invoke<string>("data_dir");
    } catch (e) {
      error = String(e);
    }
  });
</script>

<main class="container">
  <header>
    <h1>Lite Toolbox</h1>
    <button onclick={scan} disabled={loading}>
      {loading ? "스캔 중…" : "재스캔"}
    </button>
  </header>

  {#if error}
    <p class="error">
      <span>{error}</span>
      <button class="dismiss" onclick={() => (error = "")} title="닫기">✕</button>
    </p>
  {/if}

  <div class="tabs">
    <button class:active={tab === "ide"} onclick={() => (tab = "ide")}>
      IDE
    </button>
    <button class:active={tab === "project"} onclick={() => (tab = "project")}>
      프로젝트
    </button>
    <button class:active={tab === "settings"} onclick={() => (tab = "settings")}>
      설정
    </button>
  </div>

  <section hidden={tab !== "ide"}>
    <h2>설치됨</h2>
    {#if loading && ides.length === 0}
      <p class="muted">IDE 탐지 중…</p>
    {:else if ides.length === 0}
      <p class="muted">
        탐지된 IDE 가 없습니다. 설정 탭에서 실행 파일을 수동 등록할 수 있습니다.
      </p>
    {:else}
      <ul class="ide-list">
        {#each ides as ide (ide.id)}
          <li class="ide-card">
            {#if iconCache[ide.id]}
              <img class="ide-icon" src={iconCache[ide.id]} alt={ide.productCode} />
            {:else}
              <span class="badge">{ide.productCode}</span>
            {/if}
            <div class="info">
              <div class="name">{ide.toolName}</div>
              <div class="version">{ide.version}</div>
            </div>
            <button
              class="launch"
              onclick={() => launch(ide)}
              disabled={launching === ide.id}
            >
              {launching === ide.id ? "실행 중…" : "실행"}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section hidden={tab !== "project"}>
    <div class="section-head">
      <div class="search">
        <input
          placeholder="프로젝트 검색 (이름/경로)"
          bind:value={search}
          spellcheck="false"
        />
        {#if search}
          <button class="icon" onclick={() => (search = "")} title="검색 지우기">✕</button>
        {/if}
      </div>
      <div class="head-btns">
        <button
          class="icon"
          onclick={importRecent}
          disabled={importing}
          title="최근 프로젝트 가져오기"
        >
          {importing ? "⏳" : "🕘"}
        </button>
        <button
          class="icon"
          onclick={() => (showFolderAdd = !showFolderAdd)}
          title="폴더 추가"
        >
          📁
        </button>
        <button class="icon" onclick={addProject} title="프로젝트 추가">➕</button>
      </div>
    </div>

    {#if showFolderAdd}
      <div class="folder-add">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          placeholder="새 폴더 이름"
          autofocus
          bind:value={newRoot}
          onkeydown={(e) => {
            if (e.key === "Enter") addRootFolder();
            if (e.key === "Escape") {
              newRoot = "";
              showFolderAdd = false;
            }
          }}
        />
        <button onclick={addRootFolder}>추가</button>
        <button
          onclick={() => {
            newRoot = "";
            showFolderAdd = false;
          }}>취소</button
        >
      </div>
    {/if}

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
        등록된 프로젝트가 없습니다. "최근 프로젝트 가져오기"로 IDE 기록을 불러올 수
        있습니다.
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
          <div class="folder-head">
            <button class="twist" onclick={() => toggle("__unfiled__")}>
              {isCollapsed("__unfiled__") ? "▸" : "▾"}
            </button>
            <span class="folder-name muted">미분류 / 루트</span>
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
  </section>

  <section hidden={tab !== "settings"}>
    <div class="section-head">
      <h2>수동 IDE 등록</h2>
      <button class="icon" onclick={addManualIde} disabled={addingIde} title="IDE 실행파일 추가">
        {addingIde ? "⏳" : "➕"}
      </button>
    </div>
    {#if manualIdes.length === 0}
      <p class="muted">
        자동 탐지되지 않는 IDE 가 있으면 ➕ 로 <code>*64.exe</code> 를 직접 등록하세요.
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
            <button class="remove" onclick={() => removeManualIde(ide)} title="등록 해제">✕</button>
          </li>
        {/each}
      </ul>
    {/if}

    <h2>데이터</h2>
    <p class="muted small">저장 위치</p>
    <p class="path-box">{dataDir || "(확인 불가)"}</p>
    <button onclick={scan} disabled={loading}>{loading ? "스캔 중…" : "IDE 재스캔"}</button>
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
      style="padding-left:{depth * 1.1}rem"
    >
      <button class="twist" onclick={() => toggle(folder.id)}>
        {isCollapsed(folder.id) ? "▸" : "▾"}
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
          📁 {folder.name}
        </span>
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
  <li
    class="proj-card"
    draggable="true"
    ondragstart={(e) => startDrag(e, p.id)}
    ondblclick={() => openProject(p)}
    style="margin-left:{depth * 1.1}rem"
  >
    <div class="info" class:missing={missing.has(p.id)}>
      <div class="name" title={missing.has(p.id) ? "디렉토리 없음" : p.name}>
        {p.name}
      </div>
      <div class="path">{p.path}</div>
    </div>
    <select bind:value={pickedIde[p.id]} title="열 IDE 선택">
      {#each ides as ide (ide.id)}
        <option value={ide.id} selected={ideFor(p) === ide.id}>
          {ide.productCode}
        </option>
      {/each}
    </select>
    <button class="launch" onclick={() => openProject(p)} disabled={opening === p.id}>
      {opening === p.id ? "여는 중…" : "열기"}
    </button>
    <button class="remove" onclick={() => removeProject(p)} title="제거">✕</button>
  </li>
{/snippet}

<style>
  :root {
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    color: #0f0f0f;
    background-color: #f6f6f6;
  }

  .container {
    max-width: 640px;
    margin: 0 auto;
    padding: 0.75rem;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

  h1 {
    font-size: 1.4rem;
    margin: 0;
  }

  h2 {
    font-size: 1rem;
    margin: 1rem 0 0.5rem;
    color: #555;
  }

  .tabs {
    display: flex;
    gap: 0.3rem;
    margin-bottom: 0.5rem;
  }
  .tabs button {
    flex: 1;
    border-radius: 6px;
    box-shadow: none;
    background: transparent;
    border: 1px solid #ddd;
  }
  .tabs button.active {
    background: #396cd8;
    color: #fff;
    border-color: #396cd8;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
  }
  .section-head .search {
    flex: 1;
    margin: 0;
  }

  .head-btns {
    display: flex;
    gap: 0.3rem;
  }
  .head-btns .icon {
    flex: none;
    padding: 0.35em 0.5em;
    font-size: 1rem;
    line-height: 1;
  }

  .folder-add {
    display: flex;
    gap: 0.4rem;
    margin: 0.25rem 0 0.75rem;
  }
  .folder-add input {
    flex: 1;
    padding: 0.35em 0.5em;
    border-radius: 6px;
    border: 1px solid #ccc;
  }

  .search {
    display: flex;
    gap: 0.3rem;
    align-items: center;
    margin: 0.25rem 0 0.5rem;
  }
  .search input {
    flex: 1;
    padding: 0.35em 0.5em;
    border-radius: 6px;
    border: 1px solid #ccc;
    font-size: 1rem;
    line-height: 1;
    box-sizing: border-box;
  }
  .head-btns .icon {
    box-sizing: border-box;
  }


  .ide-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .tree,
  .children {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .children {
    margin-top: 0.4rem;
  }

  .folder-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.4rem;
    border-radius: 6px;
  }
  .folder-head.drop {
    outline: 2px dashed #396cd8;
    background: rgba(57, 108, 216, 0.08);
  }
  .twist {
    flex: none;
    padding: 0 0.3em;
    background: transparent;
    box-shadow: none;
    border: none;
  }
  .folder-name {
    flex: 1;
    font-weight: 600;
    cursor: default;
  }
  .rename {
    flex: 1;
    padding: 0.2em 0.4em;
    border-radius: 4px;
    border: 1px solid #396cd8;
  }
  .folder-actions {
    display: flex;
    gap: 0.2rem;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .folder-head:hover .folder-actions {
    opacity: 1;
  }
  .folder-actions button {
    padding: 0.15em 0.45em;
    box-shadow: none;
  }

  .unfiled.drop {
    outline: 2px dashed #396cd8;
    border-radius: 6px;
  }

  .ide-card,
  .proj-card {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem;
    background: #fff;
    border-radius: 8px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
  }

  .proj-card {
    cursor: grab;
  }

  .ide-icon {
    flex: none;
    width: 2.4rem;
    height: 2.4rem;
    object-fit: contain;
    margin-right: 0.5rem;
  }

  .badge {
    flex: none;
    width: 2.4rem;
    height: 2.4rem;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 0.85rem;
    color: #fff;
    background: #396cd8;
    border-radius: 6px;
  }

  .info {
    flex: 1;
    min-width: 0;
  }
  .info.missing .name,
  .info.missing .path {
    color: #aaa;
  }

  .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .version {
    font-size: 0.8rem;
    color: #666;
  }

  .path {
    font-size: 0.75rem;
    color: #888;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button {
    border-radius: 8px;
    border: 1px solid transparent;
    padding: 0.4em 1em;
    font-size: 0.9em;
    font-weight: 500;
    font-family: inherit;
    color: #0f0f0f;
    background-color: #fff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    border-color: #396cd8;
  }

  button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .launch {
    flex: none;
    padding: 0.4em 0.6em;
  }

  .remove {
    flex: none;
    padding: 0.4em 0.6em;
    color: #c0392b;
  }

  select {
    flex: none;
    padding: 0.3em;
    border-radius: 6px;
  }

  .error {
    color: #c0392b;
    background: #fdecea;
    padding: 0.5rem 0.75rem;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .error .dismiss {
    flex: none;
    padding: 0.1em 0.45em;
    color: #c0392b;
    background: transparent;
    box-shadow: none;
  }

  .muted {
    color: #888;
  }
  .small {
    font-size: 0.75rem;
    margin-bottom: 0.2rem;
  }

  .path-box {
    font-size: 0.78rem;
    font-family: ui-monospace, monospace;
    background: #fff;
    border: 1px solid #ddd;
    border-radius: 6px;
    padding: 0.4rem 0.5rem;
    word-break: break-all;
    margin: 0 0 0.75rem;
  }

  @media (prefers-color-scheme: dark) {
    :root {
      color: #f6f6f6;
      background-color: #2f2f2f;
    }
    h2 {
      color: #bbb;
    }
    .ide-card,
    .proj-card {
      background: #1f1f1f;
    }
    .version {
      color: #aaa;
    }
    .path {
      color: #999;
    }
    button {
      color: #fff;
      background-color: #0f0f0f98;
    }
    .twist,
    .folder-actions button {
      background: transparent;
    }
    .folder-add input,
    .search input,
    select {
      color: #fff;
      background-color: #1f1f1f;
      border-color: #444;
    }
    .path-box {
      background: #1f1f1f;
      border-color: #444;
    }
    .tabs button {
      color: #fff;
      border-color: #444;
    }
    .tabs button.active {
      color: #fff;
      border-color: #396cd8;
    }
  }
</style>
