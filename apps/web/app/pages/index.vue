<script setup lang="ts">
const { locale, locales, setLocale } = useI18n()

const localeItems = computed(() =>
  locales.value.map((item) => ({
    label: typeof item === 'string' ? item : item.name,
    value: typeof item === 'string' ? item : item.code,
  })),
)
</script>

<template>
  <main class="mx-auto flex min-h-screen max-w-5xl flex-col px-6 py-8">
    <header class="flex items-center justify-between border-b border-default pb-5">
      <span class="text-lg font-semibold">404 SNF</span>
      <USelect
        :model-value="locale"
        :items="localeItems"
        value-key="value"
        class="w-32"
        :aria-label="$t('language')"
        @update:model-value="setLocale"
      />
    </header>

    <section class="flex flex-1 flex-col justify-center py-16">
      <p class="mb-3 text-sm font-medium text-primary">
        {{ $t('eyebrow') }}
      </p>
      <h1 class="max-w-3xl text-4xl font-semibold sm:text-5xl">
        {{ $t('title') }}
      </h1>
      <p class="mt-5 max-w-2xl text-lg text-muted">
        {{ $t('description') }}
      </p>
      <div class="mt-8">
        <UButton icon="i-lucide-bluetooth" size="lg">
          {{ $t('connect') }}
        </UButton>
      </div>
    </section>
  </main>
</template>
