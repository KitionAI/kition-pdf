import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The kit package is served from wherever a host puts it, so every asset path is relative.
export default defineConfig({
  base: './',
  plugins: [react()],
  resolve: {
    // One React for the page, also when KITION_LINK=1 links the kition-sdk and Tessera checkouts.
    dedupe: ['react', 'react-dom'],
  },
  optimizeDeps: {
    exclude: ['@kition/sdk'],
    // Tessera's UI library reaches a CommonJS shim (use-sync-external-store, through radix-ui);
    // prebundled here, it is converted for the browser and the excluded @kition/sdk shares it.
    include: ['@kition/tessera/ui'],
  },
  worker: { format: 'es' },
  server: {
    port: 5183,
    // The repository (dev.html) and the sibling kition-sdk and kition-tessera checkouts that a
    // KITION_LINK=1 install serves from.
    fs: { allow: [fileURLToPath(new URL('..', import.meta.url)), fileURLToPath(new URL('../../kition-sdk', import.meta.url)), fileURLToPath(new URL('../../kition-tessera', import.meta.url))] },
  },
  build: {
    outDir: 'dist',
    rollupOptions: { input: { index: fileURLToPath(new URL('index.html', import.meta.url)), dev: fileURLToPath(new URL('dev.html', import.meta.url)) } },
  },
})
