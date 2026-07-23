<script setup lang="ts">
import { en, zh_cn } from '@nuxt/ui/locale'

const { locale, setLocale } = useI18n()

const uiLocales = [zh_cn, en]
const uiLocale = computed(() => (locale.value === 'zh' ? zh_cn : en))

function setUiLocale(code: string) {
  return setLocale(code === zh_cn.code ? 'zh' : 'en')
}

useHead({
  htmlAttrs: {
    lang: computed(() => uiLocale.value.code),
    dir: computed(() => uiLocale.value.dir),
  },
})
</script>

<template>
  <UApp :locale="uiLocale">
    <UHeader>
      <template #title> 404 SNF </template>

      <template #right>
        <ULocaleSelect
          :model-value="uiLocale.code"
          :locales="uiLocales"
          @update:model-value="setUiLocale"
        />
        <UColorModeButton />
      </template>
    </UHeader>

    <UMain>
      <NuxtPage />
    </UMain>
  </UApp>
</template>
