import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri core APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

// Provide minimal __TAURI_INTERNALS__ for compatibility
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  value: {},
  writable: true,
});

// Mock localStorage for jsdom
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] || null,
    setItem: (key: string, value: string) => { store[key] = String(value); },
    removeItem: (key: string) => { delete store[key]; },
    clear: () => { store = {}; },
    length: 0,
    key: (_index: number) => null,
  };
})();
Object.defineProperty(window, 'localStorage', { value: localStorageMock });

// Mock WebSocket for store tests
class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  onopen: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;

  private url: string;
  private messages: string[] = [];

  constructor(url: string | URL) {
    this.url = String(url);
    // Simulate async open
    queueMicrotask(() => {
      this.readyState = MockWebSocket.OPEN;
      if (this.onopen) {
        this.onopen(new Event('open'));
      }
    });
  }

  send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
    if (typeof data === 'string') {
      this.messages.push(data);
    }
  }

  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    if (this.onclose) {
      this.onclose(new CloseEvent('close'));
    }
  }

  /** Simulate receiving a message from the server */
  simulateMessage(data: unknown): void {
    if (this.onmessage) {
      this.onmessage(new MessageEvent('message', { data: JSON.stringify(data) }));
    }
  }
}

Object.defineProperty(window, 'MockWebSocket', {
  value: MockWebSocket,
  writable: true,
});

// Replace global WebSocket with mock
Object.defineProperty(window, 'WebSocket', {
  value: MockWebSocket,
  writable: true,
});
