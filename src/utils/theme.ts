export type ThemeMode = 'light' | 'dark' | 'system'

let currentTheme: ThemeMode = 'system'

export function applyTheme(theme: ThemeMode) {
  currentTheme = theme
  
  if (theme === 'system') {
    const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light')
    // 同步到 Element Plus
    document.documentElement.className = isDark ? 'dark' : ''
  } else {
    document.documentElement.setAttribute('data-theme', theme)
    // 同步到 Element Plus
    document.documentElement.className = theme === 'dark' ? 'dark' : ''
  }
}

// 监听系统主题变化
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  if (currentTheme === 'system') {
    applyTheme('system')
  }
})

export function getCurrentTheme(): ThemeMode {
  return currentTheme
}
