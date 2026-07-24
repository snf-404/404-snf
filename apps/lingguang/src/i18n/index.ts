import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

import { resources } from '@/i18n/resources'

export const LOCALE_STORAGE_KEY = 'repose.locale'
export type Locale = keyof typeof resources

function getInitialLocale(): Locale {
  try {
    const stored = window.localStorage.getItem(LOCALE_STORAGE_KEY)
    return stored === 'en-US' || stored === 'zh-CN' ? stored : 'zh-CN'
  } catch {
    return 'zh-CN'
  }
}

void i18n.use(initReactI18next).init({
  resources: {
    'zh-CN': { translation: resources['zh-CN'] },
    'en-US': { translation: resources['en-US'] },
  },
  lng: getInitialLocale(),
  fallbackLng: 'zh-CN',
  supportedLngs: ['zh-CN', 'en-US'],
  interpolation: { escapeValue: false },
  returnNull: false,
})

function persistLocale(language: string) {
  const locale: Locale = language === 'en-US' ? 'en-US' : 'zh-CN'
  document.documentElement.lang = locale
  try {
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale)
  } catch {
    // Language switching still works for the current session when storage is unavailable.
  }
}

persistLocale(i18n.resolvedLanguage ?? i18n.language)
i18n.on('languageChanged', persistLocale)

export default i18n
