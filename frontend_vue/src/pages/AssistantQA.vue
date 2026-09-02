<script setup>
import { ref, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { askAssistant } from '@/api/device.js'
import { ChatDotRound, Promotion, UserFilled, Cpu } from '@element-plus/icons-vue'

const router = useRouter()
const question = ref('')
const messages = ref([])
const loading = ref(false)
const chatBox = ref(null)

const quickQuestions = [
  '最近7天有哪些告警？需要处理吗？',
  '灯不亮怎么排查？',
  '调光不生效是什么原因？',
  '设备现在在线吗？',
  '路灯频繁开关怎么办？'
]

// 点击回答下方的设备标签，跳转设备详情页"锁定"问题设备
function goDevice(id) {
  router.push(`/device/${id}`)
}

async function handleSend() {
  const q = question.value.trim()
  if (!q || loading.value) return

  // 添加用户消息
  messages.value.push({ role: 'user', content: q })
  question.value = ''
  await scrollToBottom()

  loading.value = true
  try {
    const res = await askAssistant(q)
    // devices = 告警/状态涉及的设备，渲染成可点击标签
    messages.value.push({
      role: 'assistant',
      content: res.answer,
      devices: res.related_devices || []
    })
  } catch (e) {
    messages.value.push({ role: 'assistant', content: '请求失败，请检查网络连接。' })
  } finally {
    loading.value = false
    await scrollToBottom()
  }
}

function handleQuick(q) {
  question.value = q
  handleSend()
}

async function scrollToBottom() {
  await nextTick()
  if (chatBox.value) {
    chatBox.value.scrollTop = chatBox.value.scrollHeight
  }
}
</script>

<template>
  <div class="page">
    <h3>维护智能问答</h3>
    <p class="desc">基于知识库的路灯维护助手：告警查询、调修建议、点击设备标签一键锁定定位。</p>

    <!-- 聊天区域 -->
    <div class="chat-container" ref="chatBox">
      <!-- 欢迎消息（空态：垂直居中，填满聊天区） -->
      <div v-if="messages.length === 0" class="welcome">
        <div class="welcome-glyph">
          <el-icon :size="24"><ChatDotRound /></el-icon>
        </div>
        <p class="welcome-title">你好，我是路灯维护助手</p>
        <p class="welcome-sub">告警查询 · 调修建议 · 点击回答里的设备标签可锁定定位</p>
        <div class="quick-list">
          <button
            v-for="q in quickQuestions"
            :key="q"
            class="qchip"
            @click="handleQuick(q)"
          >
            {{ q }}
          </button>
        </div>
      </div>

      <!-- 消息列表 -->
      <div
        v-for="(msg, i) in messages"
        :key="i"
        :class="['message', msg.role === 'user' ? 'msg-user' : 'msg-bot']"
      >
        <div class="msg-avatar">
          <el-icon :size="15"><UserFilled v-if="msg.role === 'user'" /><Cpu v-else /></el-icon>
        </div>
        <div class="msg-bubble">
          <pre class="msg-text">{{ msg.content }}</pre>
          <!-- 查找锁定：回答涉及的设备，点击跳转设备详情 -->
          <div v-if="msg.devices && msg.devices.length" class="msg-devices">
            <span class="lock-label">点击锁定设备：</span>
            <el-tag
              v-for="d in msg.devices"
              :key="d"
              type="warning"
              size="small"
              class="lock-tag"
              @click="goDevice(d)"
            >
              {{ d }}
            </el-tag>
          </div>
        </div>
      </div>

      <!-- 加载中 -->
      <div v-if="loading" class="message msg-bot">
        <div class="msg-avatar"><el-icon :size="15"><Cpu /></el-icon></div>
        <div class="msg-bubble loading-bubble">
          <span class="dot-anim">思考中</span>
        </div>
      </div>
    </div>

    <!-- 输入区域 -->
    <div class="input-bar">
      <el-input
        v-model="question"
        placeholder="输入问题，如：最近有哪些告警？"
        @keyup.enter="handleSend"
        :disabled="loading"
        size="large"
      >
        <template #append>
          <el-button :icon="Promotion" @click="handleSend" :loading="loading" />
        </template>
      </el-input>
    </div>
  </div>
</template>

<style scoped>
.page {
  padding: 24px;
  max-width: 900px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  height: calc(100vh - 48px);
}

.page h3 {
  margin: 0 0 4px 0;
  font-size: 22px;
  font-family: var(--font-serif);
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--text-primary);
}

.desc {
  color: var(--text-secondary);
  font-size: 13px;
  margin: 0 0 16px 0;
}

.chat-container {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  background: var(--bg-panel);
  border-radius: 12px;
  border: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
}

/* 空态：垂直水平居中，撑满聊天区 */
.welcome {
  margin: auto;
  max-width: 560px;
  text-align: center;
  padding: 24px 0;
  color: var(--text-regular);
}

.welcome-glyph {
  width: 56px;
  height: 56px;
  margin: 0 auto 16px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--primary-light);
  background: var(--primary-tint);
  border: 1px solid rgba(232, 163, 61, 0.3);
  box-shadow: var(--glow-amber);
}

.welcome-title {
  font-family: var(--font-serif);
  font-size: 19px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--text-primary);
  margin: 0 0 8px;
}

.welcome-sub {
  font-size: 13px;
  color: var(--text-secondary);
  margin: 0 0 24px;
}

.quick-list {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  justify-content: center;
}

/* 快捷问题：幽灵片（透明底 + 发丝描边，hover 琥珀） */
.qchip {
  padding: 7px 16px;
  border: 1px solid var(--border-color-dark);
  border-radius: 8px;
  background: transparent;
  color: var(--text-regular);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}

.qchip:hover {
  border-color: var(--primary-color);
  color: var(--primary-light);
  background: var(--primary-tint);
}

.message {
  display: flex;
  gap: 10px;
  margin-bottom: 16px;
}

.msg-user {
  flex-direction: row-reverse;
}

.msg-avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary);
  background: var(--bg-inset);
  border: 1px solid var(--border-color);
  flex-shrink: 0;
}

.msg-user .msg-avatar {
  color: var(--primary-light);
  background: var(--primary-tint);
  border-color: rgba(232, 163, 61, 0.3);
}

.msg-bubble {
  max-width: 75%;
  padding: 12px 16px;
  border-radius: 12px;
  line-height: 1.6;
}

.msg-user .msg-bubble {
  background: var(--primary-color);
  color: #241a08;
  border-bottom-right-radius: 4px;
}

.msg-bot .msg-bubble {
  background: var(--bg-inset);
  border: 1px solid var(--border-color);
  color: var(--text-primary);
  border-bottom-left-radius: 4px;
}

.msg-text {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 14px;
}

.loading-bubble {
  color: var(--text-secondary);
}

.msg-devices {
  margin-top: 8px;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.lock-label {
  font-size: 12px;
  color: var(--text-secondary);
}

.lock-tag {
  cursor: pointer;
}

.dot-anim::after {
  content: '';
  animation: dots 1.5s infinite;
}

@keyframes dots {
  0%, 20% { content: '.'; }
  40% { content: '..'; }
  60%, 100% { content: '...'; }
}

.input-bar {
  margin-top: 12px;
}
</style>
