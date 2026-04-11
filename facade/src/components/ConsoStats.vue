<template>
  <v-card>
    <v-card-title>{{ t('consoStats') }}</v-card-title>
    <v-card-subtitle v-if="stats">
      {{ t('dataAvailableSince', { date: d(new Date(Date.parse(stats?.dataStart)), 'long') }) }}
    </v-card-subtitle>
    <v-card-text>
      <v-table v-if="stats">
        <thead>
          <tr>
            <th>{{ t('period') }}</th>
            <th>{{ t('conso') }}</th>
            <th>{{ t('compteurs') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rowsData" :key="row.period">
            <td>
              <span class="d-block text-title-medium mb-2">{{ row.period }}</span>
              <span class="d-block">{{ t('from', { date: d(new Date(Date.parse(row.data.start)), 'long') }) }}</span>
              <span class="d-block">{{ t('to', { date: d(new Date(Date.parse(row.data.end)), 'long') }) }}</span>
            </td>
            <td>
              <span class="d-block text-body-large">{{ formatKwh(row.data.totalKwh) }}</span>
              <span class="d-block text-body-large">{{ formatPrice(row.data.totalCost) }}</span>
            </td>
            <td>
              <ul class="compteurs-list w-full">
                <li v-for="(compteur, index) in row.data.compteurs" :key="index" class="compteur-row">
                  <span class="compteur-name text-title-small">
                    {{ compteurNames[index] ?? index }}
                  </span>
                  <span v-if="compteur.kwh > 0" class="compteur-value">
                    {{ formatKwh(compteur.kwh) }}, {{ formatPrice(compteur.cost) }}
                  </span>
                  <span v-else class="compteur-value">-</span>
                </li>
              </ul>
            </td>
          </tr>
        </tbody>
      </v-table>
    </v-card-text>
  </v-card>
</template>

<script setup lang="ts">
  import type { ConsoStats } from '@/api'
  import { computed, onMounted, ref, watch } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { fetchConsoStats } from '@/api'
  import { useAuthStore } from '@/stores/auth'

  const authStore = useAuthStore()
  const { t } = useI18n({
    useScope: 'local',
    inheritLocale: true,
    messages: {
      en: {
        consoStats: 'Consumption Statistics',
        dataAvailableSince: 'Data available since {date}',
        period: 'Period',
        conso: 'Consumption',
        price: 'Price',
        compteurs: 'Meters',
        from: 'from {date}',
        to: 'to {date}',
        today: 'Today',
        yesterday: 'Yesterday',
        thisWeek: 'This week',
        lastWeek: 'Last week',
        thisMonth: 'This month',
        lastMonth: 'Last month',
        thisYear: 'This year',
        lastYear: 'Last year',
      },
      fr: {
        consoStats: 'Statistiques de Consommation',
        dataAvailableSince: 'Données disponibles depuis le {date}',
        period: 'Période',
        conso: 'Consommation',
        price: 'Prix',
        compteurs: 'Compteurs',
        from: 'du {date}',
        to: 'au {date}',
        today: 'Aujourd\'hui',
        yesterday: 'Hier',
        thisWeek: 'Cette semaine',
        lastWeek: 'La semaine dernière',
        thisMonth: 'Ce mois',
        lastMonth: 'Le mois dernier',
        thisYear: 'Cette année',
        lastYear: 'L\'année dernière',
      },
    },
  })
  const { d } = useI18n({ useScope: 'global' })

  const compteurNames: Record<string, string> = {
    hc: 'HC',
    hp: 'HP',
    bleuHc: 'Bleu HC',
    bleuHp: 'Bleu HP',
    blancHc: 'Blanc HC',
    blancHp: 'Blanc HP',
    rougeHc: 'Rouge HC',
    rougeHp: 'Rouge HP',
  }

  const stats = ref<ConsoStats | null>(null)

  function formatPrice (price: number): string {
    return price.toFixed(2) + ' €'
  }
  function formatKwh (kwh: number): string {
    if (kwh < 1) {
      return (kwh * 1000).toFixed(0) + ' Wh'
    } else if (kwh < 10) {
      return kwh.toFixed(2) + ' kWh'
    } else if (kwh < 100) {
      return kwh.toFixed(1) + ' kWh'
    } else {
      return kwh.toFixed(0) + ' kWh'
    }
  }

  const rowsData = computed(() => {
    if (!stats.value) return []

    const rows = [
      {
        period: t('today'),
        data: stats.value.today,
      },
    ]
    if (stats.value.yesterday) {
      rows.push({
        period: t('yesterday'),
        data: stats.value.yesterday,
      })
    }
    rows.push(
      {
        period: t('thisWeek'),
        data: stats.value.thisWeek,
      },
    )
    if (stats.value.lastWeek) {
      rows.push({
        period: t('lastWeek'),
        data: stats.value.lastWeek,
      })
    }
    rows.push(
      {
        period: t('thisMonth'),
        data: stats.value.thisMonth,
      },
    )
    if (stats.value.lastMonth) {
      rows.push({
        period: t('lastMonth'),
        data: stats.value.lastMonth,
      })
    }

    rows.push(
      {
        period: t('thisYear'),
        data: stats.value.thisYear,
      },
    )
    if (stats.value.lastYear) {
      rows.push({
        period: t('lastYear'),
        data: stats.value.lastYear,
      })
    }

    return rows
  })

  async function loadStats () {
    if (!authStore.isAuthenticated) {
      stats.value = null
      return
    }

    try {
      stats.value = await fetchConsoStats(authStore.jwToken)
    } catch (error) {
      console.error('Error fetching conso stats:', error)
    }
  }

  watch(
    () => authStore.jwToken,
    async _newToken => {
      await loadStats()
    },
  )
  onMounted(async () => {
    await loadStats()
  })

</script>

<style scoped>

  .compteurs-list {
    margin-left: 0;
    padding-left: 1.25rem;
    list-style-type: none;
  }

  .compteur-row {
    display: flex;
    align-items: baseline;
  }

  .compteur-name {
    flex: 0 0 20%;
    width: 20%;
    text-align: right;
    margin-right: 0.5rem;
  }

  .compteur-value {
    flex: 1;
  }

  @media (max-width: 1145px) {
    .compteur-row {
      flex-direction: column;
      align-items: flex-start;
    }

    .compteur-name {
      flex: unset;
      width: auto;
      text-align: left;
      margin-right: 0;
    }

    .compteur-value {
      padding-left: 0.5rem;
    }
  }
</style>
