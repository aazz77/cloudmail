<template>
  <div class="cm-page">
    <div class="cm-page-head"><div><h2 class="cm-title">个人中心</h2><div class="cm-subtitle">账号信息与安全设置</div></div></div>

    <div class="profile-grid">
      <div class="cm-card cm-pad profile-card">
        <div class="profile-avatar">{{ (me.email||'?')[0].toUpperCase() }}</div>
        <h3>{{ me.name || (me.email||'').split('@')[0] }}</h3>
        <p class="email">{{ me.email }}</p>
        <div class="badges">
          <el-tag v-if="isAdmin" type="danger">管理员</el-tag>
          <el-tag v-else type="primary">{{ me.role?.name || '普通用户' }}</el-tag>
          <el-tag type="info">已发 {{ me.sendCount || 0 }} 封</el-tag>
        </div>
      </div>

      <div class="cm-card cm-pad">
        <div class="section-title">账号详情</div>
        <div class="info-list">
          <div><span>用户 ID</span><strong>{{ me.userId }}</strong></div>
          <div><span>邮箱</span><strong>{{ me.email }}</strong></div>
          <div><span>角色</span><strong>{{ me.role?.name || (isAdmin ? '管理员' : '-') }}</strong></div>
          <div><span>发信额度</span><strong>{{ me.role?.sendType === 'day' ? '按天' : (me.role?.sendType === 'count' ? '按总量' : '禁止') }}</strong></div>
          <div><span>绑定账号</span><strong>{{ me.account?.email || '未绑定' }}</strong></div>
        </div>
      </div>

      <div class="cm-card cm-pad">
        <div class="section-title">修改密码</div>
        <el-form label-position="top" style="max-width:360px">
          <el-form-item label="新密码"><el-input v-model="pwd" type="password" show-password placeholder="6–30 位" /></el-form-item>
          <el-button class="cm-btn-primary" :loading="busy" @click="changePwd">更新密码</el-button>
        </el-form>
      </div>

      <div class="cm-card cm-pad danger-zone">
        <div class="section-title">危险操作</div>
        <p>注销账号将删除你的全部数据且不可恢复，请谨慎操作。</p>
        <el-button type="danger" plain @click="delAccount">注销账号</el-button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { ElMessage, ElMessageBox } from 'element-plus';
import { api, setToken } from '../api';

const router = useRouter();
const me = ref({});
const pwd = ref('');
const busy = ref(false);
const isAdmin = computed(() => (me.value.permKeys || []).includes('*'));

const load = async () => {
  try { me.value = await api.get('/api/my/loginUserInfo'); }
  catch (e) { ElMessage.error(e.message); }
};
onMounted(load);

const changePwd = async () => {
  if (pwd.value.length < 6) return ElMessage.warning('密码至少 6 位');
  busy.value = true;
  try { await api.put('/api/my/resetPassword', { password: pwd.value }); pwd.value = ''; ElMessage.success('密码已更新'); }
  catch (e) { ElMessage.error(e.message); }
  finally { busy.value = false; }
};

const delAccount = async () => {
  try { await ElMessageBox.confirm('确定注销账号？此操作不可恢复！', '危险操作', { type:'error', confirmButtonText:'确认注销' }); }
  catch { return; }
  try {
    await api.del('/api/my/delete');
    setToken(''); ElMessage.success('账号已注销'); router.push('/login');
  } catch (e) { ElMessage.error(e.message); }
};
</script>

<style scoped>
.profile-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px,1fr)); gap: 16px; }
.profile-card { text-align: center; }
.profile-avatar { width: 72px; height: 72px; margin: 6px auto 14px; border-radius: 22px; background: linear-gradient(135deg,#4f6ef7,#8b5cf6); color: #fff; display: grid; place-items: center; font-size: 30px; font-weight: 700; box-shadow: 0 10px 26px rgba(79,110,247,.3); }
.profile-card h3 { margin: 0; font-size: 19px; }
.profile-card .email { color: var(--cm-text-2); font-size: 13px; margin: 5px 0 14px; }
.badges { display: flex; gap: 8px; justify-content: center; flex-wrap: wrap; }
.section-title { font-size: 15px; font-weight: 700; margin-bottom: 18px; }
.info-list { display: flex; flex-direction: column; gap: 12px; }
.info-list > div { display: flex; justify-content: space-between; font-size: 13px; padding-bottom: 10px; border-bottom: 1px solid var(--cm-border); }
.info-list span { color: var(--cm-text-2); }
.info-list strong { word-break: break-all; text-align: right; }
.danger-zone { border-color: #fecaca; }
.danger-zone p { color: var(--cm-text-2); font-size: 13px; line-height: 1.6; }
</style>
