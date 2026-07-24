import { createContext, useContext } from 'react'

import { resources, type TranslationKey } from '@/i18n/resources'

export const LOCALE_STORAGE_KEY = 'repose.locale'
export type Locale = keyof typeof resources
export type TranslationValues = Record<string, string | number>
export type TranslationFunction = (key: TranslationKey, values?: TranslationValues) => string

export type I18nController = {
  language: Locale
  resolvedLanguage: Locale
  changeLanguage: (language: string) => void
}

export type TranslationContextValue = {
  t: TranslationFunction
  i18n: I18nController
}

export const TranslationContext = createContext<TranslationContextValue | null>(null)

export function normalizeLocale(value: unknown): Locale {
  return value === 'en-US' ? 'en-US' : 'zh-CN'
}

export function getInitialLocale(): Locale {
  try {
    return normalizeLocale(window.localStorage.getItem(LOCALE_STORAGE_KEY))
  } catch {
    return 'zh-CN'
  }
}

export function translateMessage(
  locale: Locale,
  key: TranslationKey,
  values: TranslationValues = {},
): string {
  return resources[locale][key].replace(/\{\{(\w+)\}\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(values, name) ? String(values[name]) : match,
  )
}

export function useTranslation(namespace?: 'translation'): TranslationContextValue {
  void namespace
  const context = useContext(TranslationContext)
  if (context === null) throw new Error('useTranslation must be used within I18nProvider')
  return context
}
