<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { invoke } from "@tauri-apps/api/core";

  import DropZone from "./lib/components/DropZone.svelte";
  import SettingsPanel from "./lib/components/SettingsPanel.svelte";
  import FileList from "./lib/components/FileList.svelte";
  import ResultView from "./lib/components/ResultView.svelte";
  import * as api from "./lib/api";

  let platform = $state<any>(null);
  let engineStatus = $state<any>({ standard: { available: false }, compat: { available: false } });
  let dragging = $state(false);
  let files = $state<any[]>([]);
  let settings = $state<any>({
    engine: "standard",
    model: "base",
    language: "zh",
    formats: ["md", "srt"],
    parallel: 4,
  });
  let deploying = $state(false);
  let deployStage = $state("");
  let deployPercent = $state(0);
  let deployMsg = $state("");
  let transcribing = $state(false);
  let overallProgress = $state(0);
  let results = $state<any[]>([]);

  let unlisteners: Array<() => void> = [];

  onMount(async () => {
    try {
      platform = await api.getPlatformInfo();
      settings.model = platform.recommended_model;
      settings.parallel = Math.min(platform.physical_cores || 4, 6);
    } catch (e) {
      console.error(e);
    }
    try {
      engineStatus = await api.getEngineStatus();
      // 首次启动：没有任何可用引擎时，自动部署
      if (!engineStatus.standard?.available && !engineStatus.compat?.available) {
        // 标准引擎二进制已存在（bundled）但缺模型 → 优先完善标准引擎
        if (engineStatus.standard?.path) {
          settings.engine = "standard";
          deployMsg = "引擎已就绪，正在下载模型文件…";
        } else {
          settings.engine = "compat";
          deployMsg = "检测到未安装任何转写引擎，正在自动部署兼容引擎…";
        }
        deploy();
      }
    } catch (e) {
      console.error(e);
    }

    let unDrag: any;
    try {
      const wv = getCurrentWebviewWindow();
      unDrag = await wv.onDragDropEvent((event: any) => {
        try {
          const t = event.payload.type;
          invoke("debug_log", { message: `drag-drop: ${t} paths=${JSON.stringify(event.payload.paths || [])}` }).catch(() => {});
          if (t === "over") dragging = true;
          else if (t === "leave") dragging = false;
          else if (t === "drop") {
            dragging = false;
            handlePaths(event.payload.paths);
          }
        } catch (innerErr) {
          invoke("debug_log", { message: `drag-drop handler error: ${String(innerErr)}` }).catch(() => {});
        }
      });
    } catch (e) {
      invoke("debug_log", { message: `drag-drop init error: ${String(e)}` }).catch(() => {});
    }

    unlisteners.push(
      api.onEvent("deploy-progress", (p: any) => {
        deployStage = p.stage;
        deployPercent = p.percent;
        if (p.message) deployMsg = p.message;
      })
    );
    unlisteners.push(
      api.onEvent("deploy-progress-dl", (p: any) => {
        if (p.total > 0) deployMsg = `下载中 ${p.percent}%`;
      })
    );
    unlisteners.push(
      api.onEvent("transcribe-progress", (p: any) => handleProgress(p))
    );

    return () => {
      try { unDrag?.(); } catch (_) {}
      unlisteners.forEach((u) => { try { u(); } catch (_) {} });
    };
  });

  async function handlePaths(rawPaths: string[]) {
    try {
      const entries = await api.collectAudio(rawPaths);
      const existing = new Set(files.map((f) => f.path));
      const added = entries
        .filter((e: any) => !existing.has(e.path))
        .map((e: any) => ({
          path: e.path,
          name: e.path.split("/").pop(),
          output_base: e.output_base,
          status: "queued",
          progress: 0,
          error: null,
        }));
      files = [...files, ...added];
    } catch (e) {
      alert("收集文件失败：" + String(e));
    }
  }

  function handleProgress(p: any) {
    if (p.status === "all-done") {
      transcribing = false;
      overallProgress = 100;
      return;
    }
    const idx = files.findIndex((f) => f.path === p.file);
    if (idx >= 0) {
      files[idx] = {
        ...files[idx],
        progress: p.percent,
        status: p.status === "transcribing" ? "transcribing" : p.status,
      };
    }
    const done = files.filter((f) => f.status === "done" || f.status === "error").length;
    overallProgress = files.length ? Math.round((done / files.length) * 100) : 0;
  }

  function removeFile(i: number) {
    files = files.filter((_, k) => k !== i);
  }

  function clearAll() {
    files = [];
    results = [];
    overallProgress = 0;
  }

  async function deploy() {
    deploying = true;
    deployPercent = 0;
    deployMsg = "准备中…";
    try {
      if (settings.engine === "compat") {
        await api.ensureCompatEngine();
      } else {
        await api.ensureStandardEngine(settings.model);
      }
      engineStatus = await api.getEngineStatus();
    } catch (e) {
      deployMsg = "部署失败：" + String(e);
      alert("部署失败：" + String(e));
    } finally {
      deploying = false;
    }
  }

  async function start() {
    if (!files.length) return;
    if (settings.engine === "standard" && !engineStatus.standard.available) {
      alert("标准引擎未就绪，请先点击「安装本地引擎」。");
      return;
    }
    if (settings.engine === "compat" && !engineStatus.compat.available) {
      alert("未检测到兼容引擎，请先安装兼容引擎或改用标准引擎。");
      return;
    }
    transcribing = true;
    overallProgress = 0;
    files = files.map((f) => ({ ...f, status: "queued", progress: 0, error: null }));
    const req = {
      items: files.map((f) => ({ path: f.path, output_base: f.output_base })),
      options: {
        engine: settings.engine,
        model: settings.model,
        language: settings.language,
        output_formats: settings.formats,
        parallel: Number(settings.parallel) || 1,
      },
    };
    try {
      const res = await api.transcribeBatch(req);
      results = res.results;
      overallProgress = 100;
    } catch (e) {
      alert("转写失败：" + String(e));
    } finally {
      transcribing = false;
    }
  }

  let engineReady = $derived(
    settings.engine === "standard"
      ? engineStatus.standard.available
      : engineStatus.compat.available
  );
  let engineLabel = $derived(settings.engine === "compat" ? "兼容引擎" : "标准引擎");
  let deployLabel = $derived(
    settings.engine === "compat"
      ? engineStatus.compat.available
        ? "重装兼容引擎"
        : "安装兼容引擎"
      : engineStatus.standard.available
        ? "更新本地引擎"
        : "安装本地引擎"
  );
