<template>
  <h1>Domotux Dashboard</h1>
  <p>PApp: {{ dashboardStore.papp }}</p>
  <p>Prix KWh: {{ dashboardStore.kwhPrice }}</p>
</template>

<script setup lang="ts">
  import { useDashboardStore } from '@/stores/dashboard'
import { onBeforeUnmount } from 'vue';

  const dashboardStore = useDashboardStore()

  onBeforeUnmount(() => {
    dashboardStore.closeWebSocket()
  })

</script>
