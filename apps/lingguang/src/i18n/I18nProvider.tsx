import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'

import {
  getInitialLocale,
  LOCALE_STORAGE_KEY,
  normalizeLocale,
  translateMessage,
  TranslationContext,
  type I18nController,
  type TranslationFunction,
} from '@/i18n/context'

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState(getInitialLocale)

  useEffect(() => {
    document.documentElement.lang = locale
    try {
      window.localStorage.setItem(LOCALE_STORAGE_KEY, locale)
    } catch {
      // The selected locale still applies for the current session when storage is unavailable.
    }
  }, [locale])

  const changeLanguage = useCallback((language: string) => {
    setLocale(normalizeLocale(language))
  }, [])

  const t = useCallback<TranslationFunction>(
    (key, values) => translateMessage(locale, key, values),
    [locale],
  )
  const i18n = useMemo<I18nController>(
    () => ({ language: locale, resolvedLanguage: locale, changeLanguage }),
    [changeLanguage, locale],
  )
  const contextValue = useMemo(() => ({ t, i18n }), [i18n, t])

  return <TranslationContext.Provider value={contextValue}>{children}</TranslationContext.Provider>
}
