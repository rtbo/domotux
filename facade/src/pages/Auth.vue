<template>
  <v-sheet class="pa-12" rounded>
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
      </v-form>
    </v-card>
  </v-sheet>
</template>

<script setup>
  import { ref } from 'vue'
  import { authUser } from '@/api'

  const form = ref(false)
  const name = ref(null)
  const password = ref(null)
  const loading = ref(false)

  function onSubmit () {
    if (!form.value) return
    loading.value = true
    const jwt = authUser(name.value, password.value)
    console.log(jwt)
  }
  function required (v) {
    return !!v || 'Field is required'
  }
</script>
