import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const getPlatformInfo = () => invoke("get_platform_info");
export const getEngineStatus = () => invoke("get_engine_status");
export const ensureStandardEngine = (model) =>
  invoke("ensure_standard_engine", { model });
export const ensureCompatEngine = () => invoke("ensure_compat_engine");
export const collectAudio = (paths) => invoke("collect_audio", { paths });
export const transcribeBatch = (req) => invoke("transcribe_batch", { req });
export const saveAs = (path, content) => invoke("save_as", { path, content });
export const readTextFile = (path) => invoke("read_text_file", { path });
export const copyFile = (src, dst) => invoke("copy_file", { src, dst });
export const openPath = (path) => invoke("open_path", { path });

/** 监听后端事件，返回取消函数 */
export function onEvent(event, cb) {
  let unlisten = null;
  listen(event, (e) => cb(e.payload)).then((u) => {
    unlisten = u;
  });
  return () => {
    if (unlisten) unlisten();
  };
}
