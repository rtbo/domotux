import { defineStore, storeToRefs } from 'pinia'
import { ref, watch } from 'vue'
import { wsUrl } from '@/api'
import { useAuthStore } from './auth'

export const useDashboardStore = defineStore('dashboard', () => {
  const authStore = useAuthStore()
  const { jwToken } = storeToRefs(authStore)

  const papp = ref(0)
  const kwhPrice = ref(0)

  let ws: WebSocket | null = null

  function closeWebSocket () {
    if (ws) {
      console.log('Closing existing WebSocket connection...')
      ws.close()
      ws = null
    }
  }

  function connectWebSocket (newToken: string) {
    closeWebSocket()
    if (newToken) {
      console.log('JWT token updated, reconnecting WebSocket...')

      let established = false

      ws = new WebSocket(wsUrl('/dashboard_ws?token=' + newToken))

      ws.addEventListener('open', () => {
        console.log('WebSocket connection established.')
        established = true
      })
      ws.addEventListener('message', event => {
        console.log('Received WebSocket message:', event.data)
        try {
          if (event.data.startsWith('papp=')) {
            const pappValue = Number.parseFloat(event.data.split('=')[1])
            if (!Number.isNaN(pappValue)) {
              papp.value = pappValue
            }
          } else if (event.data.startsWith('prixKwh=')) {
            const kwhPriceValue = Number.parseFloat(event.data.split('=')[1])
            if (!Number.isNaN(kwhPriceValue)) {
              kwhPrice.value = kwhPriceValue
            }
          }
        } catch (error) {
          console.error('Failed to parse WebSocket message:', error)
        }
      })
      ws.addEventListener('close', () => {
        console.log('WebSocket connection closed.')
        if (!established) {
          console.error('WebSocket connection failed to establish. Please check your JWT token and server status.')
          authStore.logout()
        }
      })
      ws.addEventListener('error', error => {
        console.error('WebSocket error:', error)
      })
    }
  }

  watch(jwToken, newToken => {
    connectWebSocket(newToken)
  }, { immediate: true })

  return {
    papp,
    kwhPrice,
    closeWebSocket,
  }
})
