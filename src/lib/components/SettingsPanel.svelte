<script lang="ts">
  let { settings = $bindable(), platform } = $props();

  const MODELS = [
    { v: "tiny", t: "tiny（最快）" },
    { v: "base", t: "base（均衡）" },
    { v: "small", t: "small（更准）" },
    { v: "medium", t: "medium（最准·较大）" },
  ];
  const LANGS = [
    { v: "auto", t: "自动检测" },
    { v: "zh", t: "中文" },
    { v: "en", t: "英文" },
    { v: "ja", t: "日语" },
    { v: "ko", t: "韩语" },
  ];
  const FORMATS = [
    { v: "md", t: "MD 表格" },
    { v: "srt", t: "字幕 SRT" },
    { v: "vtt", t: "字幕 VTT" },
    { v: "txt", t: "纯文本" },
    { v: "prompt", t: "提词稿" },
    { v: "rtf", t: "RTF" },
  ];

  let maxParallel = $derived(Math.max(1, platform?.physical_cores || 4));

  function accelLabel(accel: string): string {
    const map: Record<string, string> = {
      CoreML: "CoreML + ANE",
      Mps: "MPS GPU",
      Metal: "Metal GPU",
      Cpu: "CPU",
    };
    return map[accel] || accel;
  }
</script>

<div class="panel">
  <div class="row">
    <label>引擎</label>
    <select bind:value={settings.engine}>
      <option value="standard">标准引擎（CoreML / Metal 加速）</option>
      <option value="compat">兼容引擎（本机 Python）</option>
    </select>
  </div>

  <div class="row">
    <label>模型</label>
    <select bind:value={settings.model}>
      {#each MODELS as m}
        <option value={m.v}>{m.t}</option>
      {/each}
    </select>
  </div>

  <div class="row">
    <label>语言</label>
    <select bind:value={settings.language}>
      {#each LANGS as l}
        <option value={l.v}>{l.t}</option>
      {/each}
    </select>
  </div>

  <div class="row">
    <label>导出格式</label>
    <div class="formats">
      {#each FORMATS as f}
        <label class="chip">
          <input type="checkbox" value={f.v} bind:group={settings.formats} />
          {f.t}
        </label>
      {/each}
    </div>
  </div>

  <div class="row">
    <label>并行数</label>
    <div class="slider">
      <input type="range" min="1" max={maxParallel} bind:value={settings.parallel} />
      <span class="val">{settings.parallel}</span>
    </div>
  </div>

  {#if platform}
    <p class="hint">
      检测到 {platform.apple_silicon ? "Apple 芯片" : "Intel Mac"} ·
      内存 {platform.memory_gb.toFixed(0)} GB · 物理核心 {platform.physical_cores} ·
      加速: {accelLabel(platform.acceleration)}
      {#if platform.mps_available} · MPS 就绪{/if}
    </p>
  {/if}
</div>

<style>
  .panel {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .row label {
    width: 72px;
    color: var(--text-soft);
    font-size: 13px;
    flex-shrink: 0;
  }
  .row select {
    flex: 1;
  }
  .formats {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    flex: 1;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: #f3f4f6;
    border-radius: 999px;
    padding: 4px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .chip input {
    accent-color: var(--primary);
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
  }
  .slider input {
    flex: 1;
    accent-color: var(--primary);
  }
  .val {
    width: 24px;
    text-align: right;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .hint {
    margin: 0;
    color: var(--text-soft);
    font-size: 12px;
  }
</style>