</script>

<main>
  <header>
    <div class="title">
      <span class="logo">🎧</span>
      <h1>小柳语音转写</h1>
    </div>
    <div class="header-right">
      <span class="status-dot" class:on={engineReady}></span>
      <span class="status-text">
        {engineLabel}{engineReady ? "已就绪" : "未安装"}
      </span>
      <button class="primary" onclick={deploy} disabled={deploying}>
        {deployLabel}
      </button>
    </div>
  </header>

  {#if deploying}
    <div class="deploy-bar">
      <div class="deploy-fill" style="width:{deployPercent}%"></div>
      <span class="deploy-label">{deployStage} {deployPercent}% · {deployMsg}</span>
    </div>
  {:else if transcribing}
    <div class="deploy-bar">
      <div class="deploy-fill" style="width:{overallProgress}%"></div>
      <span class="deploy-label">转写中 {overallProgress}%</span>
    </div>
  {/if}

  <div class="content">
    <DropZone {dragging} onpaths={handlePaths} />
    <SettingsPanel bind:settings {platform} />

    {#if engineStatus.compat}
      <p class="hint compat-status">
        {#if engineStatus.compat.available}
          兼容引擎 v{engineStatus.compat.version || "?"}
          {#if engineStatus.compat.ffmpeg_available}
            · ffmpeg ✓
          {:else}
            · ffmpeg ✗（部分格式无法处理）
          {/if}
          {#if engineStatus.compat.cached_models?.length}
            · 已缓存模型：{engineStatus.compat.cached_models.join(", ")}
          {/if}
          {#if engineStatus.compat.note}
            <br /><span class="warn">{engineStatus.compat.note}</span>
          {/if}
        {:else}
          兼容引擎：{engineStatus.compat.note || "未安装"}
        {/if}
      </p>
    {/if}

    <div class="actions">
      <button class="primary" onclick={start} disabled={!files.length || !engineReady || transcribing}>
        开始转写{transcribing ? "…" : ""}
      </button>
      <button onclick={clearAll} disabled={!files.length || transcribing}>清空</button>
      <span class="count">{files.length} 个文件</span>
    </div>

    <FileList {files} onremove={removeFile} />

    {#if results.length}
      <ResultView {results} />
    {/if}
  </div>
</main>

<style>
  main {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 18px;
    background: rgba(255, 255, 255, 0.8);
    backdrop-filter: blur(10px);
    border-bottom: 1px solid var(--border);
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .logo {
    font-size: 20px;
  }
  h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    background: linear-gradient(135deg, var(--primary), var(--primary-2));
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }
  .header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .status-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #cbd5e1;
  }
  .status-dot.on {
    background: var(--success);
    box-shadow: 0 0 0 3px rgba(22, 163, 74, 0.18);
  }
  .status-text {
    font-size: 12px;
    color: var(--text-soft);
  }
  .deploy-bar {
    position: relative;
    height: 22px;
    background: #eef0f3;
    overflow: hidden;
  }
  .deploy-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--primary), var(--primary-2));
    transition: width 0.25s ease;
  }
  .deploy-label {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--text);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 16px 18px 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .count {
    color: var(--text-soft);
    font-size: 12px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .hint {
    margin: 0;
    color: var(--text-soft);
    font-size: 12px;
  }
  .compat-status {
    background: #f0fdf4;
    border: 1px solid #bbf7d0;
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 11px;
    line-height: 1.5;
  }
  .warn {
    color: #d97706;
    font-weight: 500;
  }
</style>
