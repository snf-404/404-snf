<script setup lang="ts">
const { t } = useI18n()

const {
  supported,
  state,
  error,
  deviceName,
  status,
  vitals,
  fatigue,
  vitalsQuality,
  connect,
  disconnect,
} = useSnfDevice()

const isConnected = computed(() => state.value === 'connected')
const isConnecting = computed(() => state.value === 'connecting')

/** Colour for the connection-state badge. */
const stateColor = computed(() => {
  switch (state.value) {
    case 'connected':
      return 'success'
    case 'connecting':
      return 'info'
    case 'error':
      return 'error'
    default:
      return 'neutral'
  }
})

/** Colour for the vitals freshness badge (`PROTOCOL.md` §14). */
const qualityColor = computed(() => {
  switch (vitalsQuality.value) {
    case 'live':
      return 'success'
    case 'stale':
      return 'warning'
    case 'lost':
      return 'error'
    default:
      return 'neutral'
  }
})

/** Round a bpm value for display, or the em dash when unavailable. */
function formatRate(bpm: number | null): string {
  return bpm === null ? t('unavailable') : Math.round(bpm).toString()
}

function formatUptime(seconds: number): string {
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = Math.floor(seconds % 60)
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${s}s`
  return `${s}s`
}

function formatBattery(mv: number | null): string {
  return mv === null ? t('unavailable') : `${(mv / 1000).toFixed(2)} V`
}

function formatTemp(c: number | null): string {
  return c === null ? t('unavailable') : `${c.toFixed(1)} °C`
}
</script>

<template>
  <UContainer class="py-10 space-y-8">
    <UPageHero
      :headline="t('eyebrow')"
      :title="t('title')"
      :description="t('description')"
      orientation="horizontal"
    >
      <template #links>
        <div class="flex flex-col items-start gap-3">
          <div class="flex items-center gap-3">
            <UButton
              v-if="!isConnected"
              icon="i-lucide-bluetooth"
              size="lg"
              :loading="isConnecting"
              :disabled="!supported || isConnecting"
              @click="connect"
            >
              {{ isConnecting ? t('connecting') : t('connect') }}
            </UButton>
            <UButton
              v-else
              icon="i-lucide-bluetooth-off"
              color="neutral"
              variant="soft"
              size="lg"
              @click="disconnect"
            >
              {{ t('disconnect') }}
            </UButton>

            <UBadge :color="stateColor" variant="subtle">
              {{ t(`state.${state}`) }}
            </UBadge>
            <span v-if="deviceName" class="text-sm text-muted">{{ deviceName }}</span>
          </div>
        </div>
      </template>
    </UPageHero>

    <UAlert
      v-if="!supported"
      icon="i-lucide-triangle-alert"
      color="warning"
      variant="subtle"
      :title="t('unsupported.title')"
      :description="t('unsupported.body')"
    />

    <UAlert
      v-else-if="state === 'error' && error"
      icon="i-lucide-circle-alert"
      color="error"
      variant="subtle"
      :title="t('state.error')"
      :description="error"
    />

    <p v-else-if="!isConnected" class="text-muted">{{ t('empty') }}</p>

    <div v-else class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
      <!-- Vitals -->
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="font-semibold">{{ t('vitals.title') }}</h3>
            <UBadge :color="qualityColor" variant="subtle" size="sm">
              {{ t(`quality.${vitalsQuality}`) }}
            </UBadge>
          </div>
        </template>
        <dl class="space-y-3">
          <div class="flex items-baseline justify-between">
            <dt class="text-sm text-muted">{{ t('vitals.heartRate') }}</dt>
            <dd class="text-2xl font-semibold tabular-nums">
              {{ formatRate(vitals?.heartRateBpm ?? null) }}
              <span class="text-sm font-normal text-muted">{{ t('vitals.unit') }}</span>
            </dd>
          </div>
          <div class="flex items-baseline justify-between">
            <dt class="text-sm text-muted">{{ t('vitals.respiration') }}</dt>
            <dd class="text-2xl font-semibold tabular-nums">
              {{ formatRate(vitals?.respirationRateBpm ?? null) }}
              <span class="text-sm font-normal text-muted">{{ t('vitals.unit') }}</span>
            </dd>
          </div>
        </dl>
      </UCard>

      <!-- Fatigue -->
      <UCard v-if="fatigue">
        <template #header>
          <h3 class="font-semibold">{{ t('fatigue.title') }}</h3>
        </template>
        <dl class="space-y-3">
          <div class="flex items-baseline justify-between">
            <dt class="text-sm text-muted">{{ t('fatigue.level') }}</dt>
            <dd class="text-2xl font-semibold tabular-nums">{{ fatigue.level }}</dd>
          </div>
          <div class="flex items-baseline justify-between">
            <dt class="text-sm text-muted">{{ t('fatigue.confidence') }}</dt>
            <dd class="text-2xl font-semibold tabular-nums">{{ fatigue.confidence }}%</dd>
          </div>
        </dl>
      </UCard>

      <!-- Device status -->
      <UCard v-if="status">
        <template #header>
          <h3 class="font-semibold">{{ t('status.title') }}</h3>
        </template>
        <dl class="space-y-2 text-sm">
          <div class="flex justify-between">
            <dt class="text-muted">{{ t('status.uptime') }}</dt>
            <dd class="tabular-nums">{{ formatUptime(status.uptimeS) }}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-muted">{{ t('status.battery') }}</dt>
            <dd class="tabular-nums">{{ formatBattery(status.batteryMv) }}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-muted">{{ t('status.temperature') }}</dt>
            <dd class="tabular-nums">{{ formatTemp(status.processorTempC) }}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-muted">{{ t('status.dropped') }}</dt>
            <dd class="tabular-nums">
              {{ status.droppedPoseFrames + status.droppedPointFrames }}
            </dd>
          </div>
        </dl>
      </UCard>
    </div>
  </UContainer>
</template>
