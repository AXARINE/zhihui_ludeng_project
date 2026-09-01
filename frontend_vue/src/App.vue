<script setup>
/**
 * 根组件 — 侧边栏 + 顶栏 + 路由出口
 *
 * 角色与权限控制逻辑：
 * - 登录时后端返回 role（含 role_code）和 permissions（权限码数组），存入 localStorage
 * - 侧边栏菜单项根据用户权限动态显示/隐藏
 * - super_admin（系统管理员）拥有全部权限，不受权限数组限制
 *
 * 响应式说明：
 * - localStorage 变更不会触发 Vue 响应式更新
 * - 因此用 ref 存储用户信息，通过 watch(route) 在路由变化时重新读取
 * - 登录后 router.push 会触发路由变化，从而刷新侧边栏
 */
import { useRouter, useRoute } from 'vue-router'
import { ref, computed, watch } from 'vue'
import {
    Monitor, List, Bell, Setting, Document,
    User, ChatDotRound, Lock, UserFilled, MapLocation
} from '@element-plus/icons-vue'

const router = useRouter()
const route = useRoute()
const activeMenu = ref('/dashboard')

// ---- 从 localStorage 读取用户信息（响应式） ----
const currentUser = ref({})
const currentRole = ref({})
const userPermissions = ref([])

/**
 * 从 localStorage 重新读取用户数据
 * 登录后、路由切换时都会调用
 */
function refreshUserFromStorage() {
    try { currentUser.value = JSON.parse(localStorage.getItem('user') || '{}') } catch { currentUser.value = {} }
    try { currentRole.value = JSON.parse(localStorage.getItem('role') || '{}') } catch { currentRole.value = {} }
    try { userPermissions.value = JSON.parse(localStorage.getItem('permissions') || '[]') } catch { userPermissions.value = [] }
}

// 初始化读取
refreshUserFromStorage()

// 路由变化时重新读取（解决登录后侧边栏不刷新的问题）
watch(() => route.path, () => {
    refreshUserFromStorage()
    activeMenu.value = route.path
})

// ---- 通知中心（顶栏铃铛 + 未读红点 + 日报）----
import { getUnreadCount, getNotifications, markNotificationRead, getTodayReport } from '@/api/notify'
import { ElMessage } from 'element-plus'

const unread = ref(0)
const notifList = ref([])
const notifVisible = ref(false)
const reportVisible = ref(false)
const reportData = ref(null)
// 尚无日报(后端 404):弹窗内显示空态而非报错 toast
const reportEmpty = ref(false)

const hasToken = () => !!localStorage.getItem('token')

async function refreshUnread() {
    if (!hasToken()) return
    try { unread.value = (await getUnreadCount()).unread || 0 } catch { /* 忽略 */ }
}
async function loadNotifications() {
    if (!hasToken()) return
    try {
        notifList.value = (await getNotifications()) || []
        refreshUnread()
    } catch { /* 忽略 */ }
}
async function markRead(n) {
    if (n.is_read) return
    try { await markNotificationRead(n.id); n.is_read = true; refreshUnread() } catch { /* 忽略 */ }
}
async function openReport() {
    reportVisible.value = true
    reportData.value = null
    reportEmpty.value = false
    try {
        reportData.value = await getTodayReport()
    } catch (e) {
        if (e?.response?.status === 404) {
            // 日报尚未生成:保持弹窗打开,展示空态
            reportEmpty.value = true
        } else {
            reportVisible.value = false
            ElMessage.error('日报获取失败：' + (e?.response?.data || e?.message || e))
        }
    }
}
function fmtNotifTime(iso) {
    if (!iso) return ''
    const d = new Date(iso)
    const p = n => (n < 10 ? '0' : '') + n
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}
refreshUnread()
setInterval(refreshUnread, 30000)

// ---- 登录页面不显示侧边栏 ----
const showSidebar = computed(() => route.meta.public !== true)

// ---- 权限判断 ----
const isSuperAdmin = computed(() => currentRole.value.role_code === 'super_admin')

