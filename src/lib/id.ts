/**
 * 生成短唯一ID（8位十六进制）
 */
export function generateShortId(): string {
  return Math.random().toString(36).substring(2, 10) + Math.random().toString(36).substring(2, 6);
}

/**
 * 获取ID前n位显示
 */
export function formatIdShort(id: string, length: number = 4): string {
  return id.slice(0, length).toUpperCase();
}
