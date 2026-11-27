<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">用户管理</h2><div class="cm-subtitle">管理系统用户与权限</div></div>
      <el-button class="cm-btn-primary" @click="openAdd"><el-icon><Plus /></el-icon>&nbsp;添加用户</el-button>
    </div>
    <div class="cm-card cm-pad">
      <div class="filters">
        <el-input v-model="q.email" placeholder="邮箱前缀" clearable style="width:200px" @keyup.enter="reload" @clear="reload" />
        <el-select v-model="q.status" style="width:130px" @change="reload">
          <el-option label="全部" :value="-1" /><el-option label="正常" :value="0" /><el-option label="封禁" :value="1" />
        </el-select>
        <el-select v-model="q.isDel" style="width:130px" @change="reload">
          <el-option label="未删除" :value="0" /><el-option label="已删除" :value="1" />
        </el-select>
        <el-button :icon="Search" @click="reload">查询</el-button>
      </div>
      <el-table :data="list" v-loading="loading" style="width:100%">
        <el-table-column prop="userId" label="ID" width="70" />
        <el-table-column prop="email" label="邮箱" min-width="170" />
        <el-table-column label="角色" width="110">
          <template #default="{row}"><el-tag size="small" :type="row.type===1?'info':'primary'">{{ roleName(row.type) }}</el-tag></template>
        </el-table-column>
        <el-table-column prop="sendCount" label="已发" width="80" />
        <el-table-column label="状态" width="90">
          <template #default="{row}">
            <el-tag size="small" :type="row.status===1?'danger':'success'">{{ row.status===1?'封禁':'正常' }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="createTime" label="注册时间" width="160" />
        <el-table-column label="操作" min-width="240" fixed="right">
          <template #default="{row}">
            <el-button text size="small" @click="openPwd(row)">改密</el-button>
            <el-button text size="small" @click="toggleStatus(row)">{{ row.status===1?'解封':'封禁' }}</el-button>
            <el-button text size="small" @click="openType(row)">设角色</el-button>
            <el-button text size="small" @click="resetCount(row)">清额度</el-button>
            <el-button text size="small" @click="showAccounts(row)">账号</el-button>
            <el-button v-if="row.isDel" text size="small" type="warning" @click="restore(row)">恢复</el-button>
            <el-button text size="small" type="danger" @click="remove(row)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>
      <div class="pager">
        <el-pagination background layout="prev, pager, next" :page-size="20" v-model:current-page="q.num" @current-change="load" :total="list.length>=20?q.num*20:list.length" />
      </div>
    </div>

    <!-- 添加用户 -->
    <el-dialog v-model="addVisible" title="添加用户" width="440px">
      <el-form label-position="top">
        <el-form-item label="邮箱"><el-input v-model="addForm.email" placeholder="user@example.com" /></el-form-item>
        <el-form-item label="密码"><el-input v-model="addForm.password" placeholder="至少 6 位" /></el-form-item>
        <el-form-item label="角色">
          <el-select v-model="addForm.type_id" style="width:100%">
            <el-option v-for="r in roles" :key="r.roleId" :label="r.name" :value="r.roleId" />
          </el-select>
        </el-form-item>
      </el-form>
      <template #footer><el-button @click="addVisible=false">取消</el-button><el-button class="cm-btn-primary" @click="add">添加</el-button></template>
    </el-dialog>

    <!-- 改密 -->
    <el-dialog v-model="pwdVisible" title="修改密码" width="420px">
      <el-form label-position="top"><el-form-item label="新密码"><el-input v-model="pwdVal" type="password" show-password /></el-form-item></el-form>
      <template #footer><el-button @click="pwdVisible=false">取消</el-button><el-button class="cm-btn-primary" @click="doPwd">保存</el-button></template>
    </el-dialog>

    <!-- 设角色 -->
    <el-dialog v-model="typeVisible" title="设置角色" width="420px">
      <el-form label-position="top"><el-form-item label="角色">
        <el-select v-model="typeVal" style="width:100%"><el-option v-for="r in roles" :key="r.roleId" :label="r.name" :value="r.roleId" /></el-select>
      </el-form-item></el-form>
      <template #footer><el-button @click="typeVisible=false">取消</el-button><el-button class="cm-btn-primary" @click="doType">保存</el-button></template>
    </el-dialog>

    <!-- 账号列表 -->
    <el-dialog v-model="accVisible" title="用户邮箱账号" width="560px">
      <el-table :data="accList" size="small">
        <el-table-column prop="accountId" label="ID" width="70" />
        <el-table-column prop="email" label="邮箱" min-width="180" />
        <el-table-column prop="name" label="名称" />
        <el-table-column label="操作" width="90">
          <template #default="{row}"><el-button text size="small" type="danger" @click="delAccount(row)">删除</el-button></template>
        </el-table-column>
      </el-table>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Search } from '@element-plus/icons-vue';
