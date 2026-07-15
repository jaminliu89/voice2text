<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";

  let { dragging = false, onpaths } = $props();

  const AUDIO_EXT = [
    "mp3", "wav", "m4a", "flac", "ogg", "aac", "opus",
    "wma", "aiff", "mp4", "mov", "mkv", "webm", "amr",
  ];

  async function pick() {
    const selected = await open({
      multiple: true,
      filters: [{ name: "音频 / 视频", extensions: AUDIO_EXT }],
    });
    if (selected && Array.isArray(selected)) {
      onpaths(selected);
    }
  }
</script>

<section class="dropzone" class:dragging>
  <div class="inner">
    <div class="icon">🎙️</div>
    <h2>拖入音频或文件夹</h2>
    <p>支持批量导入，也支持把整个文件夹拖进来递归识别</p>
    <button class="primary" onclick={pick}>选择文件</button>
  </div>
</section>

<style>
  .dropzone {
    border: 2px dashed var(--border);
    border-radius: var(--radius);
    background: linear-gradient(135deg, #ffffff 0%, #f3f6ff 100%);
    padding: 36px 20px;
    text-align: center;
    transition: all 0.2s ease;
  }
  .dropzone.dragging {
    border-color: var(--primary-2);
    box-shadow: 0 0 0 4px rgba(59, 130, 246, 0.15);
    transform: scale(1.01);
  }
  .inner {
    pointer-events: none;
  }
  .inner button {
    pointer-events: auto;
    margin-top: 10px;
  }
  .icon {
    font-size: 40px;
    margin-bottom: 6px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }
  p {
    margin: 6px 0 0;
    color: var(--text-soft);
    font-size: 13px;
  }
</style>
