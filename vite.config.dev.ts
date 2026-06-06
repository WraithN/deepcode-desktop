import * as vite from 'vite';
import { defineConfig, loadConfigFromFile } from "vite";
import type { ConfigEnv } from "vite";
import path from "path";

export default defineConfig(async () => {
  const env: ConfigEnv = { command: "serve", mode: "development" };
  const configFile = path.resolve(__dirname, "vite.config.ts");
  const result = await loadConfigFromFile(env, configFile);
  const userConfig = result?.config;

  const viteVersionInfo = {
    version: vite.version,
    rollupVersion: (vite as any).rollupVersion ?? null,
    rolldownVersion: (vite as any).rolldownVersion ?? null,
    isRolldownVite: 'rolldownVersion' in vite
  };

  return {
    ...userConfig,
    define: {
      __VITE_INFO__: JSON.stringify(viteVersionInfo),
      ...(userConfig?.define || {})
    },
    cacheDir: path.resolve(__dirname, "node_modules/.vite"),
    server: {
      ...(userConfig?.server || {}),
      warmup: { clientFiles: ["./src/main.tsx"] }
    },
    plugins: [
      ...(userConfig?.plugins || []),
      {
        name: 'hmr-toggle',
        configureServer(server) {
          let hmrEnabled = true;
          const _send = server.ws.send;
          server.ws.send = (payload) => {
            if (hmrEnabled) {
              return _send.call(server.ws, payload);
            } else {
              console.log('[HMR disabled] skipped payload:', payload.type);
            }
          };
          server.middlewares.use('/innerapi/v1/sourcecode/__hmr_off', (req, res) => {
            hmrEnabled = false;
            res.setHeader('Content-Type', 'application/json');
            res.end(JSON.stringify({ status: 0, msg: 'HMR disabled' }));
          });
          server.middlewares.use('/innerapi/v1/sourcecode/__hmr_on', (req, res) => {
            hmrEnabled = true;
            res.setHeader('Content-Type', 'application/json');
            res.end(JSON.stringify({ status: 0, msg: 'HMR enabled' }));
          });
          server.middlewares.use('/innerapi/v1/sourcecode/__hmr_reload', (req, res) => {
            if (hmrEnabled) {
              server.ws.send({ type: 'full-reload', path: '*' });
            }
            res.statusCode = 200;
            res.setHeader('Content-Type', 'application/json');
            res.end(JSON.stringify({ status: 0, msg: 'Manual full reload triggered' }));
          });
        },
        load(id) {
          if (id === 'virtual:after-update') {
            return `
              if (import.meta.hot) {
                import.meta.hot.on('vite:afterUpdate', () => {
                  window.postMessage({ type: 'editor-update' }, '*');
                });
              }
            `;
          }
        },
        transformIndexHtml(html) {
          return {
            html,
            tags: [
              {
                tag: 'script',
                attrs: { type: 'module', src: '/@id/virtual:after-update' },
                injectTo: 'body'
              }
            ]
          };
        }
      }
    ]
  };
});
