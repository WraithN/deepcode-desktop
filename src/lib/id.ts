/**
 * 生成短唯一ID（8位十六进制）
 */
export function generateShortId(): string {
  return Math.random().toString(36).substring(2, 10) + Math.random().toString(36).substring(2, 6);
}

/**
 * 生成带时间戳的唯一ID（用于消息、请求等）
 */
export function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

/**
 * 生成请求ID（用于 WebSocket JSON-RPC 请求）
 */
export function generateRequestId(): string {
  return `req-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

/**
 * 获取ID前n位显示
 */
export function formatIdShort(id: string, length: number = 4): string {
  return id.slice(0, length).toUpperCase();
}
