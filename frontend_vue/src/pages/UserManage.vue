<script setup>
/**
 * 账号管理页面
 *
 * 功能：
 * 1. 展示所有用户账号列表（用户名、姓名、角色、状态）
 * 2. 新增账号（输入用户名、密码、姓名、选择角色）
 * 3. 编辑账号（修改姓名、密码、角色、状态）
 * 4. 删除账号
 *
 * 需要权限：user:manage
 */
import { ref, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { getUsers, createUser, deleteUser, updateUser, getRoles } from '@/api/device'

// ---- 状态 ----
const users = ref([])
const roles = ref([])
const loading = ref(false)
const creating = ref(false)

// 新增账号表单
const newForm = ref({
  username: '',
  password: '',
  real_name: '',
  role_id: null
})

// 编辑对话框
const editVisible = ref(false)
const editLoading = ref(false)
const editForm = ref({
  id: null,
  username: '',
  real_name: '',
  password: '',
  role_id: null,
  status: 1
})

// ---- 加载数据 ----
async function loadUsers() {
  loading.value = true
  try {
    users.value = await getUsers()
  } catch (e) {
    ElMessage.error('加载账号列表失败：' + (e?.response?.data || e.message))
  } finally {
    loading.value = false
  }
}

async function loadRoles() {
  try {
    roles.value = await getRoles()
    if (roles.value.length && !newForm.value.role_id) {
      newForm.value.role_id = roles.value[0].id
    }
  } catch (e) {
    ElMessage.error('加载角色列表失败：' + (e?.response?.data || e.message))
  }
}

onMounted(() => {
  loadUsers()
  loadRoles()
})

// ---- 新增账号 ----
async function handleCreate() {
  if (!newForm.value.username.trim()) {
    ElMessage.warning('请输入用户名')
    return
  }
  if (!newForm.value.password) {
    ElMessage.warning('请输入密码')
    return
  }
  creating.value = true
  try {
    await createUser({
      username: newForm.value.username.trim(),
      password: newForm.value.password,
      real_name: newForm.value.real_name.trim() || newForm.value.username.trim(),
      role_id: newForm.value.role_id
    })
    ElMessage.success('账号已创建')
    newForm.value.username = ''
    newForm.value.password = ''
    newForm.value.real_name = ''
    loadUsers()
  } catch (e) {
    ElMessage.error('创建失败：' + (e?.response?.data || e.message))
  } finally {
    creating.value = false
  }
}

// ---- 编辑账号 ----
function handleEdit(user) {
  editForm.value = {
    id: user.id,
    username: user.username,
    real_name: user.real_name || '',
    password: '',
    role_id: user.role_id,
    status: user.status
  }
  editVisible.value = true
}

async function handleUpdate() {
  editLoading.value = true
  try {
    const data = {}
    // 只发送有变更的字段
    if (editForm.value.username.trim()) data.username = editForm.value.username.trim()
    if (editForm.value.real_name.trim()) data.real_name = editForm.value.real_name.trim()
    if (editForm.value.password) data.password = editForm.value.password
    if (editForm.value.role_id) data.role_id = editForm.value.role_id
    if (editForm.value.status !== undefined) data.status = editForm.value.status

    if (Object.keys(data).length === 0) {
      ElMessage.warning('没有可更新的字段')
      return
    }

    await updateUser(editForm.value.id, data)
    ElMessage.success('账号已更新')
    editVisible.value = false
    loadUsers()
  } catch (e) {
    ElMessage.error('更新失败：' + (e?.response?.data || e.message))
  } finally {
    editLoading.value = false
  }
}

// ---- 删除账号 ----
async function handleDelete(user) {
  try {
    await ElMessageBox.confirm(
      `确定删除账号「${user.username}」吗？此操作不可撤销。`,
      '删除确认',
      { type: 'warning', confirmButtonText: '确定删除', cancelButtonText: '取消' }
    )
    await deleteUser(user.id)
    ElMessage.success('账号已删除')
    loadUsers()
  } catch (e) {
    if (e !== 'cancel') {
      ElMessage.error('删除失败：' + (e?.response?.data || e.message))
    }
  }
}

// ---- 角色名称映射 ----
function getRoleName(roleCode) {
  const map = { super_admin: '系统管理员', admin: '路灯管理员', municipal: '市政人员' }
  return map[roleCode] || roleCode
}

function getRoleTagType(roleCode) {
  const map = { super_admin: 'danger', admin: 'warning', municipal: 'info' }
  return map[roleCode] || 'info'
}
</script>

<template>
  <div class="user-page">
    <div class="page-header">
      <h2>账号管理</h2>
      <p class="desc">管理系统用户账号（需要 user:manage 权限）</p>
    </div>

    <!-- 新增账号表单 -->
    <el-card class="form-card" shadow="never">
      <template #header>
        <span class="card-title">新增账号</span>
      </template>
      <div class="create-form">
        <el-input
          v-model="newForm.username"
          placeholder="用户名"
          clearable
          style="width: 160px"
          @keyup.enter="handleCreate"
        />
        <el-input
          v-model="newForm.password"
          type="password"
          placeholder="密码"
          show-password
          style="width: 160px"
          @keyup.enter="handleCreate"
        />
        <el-input
          v-model="newForm.real_name"
          placeholder="姓名（可选）"
          clearable
          style="width: 160px"
          @keyup.enter="handleCreate"
        />
        <el-select v-model="newForm.role_id" placeholder="选择角色" style="width: 160px">
          <el-option
            v-for="role in roles"
            :key="role.id"
            :label="role.role_name"
            :value="role.id"
          />
        </el-select>
        <el-button type="primary" :loading="creating" @click="handleCreate">
          新增账号
        </el-button>
      </div>
    </el-card>

    <!-- 账号列表 -->
    <el-card shadow="never" style="margin-top: 16px">
      <template #header>
        <span class="card-title">账号列表</span>
      </template>
      <el-table :data="users" v-loading="loading" stripe style="width: 100%">
        <el-table-column prop="id" label="ID" width="60" />
        <el-table-column prop="username" label="用户名" min-width="120" />
        <el-table-column prop="real_name" label="姓名" min-width="120">
          <template #default="{ row }">
            {{ row.real_name || '-' }}
          </template>
        </el-table-column>
        <el-table-column prop="role_name" label="角色" min-width="120">
          <template #default="{ row }">
            <el-tag :type="getRoleTagType(row.role_code)" size="small">
              {{ row.role_name || '-' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="80">
          <template #default="{ row }">
            <el-tag :type="row.status === 1 ? 'success' : 'info'" size="small">
              {{ row.status === 1 ? '启用' : '停用' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="160" fixed="right">
          <template #default="{ row }">
            <el-button type="primary" size="small" text @click="handleEdit(row)">
              编辑
            </el-button>
            <el-button type="danger" size="small" text @click="handleDelete(row)">
              删除
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 编辑账号对话框 -->
    <el-dialog v-model="editVisible" title="编辑账号" width="420px">
      <el-form label-width="80px">
        <el-form-item label="用户名">
          <el-input v-model="editForm.username" placeholder="请输入用户名" />
        </el-form-item>
        <el-form-item label="姓名">
          <el-input v-model="editForm.real_name" placeholder="请输入姓名" />
        </el-form-item>
        <el-form-item label="新密码">
          <el-input v-model="editForm.password" type="password" show-password placeholder="留空则不修改" />
        </el-form-item>
        <el-form-item label="角色">
          <el-select v-model="editForm.role_id" style="width: 100%">
            <el-option
              v-for="role in roles"
              :key="role.id"
              :label="role.role_name"
              :value="role.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item label="状态">
          <el-switch
            v-model="editForm.status"
            :active-value="1"
            :inactive-value="0"
            active-text="启用"
            inactive-text="停用"
          />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="editVisible = false">取消</el-button>
        <el-button type="primary" :loading="editLoading" @click="handleUpdate">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.user-page {
  padding: 24px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0 0 4px 0;
  font-size: 20px;
  color: #303133;
}

.page-header .desc {
  margin: 0;
  font-size: 14px;
  color: #909399;
}

.card-title {
  font-weight: 600;
  font-size: 15px;
}

.create-form {
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
}
</style>
