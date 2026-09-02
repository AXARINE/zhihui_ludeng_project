<script setup>
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { login } from '@/api/device'

const router = useRouter()
const form = ref({ username: '', password: '' })
const loading = ref(false)
const error = ref('')

async function handleLogin() {
  if (!form.value.username || !form.value.password) {
    error.value = '请输入用户名和密码'
    return
  }
  loading.value = true
  error.value = ''
  try {
    const res = await login(form.value.username, form.value.password)
    localStorage.setItem('token', res.token)
    localStorage.setItem('user', JSON.stringify(res.user))
    // 保存角色和权限信息（后端登录接口现在会返回这些）
    if (res.role) localStorage.setItem('role', JSON.stringify(res.role))
    if (res.permissions) localStorage.setItem('permissions', JSON.stringify(res.permissions))
    router.push('/')
  } catch (e) {
    error.value = e?.response?.data || '登录失败，请检查用户名和密码'
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="login-page">
    <div class="login-card">
      <!-- 左侧品牌区（窄屏隐藏） -->
      <div class="brand-pane">
        <div class="brand-mark">灯</div>
        <h1>启晖智慧路灯</h1>
        <p class="brand-sub">IoT 管理系统</p>
        <p class="brand-foot">BearPi-HM Nano · 华为云 IoTDA</p>
      </div>

      <!-- 右侧表单区 -->
      <div class="form-pane">
        <h2>登录</h2>
        <p class="subtitle">欢迎回来，请登录你的账号</p>
        <div v-if="error" class="error-msg">{{ error }}</div>
        <div class="form-group">
          <label>用户名</label>
          <input v-model="form.username" placeholder="请输入用户名" @keyup.enter="handleLogin" />
        </div>
        <div class="form-group">
          <label>密码</label>
          <input v-model="form.password" type="password" placeholder="请输入密码" @keyup.enter="handleLogin" />
        </div>
        <button class="login-btn" :disabled="loading" @click="handleLogin">
          {{ loading ? '登录中...' : '登录' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.login-page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 深夜渐变底：夜空蓝黑 + 顶部青蓝夜色 + 底部城市反光的琥珀余晖 */
  background:
    radial-gradient(1000px 480px at 82% -12%, rgba(79, 179, 200, 0.12), transparent 62%),
    radial-gradient(860px 460px at 10% 112%, rgba(232, 163, 61, 0.1), transparent 62%),
    linear-gradient(180deg, #0c1220 0%, #080d18 100%);
  padding: 20px;
}

.login-card {
  display: flex;
  width: 720px;
  max-width: 100%;
  background: var(--bg-panel);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: var(--shadow-lg), var(--glow-amber);
}

/* ---- 左侧夜色品牌区 ---- */
.brand-pane {
  position: relative;
  width: 300px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  padding: 44px 36px;
  background: var(--bg-inset);
  color: var(--text-primary);
  overflow: hidden;
}

.brand-pane::before {
  content: '';
  position: absolute;
  inset: 0;
  background: radial-gradient(360px 220px at 85% -5%, rgba(232, 163, 61, 0.32), transparent 62%);
  pointer-events: none;
}

.brand-mark {
  position: relative;
  width: 44px;
  height: 44px;
  border-radius: 12px;
  background: linear-gradient(135deg, var(--primary-color), var(--primary-dark));
  color: #241a08;
  font-family: var(--font-serif);
  font-size: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 10px rgba(232, 163, 61, 0.4);
}

.brand-pane h1 {
  position: relative;
  margin: auto 0 0;
  padding-top: 120px;
  font-family: var(--font-serif);
  font-size: 26px;
  font-weight: 600;
  letter-spacing: 0.04em;
}

.brand-sub {
  position: relative;
  margin: 8px 0 0;
  font-size: 12px;
  letter-spacing: 0.22em;
  color: var(--text-secondary);
}

.brand-foot {
  position: relative;
  margin: 28px 0 0;
  padding-top: 14px;
  border-top: 1px solid var(--border-color);
  font-size: 11px;
  letter-spacing: 0.05em;
  color: var(--text-placeholder);
}

/* ---- 右侧表单区 ---- */
.form-pane {
  flex: 1;
  padding: 44px 40px;
}

.form-pane h2 {
  margin: 0 0 6px;
  font-family: var(--font-serif);
  font-size: 22px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--text-primary);
}

.subtitle {
  color: var(--text-secondary);
  font-size: 13px;
  margin: 0 0 26px;
}

.form-group { margin-bottom: 16px; }
.form-group label { display: block; margin-bottom: 6px; color: var(--text-regular); font-size: 13px; font-weight: 500; }
.form-group input {
  width: 100%; padding: 11px 14px; border: 1px solid var(--border-color-dark); border-radius: 8px;
  font-size: 14px; box-sizing: border-box; background: var(--bg-inset); color: var(--text-primary);
  transition: border-color 0.2s, background 0.2s, box-shadow 0.2s;
}
.form-group input::placeholder { color: var(--text-placeholder); }
.form-group input:focus {
  border-color: var(--primary-color); background: var(--bg-panel); outline: none;
  box-shadow: 0 0 0 3px rgba(232, 163, 61, 0.18);
}
.login-btn {
  width: 100%; padding: 12px; background: var(--primary-color); color: #241a08; border: none;
  border-radius: 8px; font-size: 15px; font-weight: 500; letter-spacing: 0.1em;
  cursor: pointer; margin-top: 8px;
  transition: background 0.2s;
}
.login-btn:hover { background: var(--primary-dark); }
.login-btn:disabled { background: #4d3f22; color: var(--text-placeholder); cursor: not-allowed; }
.error-msg { background: rgba(229, 72, 77, 0.12); color: #ea6f73; padding: 10px 12px; border-radius: 8px; margin-bottom: 16px; font-size: 13px; }

/* 窄屏：隐藏品牌区，单栏表单 */
@media (max-width: 720px) {
  .brand-pane { display: none; }
  .login-card { width: 400px; }
}
</style>