/**
 * 检查当前用户是否拥有某个权限码
 * super_admin 直接返回 true（拥有全部权限）
 */
function hasPerm(code) {
    if (isSuperAdmin.value) return true
    return userPermissions.value.includes(code)
}

// ---- 菜单路由跳转 ----
const handleMenuSelect = (index) => {
    activeMenu.value = index
    router.push(index)
}

// ---- 退出登录：清理所有 localStorage 数据 ----
function handleLogout() {
    localStorage.removeItem('token')
    localStorage.removeItem('user')
    localStorage.removeItem('role')
    localStorage.removeItem('permissions')
    refreshUserFromStorage()
    router.push('/login')
}
</script>

<template>
    <!-- 登录页面：全屏无侧边栏 -->
    <div v-if="!showSidebar">
        <router-view />
    </div>

    <!-- 主界面：侧边栏 + 内容 -->
    <div v-else class="app-container">
        <aside class="sidebar">
            <div class="logo">
                <h2>启晖智慧路灯</h2>
                <p>IoT 管理系统</p>
            </div>

            <el-menu :defaulst-active="activeMenu" @select="handleMenuSelect" class="sidebar-menu">
                <!-- 仪表盘 — 所有已登录用户可见 -->
                <el-menu-item index="/dashboard">
                    <el-icon>
                        <Monitor />
                    </el-icon>
                    <span>首页大屏</span>
                </el-menu-item>

                <!-- 设备列表 — 需要 device:status 权限 -->
                <el-menu-item v-if="hasPerm('device:status')" index="/devices">
                    <el-icon>
                        <List />
                    </el-icon>
                    <span>设备列表</span>
                </el-menu-item>

                <!-- 设备地图 — 需要 device:status 权限 -->
                <el-menu-item v-if="hasPerm('device:status')" index="/map">
                    <el-icon>
                        <MapLocation />
                    </el-icon>
                    <span>设备地图</span>
                </el-menu-item>

                <!-- 告警列表 — 需要 alarm:log 权限 -->
                <el-menu-item v-if="hasPerm('alarm:log')" index="/alarms">
                    <el-icon>
                        <Bell />
                    </el-icon>
                    <span>告警列表</span>
                </el-menu-item>

                <!-- 审计日志 — 需要 command:log 权限 -->
                <el-menu-item v-if="hasPerm('command:log')" index="/commands">
                    <el-icon>
                        <Document />
                    </el-icon>
                    <span>审计日志</span>
                </el-menu-item>

                <!-- 参数配置 — 需要 config:threshold 或 config:dimming 权限 -->
                <el-menu-item v-if="hasPerm('config:threshold') || hasPerm('config:dimming')" index="/config">
                    <el-icon>
                        <Setting />
                    </el-icon>
                    <span>参数配置</span>
                </el-menu-item>

                <!-- 智能问答 — 需要 assistant:qa 权限 -->
                <el-menu-item v-if="hasPerm('assistant:qa')" index="/assistant">
                    <el-icon>
                        <ChatDotRound />
                    </el-icon>
                    <span>智能问答</span>
                </el-menu-item>

                <!-- 账号管理 — 需要 user:manage 权限 -->
                <el-menu-item v-if="hasPerm('user:manage')" index="/users">
                    <el-icon>
                        <UserFilled />
                    </el-icon>
                    <span>账号管理</span>
                </el-menu-item>

                <!-- 权限管理 — 仅系统管理员（super_admin）可见 -->
                <el-menu-item v-if="isSuperAdmin" index="/permissions">
                    <el-icon>
                        <Lock />
                    </el-icon>
                    <span>权限管理</span>
                </el-menu-item>
            </el-menu>

            <!-- 底部用户信息 -->
            <div class="user-info">
                <div class="user-avatar">
                    {{ (currentUser.real_name || currentUser.username || '用').charAt(0) }}
                </div>
                <div class="user-meta">
                    <div class="user-name">{{ currentUser.real_name || currentUser.username || '未登录' }}</div>
                    <div class="user-role">{{ currentRole.role_name || '' }}</div>
                </div>
                <button class="logout-btn" @click="handleLogout">退出</button>
            </div>
        </aside>

        <main class="main-content">
            <!-- 通知铃铛 + 未读红点（所有已登录用户可见） -->
            <div class="content-top">
                <el-badge :value="unread" :hidden="unread === 0" :max="99" class="bell-wrap">
                    <el-popover placement="bottom-end" :width="330" trigger="click" v-model:visible="notifVisible" @show="loadNotifications">
                        <template #reference>
                            <button class="bell-btn"><el-icon :size="18"><Bell /></el-icon></button>
                        </template>
                        <div class="notif-panel">
                            <div class="notif-head">
                                <span>通知中心</span>
                                <button class="report-link" @click="openReport">📅 今日日报</button>
                            </div>
                            <div v-if="notifList.length === 0" class="notif-empty">暂无通知</div>
                            <div v-for="n in notifList" :key="n.id" class="notif-item" :class="{ 'is-unread': !n.is_read }" @click="markRead(n)">
                                <div class="notif-item-top">
                                    <span class="notif-tag" :class="n.type">{{ n.type === 'report' ? '日报' : '维修' }}</span>
                                    <span class="notif-title">{{ n.title }}</span>
                                    <span v-if="!n.is_read" class="notif-dot"></span>
                                </div>
                                <div class="notif-content">{{ n.content }}</div>
                                <div class="notif-time">{{ fmtNotifTime(n.created_at) }}</div>
                            </div>
                        </div>
                    </el-popover>
                </el-badge>
            </div>
            <router-view />
        </main>
    </div>

    <!-- 今日日报弹窗 -->
    <el-dialog v-model="reportVisible" title="📅 每日日报" width="540" append-to-body>
        <template v-if="reportData">
            <div class="report-grid">
                <div class="report-cell"><b>{{ reportData.content.devices_total }}</b><span>设备总数</span></div>
                <div class="report-cell"><b>{{ reportData.content.devices_online }}</b><span>在线设备</span></div>
                <div class="report-cell"><b>{{ reportData.content.lamp_on }}</b><span>亮灯数量</span></div>
                <div class="report-cell"><b>{{ reportData.content.alarms_today }}</b><span>今日告警</span></div>
                <div class="report-cell"><b>{{ reportData.content.alarms_unhandled }}</b><span>未处理告警</span></div>
                <div class="report-cell"><b>{{ reportData.content.avg_lux }}</b><span>平均光照(lux)</span></div>
                <div class="report-cell"><b>{{ reportData.content.reports_lux }}</b><span>光照上报次数</span></div>
                <div class="report-cell"><b>{{ reportData.content.cmd_manual }}</b><span>手动指令</span></div>
                <div class="report-cell"><b>{{ reportData.content.cmd_auto }}</b><span>自动指令</span></div>
            </div>
            <div class="report-foot">日报日期：{{ reportData.report_date }}（前一日数据）· 每天 09:00 自动更新</div>
        </template>
        <template v-else-if="reportEmpty">
            <div class="report-empty">
                <div class="report-empty-icon">📭</div>
                <div class="report-empty-title">暂无日报</div>
                <div class="report-empty-tip">日报在每天 09:00(北京时间)后生成前一日数据,请稍后再来</div>
            </div>
        </template>
    </el-dialog>
