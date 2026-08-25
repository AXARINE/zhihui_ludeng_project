<script setup>
import { ref } from 'vue'

// 创建一个开关变量，true = 开，false = 关
const isLightOn = ref(true)

// 切换开关的函数
const toggleLight = () => {
  isLightOn.value = !isLightOn.value  // 取反：true变false，false变true
}
</script>

<template>
  <div class="light-switch">
    <h3>灯光控制</h3>

    <button @click="toggleLight">
      {{ isLightOn ? '关灯' : '开灯' }}
    </button>

    <!-- v-if 的意思是：如果 isLightOn 是 true，就显示这个 div -->
    <div v-if="isLightOn" class="light-on">
      💡 灯是亮的！
    </div>

    <!-- v-else 的意思是：否则（isLightOn 是 false），就显示这个 div -->
    <div v-else class="light-off">
      🌑 灯是灭的！
    </div>
  </div>
</template>

<style scoped>
.light-switch {
  background-color: #fff3e0;
  padding: 20px;
  border-radius: 8px;
  text-align: center;
  margin: 20px 0;
}

button {
  background-color: #ff9800;
  color: white;
  border: none;
  padding: 10px 20px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 16px;
  margin-bottom: 15px;
}

button:hover {
  background-color: #f57c00;
}

.light-on {
  background-color: #fff9c4;
  padding: 15px;
  border-radius: 4px;
  font-size: 18px;
}

.light-off {
  background-color: #263238;
  color: white;
  padding: 15px;
  border-radius: 4px;
  font-size: 18px;
}
</style>
