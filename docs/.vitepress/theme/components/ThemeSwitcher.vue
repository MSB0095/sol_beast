<template>
  <div class="theme-switcher">
    <button
      @click="toggleDropdown"
      class="theme-button"
      :style="{ 
        borderColor: currentThemeData.primaryColor,
        boxShadow: `0 0 15px ${currentThemeData.primaryColor}60`
      }"
      aria-label="Change color theme"
    >
      <span class="theme-icon">{{ currentThemeData.icon }}</span>
      <span class="theme-label">THEME</span>
    </button>

    <Teleport to="body">
      <div 
        v-if="isOpen" 
        class="theme-dropdown-overlay"
        @click="toggleDropdown"
      >
        <div 
          class="theme-dropdown"
          :style="{
            borderColor: currentThemeData.primaryColor,
            boxShadow: `0 0 40px ${currentThemeData.primaryColor}90, inset 0 0 30px rgba(0,0,0,0.8)`
          }"
          @click.stop
        >
          <div class="dropdown-header">
            <p class="dropdown-title" :style="{ color: currentThemeData.primaryColor }">
              [COLOR SCHEME]
            </p>
          </div>
          
          <div class="theme-grid">
            <button
              v-for="theme in COLOR_THEMES"
              :key="theme.id"
              @click="selectTheme(theme.id)"
              class="theme-option"
              :class="{ active: currentTheme === theme.id }"
              :style="getThemeOptionStyle(theme)"
            >
              <div class="theme-option-content">
                <div 
                  class="theme-preview"
                  :style="{ 
                    backgroundColor: theme.primaryColor,
                    boxShadow: `0 0 20px ${theme.primaryColor}, inset 0 0 10px rgba(0,0,0,0.5)`,
                    color: '#000'
                  }"
                >
                  {{ theme.icon }}
                </div>
                <span 
                  class="theme-name"
                  :style="{ color: theme.primaryColor }"
                >
                  {{ theme.name }}
                </span>
                <div 
                  v-if="currentTheme === theme.id"
                  class="active-indicator"
                  :style="{ 
                    backgroundColor: theme.primaryColor,
                    boxShadow: `0 0 10px ${theme.primaryColor}`
                  }"
                ></div>
              </div>
            </button>
          </div>

          <div class="dropdown-footer">
            <p class="footer-text" :style="{ color: currentThemeData.primaryColor }">
              // SYSTEM-WIDE COLOR OVERRIDE //
            </p>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'

interface ColorTheme {
  id: string
  name: string
  primaryColor: string
  icon: string
}

const COLOR_THEMES: ColorTheme[] = [
  { 
    id: 'sol-green', 
    name: 'MATRIX', 
    primaryColor: '#00ff41',
    icon: '◉'
  },
  { 
    id: 'emerald', 
    name: 'NEON', 
    primaryColor: '#10ffb0',
    icon: '◈'
  },
  { 
    id: 'blue', 
    name: 'CYBER', 
    primaryColor: '#00d9ff',
    icon: '◆'
  },
  { 
    id: 'purple', 
    name: 'PLASMA', 
    primaryColor: '#d946ef',
    icon: '◇'
  },
  { 
    id: 'rose', 
    name: 'LASER', 
    primaryColor: '#ff0062',
    icon: '◊'
  },
  { 
    id: 'amber', 
    name: 'GOLD', 
    primaryColor: '#ffb000',
    icon: '◐'
  },
  { 
    id: 'cyan', 
    name: 'TRON', 
    primaryColor: '#00ffff',
    icon: '◑'
  },
]

const currentTheme = ref('sol-green')
const isOpen = ref(false)

const currentThemeData = computed(() => {
  return COLOR_THEMES.find(t => t.id === currentTheme.value) || COLOR_THEMES[0]
})

const applyTheme = (themeId: string) => {
  document.documentElement.setAttribute('data-color-theme', themeId)
  localStorage.setItem('colorTheme', themeId)
}

const selectTheme = (themeId: string) => {
  currentTheme.value = themeId
  applyTheme(themeId)
  isOpen.value = false
}

const toggleDropdown = () => {
  isOpen.value = !isOpen.value
}

const getThemeOptionStyle = (theme: ColorTheme) => {
  if (currentTheme.value === theme.id) {
    return {
      backgroundColor: `${theme.primaryColor}15`,
      borderColor: 'white',
      boxShadow: `0 0 20px ${theme.primaryColor}60, inset 0 0 20px rgba(0,0,0,0.5)`
    }
  }
  return {
    borderColor: 'transparent'
  }
}

onMounted(() => {
  const savedTheme = localStorage.getItem('colorTheme') || 'sol-green'
  currentTheme.value = savedTheme
  applyTheme(savedTheme)
})
</script>

<style scoped>
.theme-switcher {
  display: flex;
  align-items: center;
  margin-left: 12px;
}

.theme-button {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  background: #000;
  border: 2px solid;
  color: var(--theme-accent);
  cursor: pointer;
  font-family: monospace;
  font-size: 12px;
  font-weight: bold;
  letter-spacing: 2px;
  text-transform: uppercase;
  transition: all 0.3s ease;
}

.theme-button:hover {
  background: var(--theme-bg-secondary);
  transform: translateY(-2px);
}

.theme-icon {
  font-size: 16px;
}

.theme-label {
  display: none;
}

@media (min-width: 640px) {
  .theme-label {
    display: inline;
  }
}

.theme-dropdown-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.8);
  z-index: 10000;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 80px;
}

.theme-dropdown {
  width: 90%;
  max-width: 500px;
  background: #000;
  border: 2px solid;
  padding: 24px;
  z-index: 10001;
}

.dropdown-header {
  margin-bottom: 20px;
}

.dropdown-title {
  font-family: monospace;
  font-size: 14px;
  font-weight: 900;
  letter-spacing: 3px;
  text-transform: uppercase;
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
  margin-bottom: 20px;
}

.theme-option {
  position: relative;
  padding: 20px;
  border: 2px solid;
  background: #000;
  cursor: pointer;
  transition: all 0.2s ease;
}

.theme-option:hover {
  border-color: #666 !important;
}

.theme-option.active {
  border-color: white !important;
}

.theme-option-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.theme-preview {
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 24px;
  font-weight: bold;
}

.theme-name {
  font-family: monospace;
  font-size: 10px;
  font-weight: bold;
  letter-spacing: 2px;
  text-transform: uppercase;
}

.active-indicator {
  position: absolute;
  top: 8px;
  right: 8px;
  width: 8px;
  height: 8px;
}

.dropdown-footer {
  padding-top: 16px;
  border-top: 2px solid var(--theme-accent);
  opacity: 0.3;
}

.footer-text {
  font-family: monospace;
  font-size: 9px;
  letter-spacing: 1px;
  text-align: center;
  text-transform: uppercase;
}

@media (min-width: 640px) {
  .theme-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}
</style>
