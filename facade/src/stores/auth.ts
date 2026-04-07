import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { authUser, checkAuth } from '@/api'
import router, { loginRedirectFor } from '@/router'

const JWT_STORAGE_KEY = 'domotux.jwt'

interface JwtClaims {
  sub: string
  exp: number
}

function parseJwtClaims (token: string): JwtClaims | null {
  const jwtParts = token.split('.')
  if (jwtParts.length !== 3) {
    return null
  }

  const payloadSegment = jwtParts[1]
  const normalizedPayload = payloadSegment.replace(/-/g, '+').replace(/_/g, '/')
  const paddedPayload = normalizedPayload.padEnd(Math.ceil(normalizedPayload.length / 4) * 4, '=')

  try {
    const parsedClaims = JSON.parse(atob(paddedPayload)) as Partial<JwtClaims>
    if (typeof parsedClaims.sub !== 'string' || typeof parsedClaims.exp !== 'number') {
      return null
    }

    return {
      sub: parsedClaims.sub,
      exp: parsedClaims.exp,
    }
  } catch {
    return null
  }
}

function hasValidExpClaim (token: string): boolean {
  const payload = parseJwtClaims(token)
  if (!payload || typeof payload.exp !== 'number' || !Number.isFinite(payload.exp)) {
    return false
  }

  return payload.exp * 1000 > Date.now()
}

export const useAuthStore = defineStore('auth', () => {
  const storedToken = localStorage.getItem(JWT_STORAGE_KEY) || ''
  const jwToken = ref('')
  const isAuthenticated = computed(() => !!jwToken.value)
  let initializePromise: Promise<void> | null = null

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

  async function initialize () {
    if (initializePromise) {
      return initializePromise
    }

    initializePromise = (async () => {
      if (!storedToken) {
        return
      }

      try {
        const serverCheck = checkAuth(storedToken)
        if (!hasValidExpClaim(storedToken)) {
          localStorage.removeItem(JWT_STORAGE_KEY)
          return
        }
        if (!await serverCheck) {
          localStorage.removeItem(JWT_STORAGE_KEY)
          return
        }
        jwToken.value = storedToken
      } catch (error) {
        console.error('Failed to validate stored JWT token:', error)
        localStorage.removeItem(JWT_STORAGE_KEY)
      }
    })()

    await initializePromise
  }

  async function login (name: string, password: string) {
    jwToken.value = await authUser(name, password)
    console.log('User authenticated successfully.')
  }

  function logout () {
    jwToken.value = ''
    console.log('User logged out.')
  }

  return {
    initialize,
    login,
    logout,
    isAuthenticated,
    jwToken,
  }
})
