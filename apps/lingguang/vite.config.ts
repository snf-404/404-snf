import path from 'path'

import react from '@vitejs/plugin-react'
import { createLogger, defineConfig, loadEnv } from 'vite'

import { asapBuildContractPlugin } from './plugins/vite-plugin-asap-build-contract'
import { blockAudioApisPlugin } from './plugins/vite-plugin-block-audio-apis'
import { blockDeviceEventsPlugin } from './plugins/vite-plugin-block-device-events'
import { blockFetchPlugin } from './plugins/vite-plugin-block-fetch'
import { blockFileInputPlugin } from './plugins/vite-plugin-block-file-input'
import { blockFormSubmitPlugin } from './plugins/vite-plugin-block-form-submit'
import { blockGoogleFontImportPlugin } from './plugins/vite-plugin-block-google-font-import'
import { blockParentWindowAccessPlugin } from './plugins/vite-plugin-block-parent-window-access'
import { blockReactRouterDomRoutersPlugin } from './plugins/vite-plugin-block-react-router-dom-routers'
import { blockRemoteImportsPlugin } from './plugins/vite-plugin-block-remote'
import { blockScancodeApisPlugin } from './plugins/vite-plugin-block-scancode-apis'
import { collectLingguangUsagePlugin } from './plugins/vite-plugin-collect-lingguang-usage'
import { injectManifestPlugin } from './plugins/vite-plugin-inject-manifest'
import { instrumentCatchPlugin } from './plugins/vite-plugin-instrument-catch'
import { llmProxyPlugin } from './plugins/vite-plugin-llm-proxy'
import { rewriteSafeAreaEnvPlugin } from './plugins/vite-plugin-rewrite-safe-area-env'
import { validateManifestPlugin } from './plugins/vite-plugin-validate-manifest'

const viteLogger = createLogger()
const originalWarn = viteLogger.warn.bind(viteLogger)

viteLogger.warn = (msg, options) => {
  if (msg.includes(`can't be bundled without type="module" attribute`)) {
    return
  }
  originalWarn(msg, options)
}

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const envValue = (...names: string[]) =>
    names.map((name) => process.env[name] ?? env[name] ?? '').find((value) => value !== '') ?? ''

  return {
    customLogger: viteLogger,
    plugins: [
      react(),
      llmProxyPlugin({
        model: envValue('LLM_MODEL', 'MODEL'),
        baseUrl: envValue('LLM_BASE_URL', 'BASE_URL', 'BASEURL'),
        apiKey: envValue('LLM_API_KEY', 'API_KEY', 'APIKEY'),
      }),
      blockRemoteImportsPlugin(),
      blockGoogleFontImportPlugin(),
      blockAudioApisPlugin(),
      blockDeviceEventsPlugin(),
      blockScancodeApisPlugin(),
      blockFileInputPlugin(),
      blockFetchPlugin(),
      blockParentWindowAccessPlugin(),
      blockFormSubmitPlugin(),
      blockReactRouterDomRoutersPlugin(),
      rewriteSafeAreaEnvPlugin(),
      instrumentCatchPlugin(),
      validateManifestPlugin(),
      injectManifestPlugin(),
      collectLingguangUsagePlugin(),
      asapBuildContractPlugin(),
    ],
    build: {
      sourcemap: 'hidden', // 生成 .map 文件
    },
    base: './',
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
  }
})
