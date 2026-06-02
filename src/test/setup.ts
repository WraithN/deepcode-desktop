import '@testing-library/jest-dom';

// Mock Tauri APIs
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  value: {},
  writable: true,
});
