<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { openPath, readTextFile, copyFile } from "../api";

  let { results } = $props();

  let preview = $state({ path: "", text: "", mode: "md" as "md" | "prompt" });
  let previewing = $state(false);

  function stem(path: string): string {
    const n = path.split("/").pop() || "out";
    return n.replace(/\.[^.]+$/, "");
  }

  function stripMarkdownTags(text: string): string {
    // 移除 HTML 标签和 Markdown 标题符号，保留纯文本
    return text
      .replace(/<[^>]*>/g, "")
      .replace(/^#+\s*/gm, "")
      .replace(/^\|.*\|$/gm, "")       // 表格行
      .replace(/^\|-+\|$/gm, "")        // 表格分隔
      .replace(/^---\s*$/gm, "")        // hr
      .replace(/\n{3,}/g, "\n\n")
      .trim();
  }

  async function doOpen(dir?: string) {
    if (dir) await openPath(dir);
  }

  async function doPreview(r: any) {
    if (!r.output_dir) return;
    const src = `${r.output_dir}/${stem(r.path)}.md`;
    try {
      const raw = await readTextFile(src);
      preview = { path: r.path, text: stripMarkdownTags(raw), mode: "md" };
      previewing = true;
    } catch (e) {
      preview = { path: r.path, text: "无法读取预览：" + String(e), mode: "md" };
      previewing = true;
    }
  }

  async function doPreviewPrompt(r: any) {
    if (!r.output_dir) return;
    const src = `${r.output_dir}/${stem(r.path)}-提词稿.html`;
    try {
      const raw = await readTextFile(src);
      preview = { path: r.path, text: raw, mode: "prompt" };
      previewing = true;
    } catch (e) {
      preview = { path: r.path, text: "未找到提词稿文件（请先在设置中勾选「提词稿」格式）", mode: "prompt" };
      previewing = true;
    }
  }

  async function doSaveAs(r: any, ext: string) {
    if (!r.output_dir) return;
    const base = stem(r.path);
    const src = `${r.output_dir}/${base}.${ext}`;
    const target = await save({ defaultPath: `${base}.${ext}` });
    if (target) {
      try {
        await copyFile(src, target);
      } catch (e) {
        alert("保存失败：" + String(e));
      }
    }
  }
</script>

<section class="results">
  <h3>转写结果</h3>
  <ul class="list">
    {#each results as r (r.path)}
      <li class="item">
        <div class="meta">
          <span class="name" title={r.path}>{stem(r.path)}</span>
          <span class="badge {r.ok ? 'ok' : 'err'}">
            {r.ok ? '完成' : '失败'}
            {#if r.ok}
              <span class="seg-count">({r.segments_count ?? 0}段)</span>
            {/if}
          </span>
        </div>
        {#if r.error}
          <p class="err">{r.error}</p>
        {/if}
        {#if r.ok && !r.segments_count}
          <p class="warn-text">⚠️ 未检测到语音内容，可能为空白/噪音文件</p>
        {/if}
        <div class="acts">
          <button onclick={() => doOpen(r.output_dir)}>打开文件夹</button>
          <button onclick={() => doPreview(r)}>预览</button>
          <button onclick={() => doPreviewPrompt(r)}>预览提词稿</button>
          <button onclick={() => doSaveAs(r, "md")}>另存 MD</button>
          <button onclick={() => doSaveAs(r, "srt")}>另存 SRT</button>
          <button onclick={() => doSaveAs(r, "rtf")}>另存 RTF</button>
        </div>
      </li>
    {/each}
  </ul>

  {#if previewing}
    <div class="preview">
      <div class="preview-head">
        <span>预览：{stem(preview.path)}{preview.mode === "prompt" ? "-提词稿.html" : ".md"}</span>
        <button onclick={() => (previewing = false)}>收起</button>
      </div>
      {#if preview.mode === "prompt"}
        <iframe class="prompt-frame" title="提词稿预览" srcdoc={preview.text}></iframe>
      {:else}
        <pre>{preview.text}</pre>
      {/if}
    </div>
  {/if}
</section>

<style>
  .results {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    padding: 14px 16px;
  }
  h3 {
    margin: 0 0 10px;
    font-size: 15px;
    font-weight: 600;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .item {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px 10px;
  }
  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .name {
    font-size: 13px;
    font-weight: 500;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
  }
  .badge.ok {
    background: #dcfce7;
    color: var(--success);
  }
  .badge.err {
    background: #fee2e2;
    color: var(--danger);
  }
  .err {
    margin: 6px 0 0;
    color: var(--danger);
    font-size: 12px;
  }
  .warn-text {
    margin: 4px 0 0;
    color: #d97706;
    font-size: 11px;
  }
  .seg-count {
    font-weight: normal;
    opacity: 0.7;
    font-size: 10px;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .acts button {
    padding: 5px 10px;
    font-size: 12px;
  }
  .preview {
    margin-top: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }
  .preview-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 10px;
    background: #f8fafc;
    font-size: 12px;
    color: var(--text-soft);
  }
  .preview-head button {
    padding: 2px 8px;
    font-size: 12px;
  }
  pre {
    margin: 0;
    padding: 20px 24px;
    max-height: 480px;
    overflow: auto;
    font-size: clamp(14px, 2vw, 22px);
    line-height: 1.9;
    white-space: pre-wrap;
    word-break: break-word;
    text-align: center;
    font-weight: 450;
    color: #1e293b;
    background: linear-gradient(180deg, #fafbfc 0%, #ffffff 100%);
  }
  .prompt-frame {
    width: 100%;
    height: 520px;
    border: none;
    display: block;
    background: #0f172a;
  }
</style>
