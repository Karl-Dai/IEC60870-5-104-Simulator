import { createApp } from 'vue'
import '@shared/styles/tokens.css'
import '@shared/styles/base.css'
import '@shared/styles/transitions.css'
import '@shared/styles/toolbar.css'
import '@shared/composables/useTheme'
import App from './App.vue'

createApp(App).mount('#app')
