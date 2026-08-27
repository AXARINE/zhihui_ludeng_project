<script setup>
import { ref, nextTick } from 'vue'
import { askAssistant } from '@/api/device.js'
import { ChatDotRound, Promotion } from '@element-plus/icons-vue'

const question = ref('')
const messages = ref([])
const loading = ref(false)
const chatBox = ref(null)

const quickQuestions = [
  '最近7天有哪些告警？',
  '设备现在在线吗？',
  '光照阈值是多少？',
  '最近的光照数据怎么样？',
  '路灯频繁开关怎么办？'
]

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
    messages.value.push({ role: 'assistant', content: res.answer })
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
    <h3>🤖 维护智能问答</h3>
    <p class="desc">基于知识库的路灯维护助手，支持告警查询、设备状态、维护建议等。</p>

    <!-- 聊天区域 -->
    <div class="chat-container" ref="chatBox">
      <!-- 欢迎消息 -->
      <div v-if="messages.length === 0" class="welcome">
        <p>👋 你好！我是路灯维护助手，你可以问我：</p>
        <div class="quick-list">
          <el-button
            v-for="q in quickQuestions"
            :key="q"
            size="small"
            type="primary"
            plain
            @click="handleQuick(q)"
          >
            {{ q }}
          </el-button>
        </div>
      </div>

      <!-- 消息列表 -->
      <div
        v-for="(msg, i) in messages"
        :key="i"
        :class="['message', msg.role === 'user' ? 'msg-user' : 'msg-bot']"
      >
        <div class="msg-avatar">
          {{ msg.role === 'user' ? '👤' : '🤖' }}
        </div>
        <div class="msg-bubble">
          <pre class="msg-text">{{ msg.content }}</pre>
        </div>
      </div>

      <!-- 加载中 -->
      <div v-if="loading" class="message msg-bot">
        <div class="msg-avatar">🤖</div>
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
  color: #1f1c19;
}

.desc {
  color: #8a837b;
  font-size: 13px;
  margin: 0 0 16px 0;
}

.chat-container {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  background: #fff;
  border-radius: 10px;
  border: 1px solid #e8e4dc;
}

.welcome {
  text-align: center;
  padding: 40px 0;
  color: #57504a;
}

.quick-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: center;
  margin-top: 16px;
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
  font-size: 16px;
  background: #f5f2ec;
  border: 1px solid #e8e4dc;
  flex-shrink: 0;
}

.msg-user .msg-avatar {
  background: #faede7;
  border-color: #f5ded4;
}

.msg-bubble {
  max-width: 75%;
  padding: 12px 16px;
  border-radius: 14px;
  line-height: 1.6;
}

.msg-user .msg-bubble {
  background: #c96a4a;
  color: #fff7f2;
  border-bottom-right-radius: 4px;
}

.msg-bot .msg-bubble {
  background: #f5f2ec;
  border: 1px solid #efebe3;
  color: #1f1c19;
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
  color: #8a837b;
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
