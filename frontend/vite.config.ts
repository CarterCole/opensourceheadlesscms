import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import relay from 'vite-plugin-relay'

export default defineConfig(({ command }) => ({
  // The built bundle is served by cms-server under /admin/; the dev server stays at /.
  base: command === 'build' ? '/admin/' : '/',
  plugins: [relay, react()],
  server: {
    // Listen on all interfaces so the dev server is reachable over the tailnet
    // (e.g. http://<host>.<tailnet>.ts.net:5173), and accept those hostnames.
    host: true,
    allowedHosts: ['.ts.net'],
    // changeOrigin stays false: the API only honours the session cookie when the request's
    // Origin matches its Host (CSRF guard), and the browser's Origin is this dev server.
    // The string shorthand would set changeOrigin: true and every write would 401.
    proxy: {
      '/api': { target: 'http://localhost:8080', changeOrigin: false },
      '/graphql/ws': { target: 'ws://localhost:8080', ws: true, changeOrigin: false },
      '/graphql': { target: 'http://localhost:8080', changeOrigin: false },
      '/health': { target: 'http://localhost:8080', changeOrigin: false },
    },
  },
  build: {
    outDir: 'dist',
  },
}))
