import { describe, it, expect, beforeEach } from 'vitest';
import { createStorageManager } from './storage-version';

describe('storage-version', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('should return default value when no data exists', () => {
    const mgr = createStorageManager('test_key', 1, { defaultValue: { count: 0 } });
    const data = mgr.load();
    expect(data).toEqual({ count: 0 });
  });

  it('should save and load data with version', () => {
    const mgr = createStorageManager('test_key', 1, { defaultValue: { count: 0 } });
    mgr.save({ count: 42 });
    const data = mgr.load();
    expect(data).toEqual({ count: 42 });
  });

  it('should detect version mismatch and reset to default', () => {
    const mgrV1 = createStorageManager('test_key', 1, { defaultValue: { version: 1 } });
    mgrV1.save({ version: 1 });

    const mgrV2 = createStorageManager('test_key', 2, { defaultValue: { version: 2 } });
    const data = mgrV2.load();
    expect(data).toEqual({ version: 2 });
  });

  it('should run migration when version is lower', () => {
    const mgrV1 = createStorageManager('test_key', 1, { defaultValue: { name: '' } });
    mgrV1.save({ name: 'Alice' });

    const mgrV2 = createStorageManager('test_key', 2, {
      defaultValue: { name: '', age: 0 },
      migrations: [
        {
          fromVersion: 1,
          migrate: (data) => ({ ...(data as Record<string, unknown>), age: 18 }),
        },
      ],
    });

    const data = mgrV2.load();
    expect(data).toEqual({ name: 'Alice', age: 18 });
  });

  it('should clear data and version', () => {
    const mgr = createStorageManager('test_key', 1, { defaultValue: { count: 0 } });
    mgr.save({ count: 10 });
    mgr.clear();

    const data = mgr.load();
    expect(data).toEqual({ count: 0 });
  });
});
