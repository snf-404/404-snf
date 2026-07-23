<script setup lang="ts">
const { t } = useI18n()

const links = computed(() => [
  {
    label: t('connect'),
    icon: 'i-lucide-bluetooth',
  },
])
</script>

<template>
  <UPageHero
    :headline="$t('eyebrow')"
    :title="$t('title')"
    :description="$t('description')"
    :links="links"
  />
</template>
