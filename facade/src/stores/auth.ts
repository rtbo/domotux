import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { authUser } from '@/api'
import router, { loginRedirectFor } from '@/router'

const JWT_STORAGE_KEY = 'domotux.jwt'

export const useAuthStore = defineStore('auth', () => {
  const jwToken = ref(localStorage.getItem(JWT_STORAGE_KEY) || '')
  const isAuthenticated = computed(() => !!jwToken.value)

  watch(jwToken, token => {
    if (token) {
      localStorage.setItem(JWT_STORAGE_KEY, token)
    } else {
      localStorage.removeItem(JWT_STORAGE_KEY)
      if (router.currentRoute.value.path !== '/login') {
        router.push(loginRedirectFor(router.currentRoute.value))
      }
    }
  })

  async function login (name: string, password: string) {
    jwToken.value = await authUser(name, password)
    console.log('User authenticated successfully.')
  }

  function logout () {
    jwToken.value = ''
    console.log('User logged out.')
  }

  return {
    login,
    logout,
    isAuthenticated,
    jwToken,
  }
})
