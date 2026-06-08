/**
 * localStorage 持久化状态的版本管理和迁移机制
 *
 * 用法：
 *   const mgr = createStorageManager('my_key', 2, { default: {} });
 *   mgr.load(); // 自动执行迁移
 *   mgr.save(data);
 */

interface Migration<T> {
  fromVersion: number;
  migrate: (data: unknown) => T;
}

interface StorageManagerOptions<T> {
  defaultValue: T;
  migrations?: Migration<T>[];
}

const VERSION_PREFIX = '__v_';

export function createStorageManager<T>(
  key: string,
  currentVersion: number,
  options: StorageManagerOptions<T>,
) {
  const versionKey = `${VERSION_PREFIX}${key}`;

  function getStoredVersion(): number {
    try {
      const raw = localStorage.getItem(versionKey);
      return raw ? parseInt(raw, 10) : 0;
    } catch {
      return 0;
    }
  }

  function setStoredVersion(version: number): void {
    try {
      localStorage.setItem(versionKey, String(version));
    } catch {
      // Ignore localStorage errors
    }
  }

  function load(): T {
    const storedVersion = getStoredVersion();

    if (storedVersion === 0) {
      // 从未存储过，初始化
      setStoredVersion(currentVersion);
      return options.defaultValue;
    }

    if (storedVersion === currentVersion) {
      // 版本匹配，直接读取
      try {
        const raw = localStorage.getItem(key);
        if (raw) {
          return JSON.parse(raw) as T;
        }
      } catch {
        // Parse error, fall through to default
      }
      return options.defaultValue;
    }

    // 需要迁移
    if (storedVersion < currentVersion && options.migrations) {
      try {
        const raw = localStorage.getItem(key);
        let data: unknown = raw ? JSON.parse(raw) : options.defaultValue;

        for (const migration of options.migrations) {
          if (migration.fromVersion === storedVersion) {
            data = migration.migrate(data);
            break;
          }
        }

        setStoredVersion(currentVersion);
        localStorage.setItem(key, JSON.stringify(data));
        return data as T;
      } catch {
        // Migration failed, reset to default
      }
    }

    // 版本不兼容或迁移失败，重置
    localStorage.removeItem(key);
    setStoredVersion(currentVersion);
    return options.defaultValue;
  }

  function save(data: T): void {
    try {
      localStorage.setItem(key, JSON.stringify(data));
      setStoredVersion(currentVersion);
    } catch {
      // Ignore localStorage errors (e.g. quota exceeded)
    }
  }

  function clear(): void {
    localStorage.removeItem(key);
    localStorage.removeItem(versionKey);
  }

  return { load, save, clear };
}
