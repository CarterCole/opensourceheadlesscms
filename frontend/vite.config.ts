import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import relay from 'vite-plugin-relay'

export default defineConfig({
  plugins: [relay, react()],
  server: {
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
})
