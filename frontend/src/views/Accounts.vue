<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">邮箱账号</h2><div class="cm-subtitle">管理你的收发邮箱地址</div></div>
      <el-button class="cm-btn-primary" @click="openAdd"><el-icon><Plus /></el-icon>&nbsp;添加账号</el-button>
    </div>

    <div class="account-grid">
      <div v-for="a in list" :key="a.accountId" class="cm-card account-card">
        <div class="ac-top">
          <div class="ac-avatar">{{ (a.email||'?')[0].toUpperCase() }}</div>
          <div class="ac-meta">
            <strong>{{ a.name || a.email.split('@')[0] }}</strong>
            <span>{{ a.email }}</span>
          </div>
          <el-tag v-if="a.status === 1" type="danger" size="small">禁用</el-tag>
        </div>
        <div class="ac-info">
          <span>最新收信：{{ a.latestEmailTime || '暂无' }}</span>
          <el-tag :type="a.allReceive ? 'success' : 'info'" size="small">{{ a.allReceive ? '全部收信' : '默认收信' }}</el-tag>
        </div>
        <div class="ac-actions">
          <el-button text size="small" @click="openRename(a)"><el-icon><Edit /></el-icon>&nbsp;改名</el-button>
          <el-button text size="small" @click="toggleReceive(a)"><el-icon><Switch /></el-icon>&nbsp;{{ a.allReceive ? '关全收' : '开全收' }}</el-button>
          <el-button text size="small" @click="setTop(a)"><el-icon><Top /></el-icon>&nbsp;置顶</el-button>
          <el-button text size="small" type="danger" @click="remove(a)"><el-icon><Delete /></el-icon>&nbsp;删除</el-button>
        </div>
      </div>
      <div v-if="!list.length" class="cm-empty" style="grid-column:1/-1">
        <div class="big">📂</div><div>还没有邮箱账号</div>
      </div>
    </div>

    <el-dialog v-model="addVisible" title="添加邮箱账号" width="440px">
      <el-form label-position="top">
        <el-form-item label="邮箱地址"><el-input v-model="newEmail" placeholder="you@example.com" /></el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="addVisible=false">取消</el-button>
        <el-button class="cm-btn-primary" :loading="busy" @click="add">添加</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="renameVisible" title="修改账号名称" width="440px">
      <el-form label-position="top">
        <el-form-item label="显示名称"><el-input v-model="renameVal" /></el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="renameVisible=false">取消</el-button>
        <el-button class="cm-btn-primary" @click="doRename">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { api } from '../api';

const list = ref([]);
const busy = ref(false);
const addVisible = ref(false);
const newEmail = ref('');
const renameVisible = ref(false);
const renameVal = ref('');
const current = ref(null);

const load = async () => {
  try { list.value = await api.get('/api/account/list', { size: 30 }); }
  catch (e) { ElMessage.error(e.message); }
};
onMounted(load);

const openAdd = () => { newEmail.value = ''; addVisible.value = true; };
const add = async () => {
  if (!newEmail.value) return ElMessage.warning('请输入邮箱');
  busy.value = true;
  try {
    await api.post('/api/account/add', { email: newEmail.value });
    addVisible.value = false;
    ElMessage.success('添加成功');
    load();
  } catch (e) { ElMessage.error(e.message); }
  finally { busy.value = false; }
};
const openRename = (a) => { current.value = a; renameVal.value = a.name || ''; renameVisible.value = true; };
const doRename = async () => {
  try {
    await api.put('/api/account/setName', { accountId: current.value.accountId, name: renameVal.value });
    renameVisible.value = false;
    ElMessage.success('已保存');
    load();
  } catch (e) { ElMessage.error(e.message); }
};
const toggleReceive = async (a) => {
  try { await api.put('/api/account/setAllReceive', { accountId: a.accountId }); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const setTop = async (a) => {
  try { await api.put('/api/account/setAsTop', { accountId: a.accountId }); ElMessage.success('已置顶'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const remove = async (a) => {
  try { await ElMessageBox.confirm(`确定删除账号 ${a.email}？`, '提示', { type: 'warning' }); }
  catch { return; }
  try { await api.del('/api/account/delete', { accountId: a.accountId }); ElMessage.success('已删除'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
</script>

<style scoped>
.account-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 16px; }
.account-card { padding: 18px; }
.ac-top { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
.ac-avatar { width: 42px; height: 42px; border-radius: 12px; flex: 0 0 auto; background: linear-gradient(135deg,#4f6ef7,#8b5cf6); color: #fff; display: grid; place-items: center; font-weight: 700; font-size: 17px; }
.ac-meta { display: flex; flex-direction: column; min-width: 0; line-height: 1.4; }
.ac-meta strong { font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ac-meta span { font-size: 12px; color: var(--cm-text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ac-top .el-tag { margin-left: auto; }
.ac-info { display: flex; align-items: center; justify-content: space-between; font-size: 12px; color: var(--cm-text-2); padding: 10px 0; border-top: 1px solid var(--cm-border); }
.ac-actions { display: flex; flex-wrap: wrap; gap: 2px; border-top: 1px solid var(--cm-border); padding-top: 8px; }
.ac-actions .el-button { padding: 6px 8px; font-size: 12px; }
</style>
