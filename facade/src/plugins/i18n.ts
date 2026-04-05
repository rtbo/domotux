import { createI18n } from 'vue-i18n'

const messages = {
  en: {
    message: {
      papp: 'Apparent Power',
    },
  },
  fr: {
    message: {
      papp: 'Puissance Apparente',
    },
  },
}

export default createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages,
})
