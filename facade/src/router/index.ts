/**
 * router/index.ts
 *
 * Manual routes for ./src/pages/*.vue
 */

// Composables
import type { RouteLocationNormalizedGeneric } from 'vue-router'
import { createRouter, createWebHistory } from 'vue-router'
import Dashboard from '@/pages/Dashboard.vue'
import Index from '@/pages/index.vue'
import Login from '@/pages/Login.vue'
import { useAuthStore } from '@/stores/auth'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      component: Index,
    },
    {
      path: '/dashboard',
      component: Dashboard,
    },
    {
      name: 'Login',
      path: '/login',
      component: Login,
    },
  ],
})

export function loginRedirectFor (to: Pick<RouteLocationNormalizedGeneric, 'fullPath'>) {
  return {
    name: 'Login',
    query: { redirect: to.fullPath },
  }
}

router.beforeEach(to => {
  const authStore = useAuthStore()
  if (to.path !== '/login' && !authStore.isAuthenticated) {
    return loginRedirectFor(to)
  }
})

export default router
