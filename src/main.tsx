import { createRoot } from "react-dom/client";
import "./index.css";
import App from "./App.tsx";
import { ErrorBoundary } from "./components/common/ErrorBoundary.tsx";
import { AppWrapper } from "./components/common/PageMeta.tsx";

console.log("[main.tsx] Starting application...");
console.log("[main.tsx] UserAgent:", navigator.userAgent);
console.log("[main.tsx] Platform:", navigator.platform);

try {
  const rootElement = document.getElementById("root");
  console.log("[main.tsx] Root element:", rootElement ? "found" : "NOT FOUND");
  
  if (rootElement) {
    console.log("[main.tsx] Creating React root...");
    const root = createRoot(rootElement);
    console.log("[main.tsx] React root created, rendering App...");
    root.render(
      <ErrorBoundary>
        <AppWrapper>
          <App />
        </AppWrapper>
      </ErrorBoundary>
    );
    console.log("[main.tsx] App rendered successfully");
  } else {
    console.error("[main.tsx] CRITICAL: Root element not found!");
    document.body.innerHTML = '<div style="padding: 20px; color: red;">Error: Root element not found</div>';
  }
} catch (error) {
  console.error("[main.tsx] CRITICAL ERROR during startup:", error);
  document.body.innerHTML = `<div style="padding: 20px; color: red;">Startup Error: ${error}</div>`;
}
