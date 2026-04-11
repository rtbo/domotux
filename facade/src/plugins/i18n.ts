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
      consoStats: 'Statistiques de Consommation',
      papp: 'Puissance Apparente',
      currently: 'En ce moment',
      tomorrow: 'Demain',
      psousc: 'Puissance Souscrite',
      tempoContract: 'Contrat Tempo',
    },
  },
}
const datetimeFormats = {
  en: {
    short: {
      year: 'numeric', month: 'short', day: 'numeric',
    },
    long: {
      year: 'numeric', month: 'short', day: 'numeric',
      weekday: 'short', hour: 'numeric', minute: 'numeric',
    },
  },
  fr: {
    short: {
      year: 'numeric', month: 'short', day: 'numeric',
    },
    long: {
      year: 'numeric', month: 'short', day: 'numeric',
      weekday: 'short', hour: 'numeric', minute: 'numeric', hour12: false,
    },
  },
}

export default createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages,
  datetimeFormats: datetimeFormats as any,
})
