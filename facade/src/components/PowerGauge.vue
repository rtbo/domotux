<template>
  <v-stage :config="stageSize">
    <v-layer>
      <v-arc
        :config="{
          x: stageSize.width / 2,
          y: stageSize.height / 2,
          innerRadius: 70,
          outerRadius: 90,
          angle: 180,
          rotation: -90,
          fill: '#ddd',
        }"
      />
      <v-arc
        :config="{
          x: stageSize.width / 2,
          y: stageSize.height / 2,
          innerRadius: 70,
          outerRadius: 90,
          angle: 90, // Example value, you can bind this to a reactive property for dynamic updates
          rotation: -90,
          fill: '#f00',
        }"
      />
    </v-layer>
  </v-stage>
</template>

<script setup lang="ts">
  import { Arc as VArc, Layer as VLayer, Stage as VStage } from 'vue-konva'

  const props = defineProps<{
    value: number
  }>()

  const stageSize = {
    width: 200,
    height: 200,
  }

</script>
