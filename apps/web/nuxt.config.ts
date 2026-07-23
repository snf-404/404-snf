export default defineNuxtConfig({
  compatibilityDate: '2026-07-23',
  devtools: { enabled: true },
  modules: ['@nuxt/ui', '@nuxtjs/i18n'],
  css: ['~/assets/css/main.css'],
  ui: {
    fonts: false,
  },
  i18n: {
    strategy: 'prefix_except_default',
    defaultLocale: 'zh',
    locales: [
      { code: 'zh', name: '中文', file: 'zh.json' },
      { code: 'en', name: 'English', file: 'en.json' },
    ],
  },
})
