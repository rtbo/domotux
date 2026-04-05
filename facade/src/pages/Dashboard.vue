<template>
  <h1>Domotux Dashboard</h1>
  <p>PApp: {{ dashboardStore.papp }}</p>
  <p>Prix KWh: {{ dashboardStore.kwhPrice }}</p>
  <v-card
    outlined
    :title="$t('message.papp')"
  >
    <PowerGauge :value="dashboardStore.papp" />
  </v-card>
</template>

<script setup lang="ts">
  import { onBeforeUnmount } from 'vue'
  import PowerGauge from '@/components/PowerGauge.vue'
  import { useDashboardStore } from '@/stores/dashboard'

  const dashboardStore = useDashboardStore()

  onBeforeUnmount(() => {
    dashboardStore.closeWebSocket()
  })

</script>
