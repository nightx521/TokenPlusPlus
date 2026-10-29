import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({ plugins: [react()], clearScreen: false, server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**', '**/release/**', '**/artifacts/**', '**/native-glass/target/**', '**/prototypes/**', '**/.qa/**'] } }, envPrefix: ['VITE_'], build: { target: 'es2022' } });