</template>

<style scoped>
.app-container {
    display: flex;
    min-height: 100vh;
}

/* ---- 深墨侧边栏 ---- */
.sidebar {
    width: 220px;
    background-color: #1c1b1a;
    color: #a8a29c;
    display: flex;
    flex-direction: column;
}

.logo {
    padding: 22px 20px 18px;
    text-align: center;
    border-bottom: 1px solid #2a2825;
}


.logo h2 {
    margin: 0;
    font-size: 20px;
    font-family: var(--font-serif);
    font-weight: 600;
    letter-spacing: 0.06em;
    color: #f5f3ee;
}

.logo p {
    margin: 6px 0 0 0;
    font-size: 11px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: #c96a4a;
}

/* 菜单：胶囊项，激活项赤陶浅底 */
.sidebar-menu {
    border-right: none;
    background-color: transparent;
    flex: 1;
    padding: 10px 8px;
    --el-menu-bg-color: transparent;
    --el-menu-text-color: #a8a29c;
    --el-menu-hover-bg-color: #282622;
    --el-menu-active-color: #e8a587;
    --el-menu-border-color: transparent;
}

.sidebar-menu .el-menu-item {
    height: 42px;
    line-height: 42px;
    border-radius: 8px;
    margin: 2px 0;
    transition: background-color 0.15s, color 0.15s;
}

