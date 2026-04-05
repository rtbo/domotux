import { defineStore, storeToRefs } from 'pinia'
import { computed, ref, watch } from 'vue'
import { restUrl, wsUrl } from '@/api'
import { useAuthStore } from './auth'

export const usePappStore = defineStore('dashboard', () => {
  const authStore = useAuthStore()
  const { jwToken } = storeToRefs(authStore)

  const papp = ref(0)

  let pappWs: WebSocket | null = null

  function closePappWs () {
    if (pappWs) {
      console.log('Closing existing WebSocket connection...')
      pappWs.close()
      pappWs = null
    }
  }

  function openPappWs (jwToken: string) {
    closePappWs()
    if (jwToken) {
      console.log('JWT token updated, reconnecting WebSocket...')

      let established = false

      pappWs = new WebSocket(wsUrl('/papp_ws?token=' + jwToken))

      pappWs.addEventListener('open', () => {
        console.log('WebSocket connection established.')
        established = true
      })
      pappWs.addEventListener('message', event => {
        try {
          if (event.data.startsWith('papp=')) {
            const pappValue = Number.parseFloat(event.data.split('=')[1])
            if (!Number.isNaN(pappValue)) {
              papp.value = pappValue
            }
          }
        } catch (error) {
          console.error('Failed to parse WebSocket message:', error)
        }
      })
      pappWs.addEventListener('close', () => {
        console.log('WebSocket connection closed.')
        if (!established) {
          console.error('WebSocket connection failed to establish. Please check your JWT token and server status.')
          authStore.logout()
        }
      })
      pappWs.addEventListener('error', error => {
        console.error('WebSocket error:', error)
      })
    }
  }

  watch(jwToken, newToken => {
    if (newToken) {
      openPappWs(newToken)
    } else {
      closePappWs()
      papp.value = 0
    }
  }, { immediate: true })

  return {
    papp,
    closePappWs,
  }
})

interface InfoContrat {
  subscPower?: number
  option?: string
  compteurActif?: string
  prixKwhActif?: number
  prixKwh?: Record<string, number>
  couleurAjd?: string
  couleurDemain?: string
}

export const useContratStore = defineStore('contrat', () => {
  const authStore = useAuthStore()
  const { jwToken } = storeToRefs(authStore)

  const contrat = ref<InfoContrat>({})

  const prixKwhActif = computed(() => contrat.value.prixKwhActif || 0)
  const prixKwh = computed(() => contrat.value.prixKwh || {})
  const couleurAjd = computed(() => contrat.value.couleurAjd)
  const couleurDemain = computed(() => contrat.value.couleurDemain)
  const subscKva = computed(() => contrat.value.subscPower || 0)
  const option = computed(() => contrat.value.option || 'unknown')
  const compteurActif = computed(() => contrat.value.compteurActif || 'unknown')

  async function fetchContrat (jwToken: string) {
    const resp = await fetch(restUrl('/info_contrat'), {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${jwToken}`,
      },
    })
    if (resp.ok) {
      const data = await resp.json()
      contrat.value = data
    } else {
      console.error('Failed to fetch contrat:', resp.status, resp.statusText)
    }
  }

  watch(jwToken, newToken => {
    if (newToken) {
      fetchContrat(newToken)
    } else {
      contrat.value = {}
    }
  }, { immediate: true })

  return {
    prixKwhActif,
    prixKwh,
    couleurAjd,
    couleurDemain,
    subscKva,
    option,
    compteurActif,
  }
})
