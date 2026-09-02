<script setup>
import { ref } from 'vue'

// 创建一个数字变量，初始值是 0
const count = ref(0)

// 创建一个函数，按按钮时执行
const addOne = () => {
  count.value = count.value + 1  // 数字加 1
}
</script>

<template>
  <div class="counter">
    <p>你点击了 {{ count }} 次</p>
    <button @click="addOne">点我加 1</button>
  </div>
</template>

<style scoped>
.counter {
  background-color: var(--bg-panel);
  padding: 20px;
  border-radius: 8px;
  text-align: center;
  margin: 20px 0;
}

button {
  background-color: var(--night-cyan);
  color: #0c1220;
  border: none;
  padding: 10px 20px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 16px;
}

button:hover {
  background-color: var(--night-cyan-dim);
}
</style>
