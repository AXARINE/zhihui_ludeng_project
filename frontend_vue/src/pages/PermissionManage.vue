<script setup>
/**
 * 权限管理页面
 *
 * 功能：
 * 1. 左侧显示角色列表，点击选中某个角色
 * 2. 右侧显示该角色当前拥有的权限（按 module 分组的 checkbox）
 * 3. 可以勾选/取消权限并保存
 * 4. 系统管理员角色（super_admin）的权限不可修改
 *
 * 需要的权限：role:manage（只有系统管理员才有）
 */
import { ref, onMounted, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { getRoles, getPermissions, getRolePermissions, updateRolePermissions } from '@/api/device'

// ---- 状态 ----
const roles = ref([])            // 所有角色
const allPermissions = ref([])   // 所有权限（从后端获取）
const selectedRoleId = ref(null) // 当前选中的角色 ID
const selectedRole = ref(null)   // 当前选中的角色对象
const checkedPermIds = ref([])   // 当前勾选的权限 ID 列表
const loading = ref(false)       // 加载状态
const saving = ref(false)        // 保存状态

// ---- 初始化：加载角色列表和所有权限 ----
onMounted(async () => {
  loading.value = true
  try {
    // 并行请求角色列表和权限列表
    const [rolesRes, permsRes] = await Promise.all([
      getRoles(),
      getPermissions()
    ])
    roles.value = rolesRes
    allPermissions.value = permsRes
  } catch (e) {
    ElMessage.error('加载数据失败：' + (e?.response?.data || e.message))
  } finally {
    loading.value = false
  }
})

// ---- 点击角色行 ----
async function selectRole(role) {
  selectedRoleId.value = role.id
  selectedRole.value = role
  // 获取该角色当前拥有的权限 ID 列表
  try {
    const permIds = await getRolePermissions(role.id)
    checkedPermIds.value = permIds || []
  } catch (e) {
    ElMessage.error('获取角色权限失败：' + (e?.response?.data || e.message))
    checkedPermIds.value = []
  }
}

// ---- 按 module 分组的权限列表 ----
// computed 在 script setup 中直接用函数实现
function getGroupedPermissions() {
  const groups = {}
  for (const perm of allPermissions.value) {
    const module = perm.module || '其他'
    if (!groups[module]) groups[module] = []
    groups[module].push(perm)
  }
  return groups
}

// ---- 是否不可修改（系统管理员角色） ----
const isImmutable = ref(false)
watch(selectedRole, (role) => {
  isImmutable.value = role?.role_code === 'super_admin'
})

// ---- 保存权限 ----
async function handleSave() {
  if (!selectedRoleId.value) return
  saving.value = true
  try {
    await updateRolePermissions(selectedRoleId.value, checkedPermIds.value)
    ElMessage.success('权限保存成功')
  } catch (e) {
    const msg = e?.response?.data || e.message
    ElMessage.error('保存失败：' + msg)
  } finally {
    saving.value = false
  }
}

// ---- checkbox 变化处理 ----
function handleCheckChange(permId, checked) {
  if (checked) {
    if (!checkedPermIds.value.includes(permId)) {
      checkedPermIds.value.push(permId)
    }
  } else {
    checkedPermIds.value = checkedPermIds.value.filter(id => id !== permId)
  }
}
</script>

<template>
  <div class="perm-page">
    <div class="page-header">
      <h2>权限管理</h2>
      <p class="desc">管理各角色的功能权限（仅系统管理员可操作）</p>
    </div>

    <div class="perm-content" v-loading="loading">
      <!-- 左侧：角色列表 -->
      <div class="role-panel">
        <div class="panel-title">角色列表</div>
        <div class="role-list">
          <div
            v-for="role in roles"
            :key="role.id"
            class="role-item"
            :class="{ active: selectedRoleId === role.id }"
            @click="selectRole(role)"
          >
            <div class="role-name">{{ role.role_name }}</div>
            <div class="role-code">{{ role.role_code }}</div>
            <div class="role-desc" v-if="role.description">{{ role.description }}</div>
          </div>
          <div v-if="!roles.length && !loading" class="empty-hint">暂无角色数据</div>
        </div>
      </div>

      <!-- 右侧：权限配置 -->
      <div class="perm-panel">
        <template v-if="selectedRole">
          <div class="panel-title">
            <span>{{ selectedRole.role_name }} — 权限配置</span>
            <el-tag v-if="isImmutable" type="warning" size="small">不可修改</el-tag>
          </div>

          <!-- 不可修改提示 -->
          <el-alert
            v-if="isImmutable"
            title="系统管理员角色的权限固定为全部权限，不可修改"
            type="warning"
            :closable="false"
            show-icon
            style="margin-bottom: 16px"
          />

          <!-- 按 module 分组的权限 checkbox -->
          <div class="perm-groups">
            <div
              v-for="(perms, module) in getGroupedPermissions()"
              :key="module"
              class="perm-group"
            >
              <div class="group-title">{{ module }}</div>
              <div class="group-items">
                <el-checkbox
                  v-for="perm in perms"
                  :key="perm.id"
                  :model-value="checkedPermIds.includes(perm.id)"
                  :disabled="isImmutable"
                  @change="(val) => handleCheckChange(perm.id, val)"
                >
                  <span class="perm-name">{{ perm.perm_name }}</span>
                  <span class="perm-code">({{ perm.perm_code }})</span>
                </el-checkbox>
              </div>
            </div>
          </div>

          <!-- 保存按钮 -->
          <div class="save-bar" v-if="!isImmutable">
            <el-button type="primary" :loading="saving" @click="handleSave">
              保存权限
            </el-button>
            <span class="save-hint">修改后需重新登录才能生效</span>
          </div>
        </template>

        <template v-else>
          <div class="no-selection">
            <p>← 请从左侧选择一个角色</p>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.perm-page {
  padding: 24px;
}

.page-header {
  margin-bottom: 24px;
  padding-bottom: 14px;
  border-bottom: 1px solid #efebe3;
}

.page-header h2 {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 0 4px 0;
  font-size: 24px;
  font-family: var(--font-serif);
  font-weight: 600;
  color: #1f1c19;
}

.page-header h2::before {
  content: '';
  width: 4px;
  height: 0.95em;
  background: #c96a4a;
  border-radius: 2px;
}

.page-header .desc {
  margin: 0;
  font-size: 14px;
  color: #8a837b;
}

.perm-content {
  display: flex;
  gap: 20px;
  align-items: flex-start;
}

/* 左侧角色列表 */
.role-panel {
  width: 280px;
  flex-shrink: 0;
  background: #fff;
  border-radius: 10px;
  border: 1px solid #e8e4dc;
  overflow: hidden;
}

.panel-title {
  padding: 14px 16px;
  font-size: 15px;
  font-weight: 600;
  color: #1f1c19;
  background: #f5f2ec;
  border-bottom: 1px solid #e8e4dc;
  display: flex;
  align-items: center;
  gap: 8px;
}

.role-list {
  max-height: 600px;
  overflow-y: auto;
}

.role-item {
  padding: 14px 16px;
  cursor: pointer;
  border-bottom: 1px solid #f5f2ec;
  border-left: 3px solid transparent;
  transition: background 0.15s;
}

.role-item:hover {
  background: #faf8f3;
}

.role-item.active {
  background: #faede7;
  border-left: 3px solid #c96a4a;
}

.role-name {
  font-size: 14px;
  font-weight: 600;
  color: #1f1c19;
}

.role-code {
  font-size: 12px;
  font-family: var(--font-mono);
  color: #8a837b;
  margin-top: 2px;
}

.role-desc {
  font-size: 12px;
  color: #b4ada3;
  margin-top: 4px;
}

.empty-hint {
  padding: 40px 16px;
  text-align: center;
  color: #b4ada3;
  font-size: 14px;
}

/* 右侧权限配置 */
.perm-panel {
  flex: 1;
  background: #fff;
  border-radius: 10px;
  border: 1px solid #e8e4dc;
  padding: 20px;
  min-height: 400px;
}

.perm-groups {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.perm-group {
  border: 1px solid #efebe3;
  border-radius: 8px;
  overflow: hidden;
}

.group-title {
  padding: 10px 14px;
  font-size: 13px;
  font-weight: 600;
  color: #57504a;
  background: #faf8f3;
  border-bottom: 1px solid #efebe3;
}

.group-items {
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.perm-name {
  font-size: 13px;
  color: #1f1c19;
}

.perm-code {
  font-size: 12px;
  font-family: var(--font-mono);
  color: #8a837b;
  margin-left: 4px;
}

.save-bar {
  margin-top: 24px;
  display: flex;
  align-items: center;
  gap: 12px;
}

.save-hint {
  font-size: 12px;
  color: #8a837b;
}

.no-selection {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 300px;
  color: #b4ada3;
  font-size: 16px;
}
</style>
