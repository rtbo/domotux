<template>
  <v-card
    class="d-flex flex-column align-center align-items-center justify-center pa-4"
    elevation="12"
    outlined
    rounded
    :subtitle="`${$t('message.psousc')}: ${props.subscKva} KVA`"
    :title="$t('message.tempoContract')"
  >
    <v-container density="compact">
      <v-row align="stretch">
        <v-col class="d-flex" cols="12" sm="6">
          <v-card
            class="fill-height flex-grow-1"
            :color="colorAjd"
            :subtitle="$t('message.currently')"
            :title="colorNameAjd"
            variant="flat"
          >
            <v-card-text>
              <p>{{ hcHpAjd }}</p>
              <p>{{ props.prixKwhActif.toFixed(4) }} €/kWh</p>
            </v-card-text>
          </v-card>
        </v-col>
        <v-col class="d-flex" cols="12" sm="6">
          <v-card
            class="fill-height flex-grow-1"
            :color="colorDemain"
            :subtitle="$t('message.tomorrow')"
            :title="colorNameDemain"
            variant="flat"
          >
            <v-card-text v-if="props.couleurDemain">
              <p>HP: {{ prixHpDemain }} €/kWh</p>
              <p>HC: {{ prixHcDemain }} €/kWh</p>
            </v-card-text>
          </v-card>
        </v-col>
      </v-row>
    </v-container>
    <v-sheet />
  </v-card>
</template>

<script setup lang="ts">
  import { computed } from 'vue'

  const props = defineProps<{
    subscKva: number
    compteurActif: string
    prixKwhActif: number
    couleurDemain?: string
    prixKwh?: Record<string, number>
  }>()

  const colorNameAjd = computed(() => {
    if (props.compteurActif.startsWith('bleu')) return 'Bleu'
    else if (props.compteurActif.startsWith('blanc')) return 'Blanc'
    else if (props.compteurActif.startsWith('rouge')) return 'Rouge'
    else return '(inconnu)'
  })

  const colorAjd = computed(() => {
    if (props.compteurActif.startsWith('bleu')) return '#33c'
    else if (props.compteurActif.startsWith('blanc')) return '#aaa'
    else if (props.compteurActif.startsWith('rouge')) return '#a00'
    else return '#000'
  })

  const hcHpAjd = computed(() => {
    if (props.compteurActif.endsWith('Hc')) return 'Heures Creuses'
    else if (props.compteurActif.endsWith('Hp')) return 'Heures Pleines'
    else return 'inconnu'
  })

  const colorNameDemain = computed(() => {
    return props.couleurDemain ?? '(inconnu)'
  })

  const colorDemain = computed(() => {
    if (!props.couleurDemain) return '#aa3'
    if (props.couleurDemain.toLowerCase() === 'bleu') return '#33c'
    else if (props.couleurDemain.toLowerCase() === 'blanc') return '#aaa'
    else if (props.couleurDemain.toLowerCase() === 'rouge') return '#a00'
    else return '#000'
  })

  const prixHcDemain = computed(() => {
    if (!props.prixKwh) return null
    if (!props.couleurDemain) return null
    const hcKey = props.couleurDemain.toLowerCase() + 'Hc'
    return hcKey ? props.prixKwh[hcKey] : 0
  })

  const prixHpDemain = computed(() => {
    if (!props.prixKwh) return null
    if (!props.couleurDemain) return null
    const hpKey = props.couleurDemain.toLowerCase() + 'Hp'
    return hpKey ? props.prixKwh[hpKey] : 0
  })
</script>
