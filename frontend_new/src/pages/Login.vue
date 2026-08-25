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
      <h2>智慧路灯管理系统</h2>
      <p class="subtitle">请登录</p>
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
</template>

<style scoped>
.login-page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
}
.login-card {
  background: #fff;
  padding: 40px;
  border-radius: 12px;
  width: 360px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.2);
}
.login-card h2 { text-align: center; margin: 0 0 8px; color: #333; }
.subtitle { text-align: center; color: #999; margin: 0 0 24px; }
.form-group { margin-bottom: 16px; }
.form-group label { display: block; margin-bottom: 6px; color: #555; font-size: 14px; }
.form-group input {
  width: 100%; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px;
  font-size: 14px; box-sizing: border-box;
}
.form-group input:focus { border-color: #409eff; outline: none; }
.login-btn {
  width: 100%; padding: 12px; background: #409eff; color: #fff; border: none;
  border-radius: 6px; font-size: 16px; cursor: pointer; margin-top: 8px;
}
.login-btn:hover { background: #66b1ff; }
.login-btn:disabled { background: #a0cfff; cursor: not-allowed; }
.error-msg { background: #fef0f0; color: #f56c6c; padding: 10px; border-radius: 6px; margin-bottom: 16px; font-size: 14px; }
</style>
