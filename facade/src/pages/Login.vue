<template>
  <v-container class="fill-height d-flex align-center justify-center pa-12">
    <v-row>
      <v-col>
        <v-card class="mx-auto px-6 py-8" max-width="344">
          <v-form v-model="form" @submit.prevent="onSubmit">
            <v-text-field
              v-model="name"
              class="mb-2"
              clearable
              label="Name"
              :readonly="loading"
              :rules="[required]"
            />

            <v-text-field
              v-model="password"
              clearable
              label="Password"
              placeholder="Enter your password"
              :readonly="loading"
              :rules="[required]"
              type="password"
            />

            <br>

            <v-btn
              block
              color="success"
              :disabled="!form"
              :loading="loading"
              size="large"
              type="submit"
              variant="elevated"
            >
              Sign In
            </v-btn>
            <v-alert
              v-if="errorMsg"
              class="mt-4"
              color="error"
              variant="outlined"
            >
              {{ errorMsg }}
            </v-alert>
          </v-form>
        </v-card>

      </v-col>
    </v-row>
  </v-container>
</template>

<script setup lang="ts">
  import { ref } from 'vue'
  import { useRoute, useRouter } from 'vue-router'
  import { useAuthStore } from '@/stores/auth'

  const router = useRouter()
  const route = useRoute()
  const authStore = useAuthStore()

  const form = ref(false)
  const name = ref('')
  const password = ref('')
  const loading = ref(false)
  const errorMsg = ref('')

  async function onSubmit () {
    if (!form.value) return
    loading.value = true
    try {
      await authStore.login(name.value, password.value)
      const redirect = Array.isArray(route.query.redirect) ? route.query.redirect[0] : route.query.redirect
      await router.push((redirect as string) || '/')
    } catch (error) {
      console.error('Authentication error:', error)
      errorMsg.value = (error as Error).message
    }
    loading.value = false
  }
  function required (v: string) {
    return !!v || 'Field is required'
  }
</script>
