<script lang="ts">
  let { files, onremove } = $props();

  const STATUS = {
    queued: { t: "等待", c: "queued" },
    transcribing: { t: "转写中", c: "run" },
    done: { t: "完成", c: "ok" },
    error: { t: "失败", c: "err" },
  };
</script>

{#if files.length === 0}
  <p class="empty">还没有文件，拖入音频或点击上方「选择文件」。</p>
{:else}
  <ul class="list">
    {#each files as f, i (f.path)}
      <li class="item">
        <div class="meta">
          <span class="name" title={f.path}>{f.name}</span>
          <span class="badge {STATUS[f.status]?.c}">{STATUS[f.status]?.t || f.status}</span>
        </div>
        <div class="bar">
          <div
            class="fill {f.status === 'error' ? 'err' : ''}"
            style="width:{f.progress}%"
          ></div>
        </div>
        {#if f.error}
          <p class="err-msg">{f.error}</p>
        {/if}
        <button class="rm" onclick={() => onremove(i)} title="移除">✕</button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .empty {
    color: var(--text-soft);
    font-size: 13px;
    padding: 8px 2px;
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
    position: relative;
    background: var(--surface);
    border-radius: 12px;
    box-shadow: var(--shadow);
    padding: 10px 36px 10px 12px;
  }
  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .name {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    flex-shrink: 0;
    background: #eef2ff;
    color: var(--primary);
  }
  .badge.ok {
    background: #dcfce7;
    color: var(--success);
  }
  .badge.run {
    background: #fef3c7;
    color: var(--warn);
  }
  .badge.err {
    background: #fee2e2;
    color: var(--danger);
  }
  .badge.queued {
    background: #f3f4f6;
    color: var(--text-soft);
  }
  .bar {
    margin-top: 8px;
    height: 5px;
    background: #eef0f3;
    border-radius: 999px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: linear-gradient(90deg, var(--primary), var(--primary-2));
    transition: width 0.25s ease;
  }
  .fill.err {
    background: var(--danger);
  }
  .err-msg {
    margin: 6px 0 0;
    color: var(--danger);
    font-size: 12px;
  }
  .rm {
    position: absolute;
    top: 8px;
    right: 8px;
    border: none;
    background: transparent;
    color: var(--text-soft);
    padding: 2px 6px;
    font-size: 12px;
  }
  .rm:hover {
    color: var(--danger);
    box-shadow: none;
  }
</style>
