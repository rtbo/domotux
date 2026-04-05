<template>
  <v-app>
    <v-main>
      <router-view />
    </v-main>
  </v-app>
</template>

<script lang="ts" setup>
  import { computed } from 'vue'
  import { useRoute } from 'vue-router'

  const route = useRoute()
  const showMenu = computed(() => route.name === 'auth')
</script>
