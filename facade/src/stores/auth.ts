import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { authUser } from '@/api'

export const useAuthStore = defineStore('auth', () => {
  const jwToken = ref('')
  const isAuthenticated = computed(() => !!jwToken.value)

  async function login (name: string, password: string) {
    jwToken.value = await authUser(name, password)
    console.log('User authenticated successfully.')
    console.debug('JWT Token:', jwToken.value)
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
