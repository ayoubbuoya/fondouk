import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

/** Where `cargo run` serves the backend (BIND_ADDR default). */
const BACKEND = 'http://127.0.0.1:8080'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      // Every backend call goes through `/api` (HANDOFF D47). The browser then
      // sees one origin (localhost:5173), so the HttpOnly session cookie works
      // without CORS (D37). The prefix is removed: `/api/villages` -> `/villages`.
      // `ws: true` also forwards the `/api/ws` WebSocket upgrade.
      '/api': {
        target: BACKEND,
        ws: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
  test: {
    environment: 'node',
  },
})
