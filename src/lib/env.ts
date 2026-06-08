/**
 * 检测当前是否运行在 Tauri 桌面环境中
 */
export function isTauri(): boolean {
  return !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
    || !!(window as unknown as Record<string, unknown>).__TAURI__;
}
