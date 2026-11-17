import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { MotionPlugin } from '@vueuse/motion'
import { createRouter, createWebHashHistory } from 'vue-router'
import App from './App.vue'
import './style.css'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', component: App },
    { path: '/modes', component: App },
    { path: '/classic', component: App },
    { path: '/battle', component: App },
    { path: '/profile', component: App },
    { path: '/settings', component: App },
    { path: '/coming-soon', component: App },
  ],
})
createApp(App).use(createPinia()).use(MotionPlugin).use(router).mount('#app')
