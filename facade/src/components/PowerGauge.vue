<template>
  <div class="power-gauge">
    <v-stage :config="stageSize">
      <v-layer>
        <v-arc
          v-for="section in sections"
          :key="section.key"
          :config="{
            x: stageSize.width / 2,
            y: stageSize.height / 2,
            innerRadius: midRadius,
            outerRadius: outerRadius,
            ...section,
          }"
        />
        <v-arc
          :config="{
            x: stageSize.width / 2,
            y: stageSize.height / 2,
            innerRadius: innerRadius,
            outerRadius: midRadius,
            angle: (props.papp / props.maxp) * (maxAngle - minAngle),
            rotation: 90 + minAngle,
            fill: color,
          }"
        />
      </v-layer>
    </v-stage>
    <div class="power-gauge__overlay">
      <div class="power-gauge__value">{{ props.papp }} W</div>
      <div class="power-gauge__cost">{{ hourlyCost.toFixed(2) }} €/h</div>
    </div>
  </div>
</template>

<script setup lang="ts">
  import { computed } from 'vue'
  import { Arc as VArc, Layer as VLayer, Stage as VStage } from 'vue-konva'

  const props = defineProps<{
    maxp: number
    papp: number
    prixKwhActif: number
    stops: { value: number, color: string }[]
  }>()

  const lastColor = '#a00'

  const color = computed(() => {
    let color = lastColor
    for (const stop of props.stops) {
      if (props.papp <= stop.value) {
        color = stop.color
        break
      }
    }
    return color
  })

  const hourlyCost = computed(() => (props.papp / 1000) * props.prixKwhActif)

  const stageSize = {
    width: 200,
    height: 200,
  }

  const outerRadius = 90
  const innerRadius = 70
  const midRadius = (outerRadius + innerRadius) / 2
  const angleRange = 270
  const minAngle = (360 - angleRange) / 2
  const maxAngle = 360 - minAngle

  const sections = computed(() => {
    const sections = []
    let lastAngle = 90 + minAngle
    let lastValue = 0
    let key = 0
    for (const stop of props.stops) {
      const angle = ((stop.value - lastValue) / props.maxp) * (maxAngle - minAngle)
      sections.push({
        key,
        fill: stop.color,
        rotation: lastAngle,
        angle: angle,
      })
      lastAngle += angle
      lastValue = stop.value
      key++
    }
    if (lastAngle < maxAngle) {
      sections.push({
        key,
        fill: lastColor,
        rotation: lastAngle,
        angle: maxAngle - lastAngle,
      })
    }
    return sections
  })

</script>

<style scoped>
  .power-gauge {
    position: relative;
    width: 200px;
    height: 200px;
  }

  .power-gauge__overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    pointer-events: none;
    text-align: center;
    color: inherit;
  }

  .power-gauge__value {
    font-size: 1.5rem;
    line-height: 1.1;
  }

  .power-gauge__cost {
    font-size: 1.125rem;
    line-height: 1.1;
  }
</style>
