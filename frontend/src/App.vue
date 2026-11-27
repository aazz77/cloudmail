<template>
  <template v-if="route.meta.public">
    <router-view />
  </template>
  <div v-else class="cm-shell">
    <!-- 桌面侧边栏 -->
    <aside class="sidebar" :class="{ open: drawerOpen }">
      <div class="brand" @click="go('/inbox')">
        <div class="logo"><el-icon><Message /></el-icon></div>
        <div class="brand-text">
          <strong>Cloud Mail</strong>
          <span>云端邮箱</span>
        </div>
        <button class="drawer-close" @click.stop="drawerOpen=false" aria-label="关闭"><el-icon><Close /></el-icon></button>
      </div>

      <nav class="nav">
        <div class="nav-group" v-for="g in menus" :key="g.label">
          <div class="nav-label" v-if="g.label">{{ g.label }}</div>
          <router-link
            v-for="m in g.items" :key="m.path" :to="m.path"
            class="nav-item" :class="{ active: route.path === m.path }"
            @click="drawerOpen=false"
          >
            <el-icon><component :is="m.icon" /></el-icon>
            <span>{{ m.title }}</span>
            <span class="nav-badge" v-if="m.badge">{{ m.badge }}</span>
          </router-link>
        </div>
      </nav>

      <div class="sidebar-foot">
        <div class="user-mini">
          <div class="avatar">{{ (me.email || '?')[0].toUpperCase() }}</div>
          <div class="user-meta">
            <strong>{{ me.name || (me.email || '').split('@')[0] || '用户' }}</strong>
            <span>{{ me.email || '' }}</span>
          </div>
          <el-icon class="logout-ico" @click="logout"><SwitchButton /></el-icon>
        </div>
      </div>
    </aside>

    <!-- 遮罩 -->
    <div v-if="drawerOpen" class="mask" @click="drawerOpen=false"></div>

    <!-- 主内容 -->
    <div class="main">
      <header class="topbar">
        <button class="menu-btn" @click="drawerOpen=true" aria-label="菜单"><el-icon><Menu /></el-icon></button>
        <div class="crumb">{{ route.meta.title || '收件箱' }}</div>
        <div class="top-actions">
          <el-tooltip content="写邮件" placement="bottom">
            <button class="icon-btn" @click="go('/send')"><el-icon><EditPen /></el-icon></button>
          </el-tooltip>
          <el-tooltip content="个人中心" placement="bottom">
            <button class="icon-btn" @click="go('/profile')"><el-icon><User /></el-icon></button>
          </el-tooltip>
        </div>
      </header>
      <main class="content">
        <router-view />
      </main>
    </div>

    <!-- 移动底部 Tab -->
    <nav class="tabbar">
      <router-link to="/inbox" class="tab" :class="{active: route.path==='/inbox'}">
        <el-icon><Message /></el-icon><span>收信</span>
      </router-link>
      <router-link to="/send" class="tab" :class="{active: route.path==='/send'}">
        <el-icon><EditPen /></el-icon><span>写信</span>
      </router-link>
      <router-link to="/star" class="tab" :class="{active: route.path==='/star'}">
        <el-icon><Star /></el-icon><span>星标</span>
      </router-link>
      <router-link to="/accounts" class="tab" :class="{active: route.path==='/accounts'}">
        <el-icon><FolderOpened /></el-icon><span>账号</span>
      </router-link>
      <button class="tab" @click="drawerOpen=true">
        <el-icon><Menu /></el-icon><span>更多</span>
      </button>
    </nav>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ElMessageBox } from 'element-plus';
import { api, getToken, setToken } from './api';

const route = useRoute();
const router = useRouter();
const drawerOpen = ref(false);
const me = ref({});

const loadMe = async () => {
  try { me.value = await api.get('/api/my/loginUserInfo'); }
  catch (e) { /* 401 已在 api 层处理跳转 */ }
};
onMounted(loadMe);
watch(() => route.path, () => { if (getToken()) loadMe(); });

const isAdmin = computed(() => (me.value.permKeys || []).includes('*'));
const has = (key) => isAdmin.value || (me.value.permKeys || []).includes(key);

// 不显示空功能：只列后端真实存在的路由，且按权限过滤
const menus = computed(() => {
  const groups = [
    {
      label: '邮件', items: [
        { path: '/inbox', title: '收件箱', icon: 'Message' },
        { path: '/sent', title: '发件箱', icon: 'Promotion' },
        { path: '/star', title: '星标邮件', icon: 'Star' },
        { path: '/send', title: '写邮件', icon: 'EditPen' },
      ],
    },
    { label: '管理', items: [] },
  ];
  const mg = groups[1].items;
  if (has('mail')) mg.push({ path: '/all', title: '全部邮件', icon: 'Files' });
  mg.push({ path: '/accounts', title: '邮箱账号', icon: 'FolderOpened' });
  if (has('analysis')) mg.push({ path: '/analysis', title: '数据分析', icon: 'TrendCharts' });
  if (has('user')) mg.push({ path: '/users', title: '用户管理', icon: 'UserFilled' });
  if (has('role')) mg.push({ path: '/roles', title: '角色权限', icon: 'Lock' });
  if (has('regkey')) mg.push({ path: '/regkeys', title: '注册码', icon: 'Key' });
  if (has('setting')) mg.push({ path: '/settings', title: '系统设置', icon: 'Setting' });
  if (!mg.length) groups.splice(1, 1);
  groups.push({ label: '账户', items: [{ path: '/profile', title: '个人中心', icon: 'User' }] });
  return groups;
});