import { api } from '../api';

const list = ref([]);
const roles = ref([]);
const loading = ref(false);
const q = reactive({ num: 1, size: 20, email: '', status: -1, isDel: 0, timeSort: 0 });

const addVisible = ref(false);
const addForm = reactive({ email: '', password: '', type_id: 1 });
const pwdVisible = ref(false); const pwdVal = ref(''); const cur = ref(null);
const typeVisible = ref(false); const typeVal = ref(0);
const accVisible = ref(false); const accList = ref([]);

const load = async () => {
  loading.value = true;
  try {
    const data = await api.get('/api/user/list', { ...q, num: Math.max(1, q.num), status: q.status, isDel: q.isDel });
    list.value = data?.list || [];
  } catch (e) { ElMessage.error(e.message); }
  finally { loading.value = false; }
};
const reload = () => { q.num = 1; load(); };
const roleName = (t) => ({ 0:'管理员',1:'默认' }[t] || `角色${t}`);

const openAdd = () => { Object.assign(addForm, { email:'', password:'', type_id: roles.value[0]?.roleId || 1 }); addVisible.value = true; };
const add = async () => {
  if (!addForm.email || !addForm.password) return ElMessage.warning('请填写邮箱和密码');
  try {
    await api.post('/api/user/add', { email: addForm.email, password: addForm.password, type_id: addForm.type_id });
    addVisible.value = false; ElMessage.success('已添加'); reload();
  } catch (e) { ElMessage.error(e.message); }
};

const openPwd = (row) => { cur.value = row; pwdVal.value = ''; pwdVisible.value = true; };
const doPwd = async () => {
  if (pwdVal.value.length < 6) return ElMessage.warning('密码至少 6 位');
  try { await api.put('/api/user/setPwd', { userId: cur.value.userId, password: pwdVal.value }); pwdVisible.value = false; ElMessage.success('已修改'); }
  catch (e) { ElMessage.error(e.message); }
};

const toggleStatus = async (row) => {
  try { await api.put('/api/user/setStatus', { userId: row.userId, status: row.status === 1 ? 0 : 1 }); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const openType = (row) => { cur.value = row; typeVal.value = row.type; typeVisible.value = true; };
const doType = async () => {
  try { await api.put('/api/user/setType', { userId: cur.value.userId, typeId: typeVal.value }); typeVisible.value = false; ElMessage.success('已设置'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const resetCount = async (row) => {
  try { await api.put('/api/user/resetSendCount', { userId: row.userId }); ElMessage.success('额度已清零'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const showAccounts = async (row) => {
  try { const d = await api.get('/api/user/allAccount', { userId: row.userId }); accList.value = d?.list || []; accVisible.value = true; }
  catch (e) { ElMessage.error(e.message); }
};
const delAccount = async (row) => {
  try { await api.del('/api/user/deleteAccount', { accountId: row.accountId }); ElMessage.success('已删除'); showAccounts(cur.value); }
  catch (e) { ElMessage.error(e.message); }
};
const restore = async (row) => {
  try { await api.put('/api/user/restore', { userId: row.userId, type: 0 }); ElMessage.success('已恢复'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const remove = async (row) => {
  try { await ElMessageBox.confirm(`确定删除用户 ${row.email}？`, '警告', { type: 'warning' }); }
  catch { return; }
  try { await api.del('/api/user/delete', { ids: String(row.userId) }); ElMessage.success('已删除'); load(); }
  catch (e) { ElMessage.error(e.message); }
};

onMounted(async () => {
  await load();
  try { roles.value = await api.get('/api/role/selectUse'); } catch {}
});
</script>

<style scoped>
.filters { display: flex; gap: 8px; margin-bottom: 16px; flex-wrap: wrap; }
.pager { display: flex; justify-content: center; padding-top: 16px; }
</style>
