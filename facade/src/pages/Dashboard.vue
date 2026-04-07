<template>
  <v-container>
    <v-row>
      <v-col cols="12" lg="6">
        <v-card
          class="d-flex fill-height flex-column align-center align-items-center justify-center pa-4"
          elevation="12"
          outlined
          rounded
          :title="$t('message.papp')"
        >
          <PowerGauge
            :maxp="contratStore.subscKva * 1000"
            :papp="pappStore.papp"
            :prix-kwh-actif="contratStore.prixKwhActif"
            :stops="gaugeStops"
          />
        </v-card>
      </v-col>
      <v-col cols="12" lg="6">
        <TempoCard
          v-if="contratStore.option === 'tempo'"
          class="fill-height"
          :compteur-actif="contratStore.compteurActif"
          :couleur-demain="contratStore.couleurDemain"
          :prix-kwh="contratStore.prixKwh"
          :prix-kwh-actif="contratStore.prixKwhActif"
          :subsc-kva="contratStore.subscKva"
        />
      </v-col>
    </v-row>
  </v-container>
</template>

<script setup lang="ts">
  import { computed, onBeforeUnmount } from 'vue'
  import PowerGauge from '@/components/PowerGauge.vue'
  import TempoCard from '@/components/TempoCard.vue'
  import { useContratStore, usePappStore } from '@/stores/dashboard'

  const pappStore = usePappStore()
  const contratStore = useContratStore()

  const gaugeStops = computed(() => [
    { value: 1000, color: '#0a0' },
    { value: contratStore.subscKva * 500, color: '#aa0' },
    { value: contratStore.subscKva * 1000, color: '#a00' },
  ])

  onBeforeUnmount(() => {
    pappStore.closePappWs()
  })

</script>