.sidebar-menu .el-menu-item:hover {
    color: #e8e4dc;
}

.sidebar-menu .el-menu-item.is-active {
    background-color: rgba(201, 106, 74, 0.2);
    color: #e8a587;
    font-weight: 600;
}

/* ---- 底部用户卡片 ---- */
.user-info {
    margin: 12px;
    padding: 12px;
    background: #232120;
    border: 1px solid #2e2b28;
    border-radius: 10px;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: #8a8578;
}

.user-avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: #c96a4a;
    color: #fff7f2;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 600;
    flex-shrink: 0;
}

.user-meta {
    flex: 1;
    min-width: 0;
    line-height: 1.3;
}

.user-name {
    font-size: 13px;
    font-weight: 500;
    color: #e8e4dc;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.user-role {
    font-size: 11px;
    color: #7a746c;
}

.logout-btn {
    background: none;
    border: 1px solid #4a453f;
    color: #a8a29c;
    padding: 3px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
    flex-shrink: 0;
    transition: all 0.2s;
}

.logout-btn:hover {
    border-color: #be4b40;
    color: #d07c72;
}

/* ---- 暖纸内容区 ---- */
.main-content {
    flex: 1;
    background-color: #faf9f5;
    overflow-y: auto;
}

/* ---- 通知铃铛 ---- */
.content-top {
    position: fixed;
    top: 16px;
    right: 20px;
    z-index: 90;
}
.bell-btn {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    border: 1px solid #E8ECF1;
    background: #fff;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.08);
    transition: box-shadow 0.15s;
}
.bell-btn:hover { box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15); }
.notif-panel { max-height: 420px; overflow-y: auto; }
.notif-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    font-weight: 600;
    font-size: 14px;
}
.report-link { border: none; background: none; color: #2F6FED; cursor: pointer; font-size: 12px; }
.notif-item {
    padding: 8px 10px;
    border-radius: 8px;
    margin-bottom: 6px;
    background: #F7F9FC;
    cursor: pointer;
}
.notif-item.is-unread { background: #EAF1FE; }
.notif-item-top { display: flex; align-items: center; gap: 6px; }
.notif-tag { font-size: 11px; padding: 1px 8px; border-radius: 8px; color: #fff; flex-shrink: 0; }
.notif-tag.report { background: #2F6FED; }
.notif-tag.alert { background: #E5484D; }
.notif-title { font-size: 13px; font-weight: 500; flex: 1; }
.notif-dot { width: 7px; height: 7px; border-radius: 50%; background: #E5484D; flex-shrink: 0; }
.notif-content { font-size: 12px; color: #6B7280; margin: 4px 0; line-height: 1.5; }
.notif-time { font-size: 11px; color: #9AA3AF; }
.notif-empty { text-align: center; color: #9AA3AF; padding: 20px 0; font-size: 13px; }

/* ---- 日报弹窗 ---- */
.report-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
.report-cell { background: #F7F9FC; border-radius: 10px; padding: 12px; text-align: center; }
.report-cell b { display: block; font-size: 22px; color: #2F6FED; }
.report-cell span { font-size: 12px; color: #6B7280; }
.report-foot { margin-top: 14px; font-size: 12px; color: #9AA3AF; text-align: center; }
.report-empty { padding: 28px 0 18px; text-align: center; }
.report-empty-icon { font-size: 40px; }
.report-empty-title { margin-top: 10px; font-size: 15px; font-weight: 600; color: #4B5563; }
.report-empty-tip { margin-top: 8px; font-size: 12px; color: #9AA3AF; }
</style>
