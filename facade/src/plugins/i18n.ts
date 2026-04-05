import { createI18n } from 'vue-i18n'

const messages = {
  en: {
    message: {
      papp: 'Apparent Power',
      psousc: 'Subscribed Power',
      currently: 'Currently',
      tomorrow: 'Tomorrow',
      tempoContract: 'Tempo Contract',
    },
  },
  fr: {
    message: {
      papp: 'Puissance Apparente',
      currently: 'En ce moment',
      tomorrow: 'Demain',
      psousc: 'Puissance Souscrite',
      tempoContract: 'Contrat Tempo',
    },
  },
}

export default createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages,
})