const go = (p) => router.push(p);
const logout = async () => {
  try { await ElMessageBox.confirm('确定退出登录？', '提示', { type: 'warning' }); }
  catch { return; }
  try { await api.del('/api/logout'); } catch {}
  setToken('');
  router.push('/login');
};
</script>

<style scoped>
.cm-shell { min-height: 100vh; display: flex; }
.sidebar {
  width: var(--cm-sidebar-w); flex: 0 0 var(--cm-sidebar-w);
  background: var(--cm-surface); border-right: 1px solid var(--cm-border);
  display: flex; flex-direction: column; position: sticky; top: 0; height: 100vh;
  z-index: 30;
}
.brand { height: var(--cm-header-h); display: flex; align-items: center; gap: 11px; padding: 0 20px; cursor: pointer; }
.brand .logo {
  width: 38px; height: 38px; border-radius: 12px; flex: 0 0 auto;
  background: linear-gradient(135deg, #4f6ef7, #8b5cf6); color: #fff;
  display: grid; place-items: center; font-size: 20px;
  box-shadow: 0 6px 16px rgba(79,110,247,.3);
}
.brand-text { display: flex; flex-direction: column; line-height: 1.25; }
.brand-text strong { font-size: 15px; font-weight: 750; }
.brand-text span { font-size: 11px; color: var(--cm-text-3); }
.drawer-close { display: none; margin-left: auto; background: none; border: none; font-size: 20px; color: var(--cm-text-2); cursor: pointer; }

.nav { flex: 1; overflow-y: auto; padding: 10px 12px 20px; }
.nav-group { margin-bottom: 14px; }
.nav-label { font-size: 11px; color: var(--cm-text-3); padding: 8px 12px 6px; letter-spacing: .5px; font-weight: 600; }
.nav-item {
  display: flex; align-items: center; gap: 11px; padding: 11px 12px;
  border-radius: 11px; color: var(--cm-text-2); font-size: 14px; font-weight: 500;
  margin-bottom: 3px; transition: all .16s;
}
.nav-item:hover { background: var(--cm-surface-soft); color: var(--cm-text); }
.nav-item.active { background: var(--cm-primary-soft); color: var(--cm-primary); font-weight: 650; }
.nav-item .el-icon { font-size: 18px; }
.nav-badge { margin-left: auto; background: var(--cm-primary); color: #fff; border-radius: 10px; font-size: 11px; padding: 1px 7px; }

.sidebar-foot { padding: 12px; border-top: 1px solid var(--cm-border); }
.user-mini { display: flex; align-items: center; gap: 10px; padding: 8px; border-radius: 12px; }
.user-mini .avatar {
  width: 36px; height: 36px; border-radius: 11px; flex: 0 0 auto;
  background: linear-gradient(135deg, #22c55e, #10b981); color: #fff;
  display: grid; place-items: center; font-weight: 700;
}
.user-meta { display: flex; flex-direction: column; min-width: 0; line-height: 1.3; }
.user-meta strong { font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.user-meta span { font-size: 11px; color: var(--cm-text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.logout-ico { margin-left: auto; color: var(--cm-text-3); cursor: pointer; font-size: 17px; }
.logout-ico:hover { color: var(--cm-danger); }

.main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.topbar {
  height: var(--cm-header-h); background: rgba(255,255,255,.85); backdrop-filter: blur(12px);
  border-bottom: 1px solid var(--cm-border); display: flex; align-items: center;
  padding: 0 28px; position: sticky; top: 0; z-index: 20;
}
.menu-btn { display: none; background: none; border: none; font-size: 22px; cursor: pointer; color: var(--cm-text); }
.crumb { font-size: 15px; font-weight: 650; }
.top-actions { margin-left: auto; display: flex; gap: 8px; }
.icon-btn {
  width: 38px; height: 38px; border-radius: 11px; border: 1px solid var(--cm-border);
  background: #fff; color: var(--cm-text-2); cursor: pointer; font-size: 17px;
  display: grid; place-items: center; transition: all .16s;
}
.icon-btn:hover { color: var(--cm-primary); border-color: #c7d2fe; background: var(--cm-primary-soft); }
.content { flex: 1; }

.tabbar { display: none; }
.mask { display: none; }

@media (max-width: 900px) {
  .sidebar {
    position: fixed; left: 0; top: 0; bottom: 0; transform: translateX(-100%);
    transition: transform .24s ease; box-shadow: 8px 0 40px rgba(0,0,0,.1);
  }
  .sidebar.open { transform: translateX(0); }
  .drawer-close { display: block; }
  .mask { display: block; position: fixed; inset: 0; background: rgba(15,23,42,.4); z-index: 25; }
  .menu-btn { display: block; margin-right: 14px; }
  .topbar { padding: 0 16px; }
  .tabbar {
    display: flex; position: fixed; left: 0; right: 0; bottom: 0; z-index: 20;
    height: calc(var(--cm-tabbar-h) + env(safe-area-inset-bottom, 0px));
    padding-bottom: env(safe-area-inset-bottom, 0px);
    background: rgba(255,255,255,.94); backdrop-filter: blur(14px);
    border-top: 1px solid var(--cm-border); justify-content: space-around;
  }
  .tab {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 3px; color: var(--cm-text-3); font-size: 11px; background: none; border: none; cursor: pointer;
  }
  .tab .el-icon { font-size: 20px; }
  .tab.active { color: var(--cm-primary); font-weight: 650; }
}
</style>
