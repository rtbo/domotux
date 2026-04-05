<template>
  <v-responsive class="rounded">
    <v-app>
      <v-app-bar v-if="showMenu" title="Domotux">
        <!-- Button on the right for logout -->
        <v-btn text @click="authStore.logout()">
          <v-icon>mdi-logout</v-icon>
        </v-btn>
      </v-app-bar>
      <v-main>
        <router-view />
      </v-main>
    </v-app>
  </v-responsive>
</template>

<script lang="ts" setup>
  import { computed, onMounted } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { useRoute } from 'vue-router'
  import { useAuthStore } from './stores/auth'

  const { locale } = useI18n()

  const route = useRoute()
  const authStore = useAuthStore()

  const showMenu = computed(() => authStore.isAuthenticated && route.path !== '/login')

  onMounted(() => {
    const browserLanguage = navigator.language || (navigator as any).userLanguage
    const lang = browserLanguage?.split('-')[0] // Get the language code (e.g., 'en' from 'en-US')
    const supportedLanguages = ['en', 'fr']
    const defaultLanguage = 'en'
    const language = supportedLanguages.includes(lang) ? lang : defaultLanguage
    document.documentElement.lang = language
    locale.value = language
  })
</script>
