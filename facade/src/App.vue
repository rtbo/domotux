<template>
  <v-app>
    <v-main>
      <router-view />
    </v-main>
  </v-app>
</template>

<script lang="ts" setup>
  import { computed, onMounted } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { useRoute } from 'vue-router'

  const { locale } = useI18n()

  const route = useRoute()
  const showMenu = computed(() => route.name === 'auth')

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
