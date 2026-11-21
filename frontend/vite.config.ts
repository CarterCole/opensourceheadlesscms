import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import relay from 'vite-plugin-relay'

export default defineConfig({
  plugins: [relay, react()],
  server: {
    proxy: {
      '/api': 'http://localhost:8080',
      '/graphql/ws': { target: 'ws://localhost:8080', ws: true },
      '/graphql': 'http://localhost:8080',
      '/health': 'http://localhost:8080',
    },
  },
  build: {
    outDir: 'dist',
  },
})
