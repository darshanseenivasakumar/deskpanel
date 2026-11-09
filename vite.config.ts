import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

const root = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: 'chrome120',
    rollupOptions: {
      input: { main: root('./index.html'), pill: root('./pill.html') },
    },
  },
});
