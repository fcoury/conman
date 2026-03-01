import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';
import { defineConfig } from 'vite';

const DEDUPE_PACKAGES = [
  'react',
  'react-dom',
  '@codemirror/state',
  '@codemirror/view',
  '@codemirror/language',
  '@codemirror/autocomplete',
  '@codemirror/theme-one-dark',
  '@codemirror/lang-javascript',
  '@codemirror/lang-json',
  '@codemirror/lang-markdown',
  '@codemirror/lang-css',
  '@codemirror/lang-yaml',
  '@codemirror/legacy-modes',
];

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: false,
      },
    },
    allowedHosts: ['m3pro', 'dxflow-app.localhost', '.dxflow-app.localhost'],
    fs: { allow: ['..'] },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './gistia-design-system/src'),
      'gistia-design-system': path.resolve(
        __dirname,
        './gistia-design-system/src/lib/index.ts',
      ),
      '~': path.resolve(__dirname, './src'),
    },
    dedupe: DEDUPE_PACKAGES,
  },
  optimizeDeps: {
    exclude: ['gistia-design-system'],
  },
});
