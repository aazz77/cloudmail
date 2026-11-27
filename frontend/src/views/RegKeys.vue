<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">注册码</h2><div class="cm-subtitle">邀请注册凭证管理</div></div>
      <div class="head-actions">
        <el-button plain @click="clearUnused"><el-icon><Delete /></el-icon>&nbsp;清理失效</el-button>
        <el-button class="cm-btn-primary" @click="openAdd"><el-icon><Plus /></el-icon>&nbsp;生成注册码</el-button>
      </div>
    </div>
    <div class="cm-card cm-pad">
      <div class="filters">
        <el-input v-model="codeFilter" placeholder="注册码前缀" clearable style="width:200px" :prefix-icon="Search" @keyup.enter="load" @clear="load" />
        <el-button :icon="Search" @click="load">查询</el-button>
      </div>
      <el-table :data="list" v-loading="loading" style="width:100%">
        <el-table-column prop="code" label="注册码" min-width="150" />
        <el-table-column label="角色" width="140"><template #default="{row}">{{ row.roleName || '-' }}</template></el-table-column>
        <el-table-column prop="count" label="剩余次数" width="100" />
        <el-table-column label="过期时间" width="180">
          <template #default="{row}"><span :class="{expired: !row.expireTime}">{{ row.expireTime || '已过期' }}</span></template>
        </el-table-column>
        <el-table-column prop="createTime" label="创建时间" width="170" />
        <el-table-column label="操作" width="160" fixed="right">
          <template #default="{row}">
            <el-button text size="small" @click="showHistory(row)">使用记录</el-button>
            <el-button text size="small" type="danger" @click="remove(row)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <el-dialog v-model="addVisible" title="生成注册码" width="440px">
      <el-form label-position="top">
        <el-form-item label="注册码"><el-input v-model="form.code" placeholder="自定义码值" /></el-form-item>
        <el-form-item label="绑定角色">
          <el-select v-model="form.roleId" style="width:100%">
            <el-option v-for="r in roles" :key="r.roleId" :label="r.name" :value="r.roleId" />
          </el-select>
        </el-form-item>
        <el-form-item label="可用次数"><el-input-number v-model="form.count" :min="1" style="width:100%" /></el-form-item>
        <el-form-item label="过期时间">
          <el-date-picker v-model="expireDate" type="datetime" placeholder="选择过期时间" style="width:100%" format="YYYY-MM-DD HH:mm:ss" value-format="YYYY-MM-DD HH:mm:ss" />
        </el-form-item>
      </el-form>
      <template #footer><el-button @click="addVisible=false">取消</el-button><el-button class="cm-btn-primary" @click="add">生成</el-button></template>
    </el-dialog>

    <el-dialog v-model="historyVisible" title="使用记录" width="520px">
      <el-table :data="history" size="small">
        <el-table-column prop="email" label="邮箱" />
        <el-table-column prop="createTime" label="使用时间" />
      </el-table>
      <el-empty v-if="!history.length" description="暂无使用记录" :image-size="60" />
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
const codeFilter = ref('');
const addVisible = ref(false);
const form = reactive({ code:'', roleId: 1, count: 1 });
const expireDate = ref('');
const historyVisible = ref(false);
const history = ref([]);

const load = async () => {
  loading.value = true;
  try { list.value = await api.get('/api/regKey/list', { code: codeFilter.value }); }
  catch (e) { ElMessage.error(e.message); }
  finally { loading.value = false; }
};
onMounted(async () => {
  await load();
  try { roles.value = await api.get('/api/role/selectUse'); } catch {}
});

const openAdd = () => {
  Object.assign(form, { code:'', roleId: roles.value[0]?.roleId || 1, count: 1 });
  expireDate.value = ''; addVisible.value = true;
};
const add = async () => {
  if (!form.code || !expireDate.value) return ElMessage.warning('请填写注册码和过期时间');
  try {
    await api.post('/api/regKey/add', { code: form.code, roleId: form.roleId, count: form.count, expireTime: expireDate.value });
    addVisible.value = false; ElMessage.success('已生成'); load();
  } catch (e) { ElMessage.error(e.message); }
};
const remove = async (row) => {
  try { await ElMessageBox.confirm(`确定删除注册码「${row.code}」？`, '提示', { type:'warning' }); }
  catch { return; }
  try { await api.del('/api/regKey/delete', { regKeyIds: String(row.regKeyId) }); ElMessage.success('已删除'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const clearUnused = async () => {
  try { await ElMessageBox.confirm('确定清理所有已用完或已过期的注册码？', '提示', { type:'warning' }); }
  catch { return; }
  try { await api.del('/api/regKey/clearNotUse'); ElMessage.success('已清理'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const showHistory = async (row) => {
  try { history.value = await api.get('/api/regKey/history', { regKeyId: row.regKeyId }); historyVisible.value = true; }
  catch (e) { ElMessage.error(e.message); }
};
</script>

<style scoped>
.filters { display: flex; gap: 8px; margin-bottom: 16px; }
.head-actions { display: flex; gap: 8px; }
.expired { color: var(--cm-text-3); }
</style>
