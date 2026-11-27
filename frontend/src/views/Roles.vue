<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">角色权限</h2><div class="cm-subtitle">配置角色与功能权限</div></div>
      <el-button class="cm-btn-primary" @click="openAdd"><el-icon><Plus /></el-icon>&nbsp;新建角色</el-button>
    </div>
    <div class="cm-card cm-pad">
      <el-table :data="list" v-loading="loading" style="width:100%">
        <el-table-column prop="roleId" label="ID" width="70" />
        <el-table-column prop="name" label="角色名" min-width="140" />
        <el-table-column label="默认" width="80">
          <template #default="{row}"><el-tag v-if="row.isDefault" type="success" size="small">默认</el-tag></template>
        </el-table-column>
        <el-table-column label="发信" width="130">
          <template #default="{row}"><el-tag size="small">{{ sendTypeText(row.sendType) }}</el-tag></template>
        </el-table-column>
        <el-table-column prop="sendCount" label="发信额度" width="100" />
        <el-table-column prop="accountCount" label="账号上限" width="100" />
        <el-table-column label="权限数" width="90">
          <template #default="{row}">{{ (row.permIds||[]).length }}</template>
        </el-table-column>
        <el-table-column label="操作" width="200" fixed="right">
          <template #default="{row}">
            <el-button text size="small" @click="openEdit(row)">编辑</el-button>
            <el-button text size="small" :disabled="row.isDefault" @click="setDefault(row)">设默认</el-button>
            <el-button text size="small" type="danger" :disabled="row.isDefault" @click="remove(row)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <!-- 角色编辑弹窗 -->
    <el-dialog v-model="editVisible" :title="editing ? '编辑角色' : '新建角色'" width="640px" top="4vh">
      <el-form label-position="top">
        <el-row :gutter="16">
          <el-col :span="12"><el-form-item label="角色名"><el-input v-model="form.name" /></el-form-item></el-col>
          <el-col :span="12"><el-form-item label="标识 Key"><el-input v-model="form.key" /></el-form-item></el-col>
        </el-row>
        <el-form-item label="描述"><el-input v-model="form.description" /></el-form-item>
        <el-row :gutter="16">
          <el-col :span="8"><el-form-item label="发信类型">
            <el-select v-model="form.sendType" style="width:100%">
              <el-option label="禁止发信" value="ban" /><el-option label="按总量" value="count" /><el-option label="按天" value="day" />
            </el-select>
          </el-form-item></el-col>
          <el-col :span="8"><el-form-item label="发信额度"><el-input-number v-model="form.sendCount" :min="0" style="width:100%" /></el-form-item></el-col>
          <el-col :span="8"><el-form-item label="账号上限"><el-input-number v-model="form.accountCount" :min="0" style="width:100%" /></el-form-item></el-col>
        </el-row>
        <el-row :gutter="16">
          <el-col :span="12"><el-form-item label="禁止收发的邮箱/域名（逗号分隔）"><el-input v-model="banEmailStr" placeholder="如 spam.com, *" /></el-form-item></el-col>
          <el-col :span="12"><el-form-item label="可用域名（逗号分隔）"><el-input v-model="availDomainStr" placeholder="留空不限制" /></el-form-item></el-col>
        </el-row>
        <el-form-item label="功能权限">
          <div class="perm-tree">
            <div v-for="g in permTree" :key="g.permId" class="perm-group">
              <div class="perm-group-title">{{ g.name }}</div>
              <el-checkbox-group v-model="form.permIds">
                <el-checkbox v-for="c in g.children || []" :key="c.permId" :value="c.permId">{{ c.name }}</el-checkbox>
              </el-checkbox-group>
            </div>
          </div>
        </el-form-item>
      </el-form>
      <template #footer><el-button @click="editVisible=false">取消</el-button><el-button class="cm-btn-primary" @click="save">保存</el-button></template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, reactive, computed, onMounted } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { api } from '../api';

const list = ref([]);
const permTree = ref([]);
const loading = ref(false);
const editVisible = ref(false);
const editing = ref(false);

const blank = () => ({ name:'', key:'', description:'', sendType:'count', sendCount:10, accountCount:10, permIds:[], sort:0, banEmailType:0 });
const form = reactive(blank());
const banEmailStr = ref('');
const availDomainStr = ref('');

const sendTypeText = (t) => ({ ban:'禁止', count:'按总量', day:'按天' }[t] || t);

const load = async () => {
  loading.value = true;
  try {
    list.value = await api.get('/api/role/list');
    permTree.value = await api.get('/api/role/tree');
  } catch (e) { ElMessage.error(e.message); }
  finally { loading.value = false; }
};
onMounted(load);

const openAdd = () => { Object.assign(form, blank()); banEmailStr.value=''; availDomainStr.value=''; editing.value=false; editVisible.value=true; };
const openEdit = (row) => {
  Object.assign(form, {
    name: row.name || '', key: row.key || '', description: row.description || '',
    sendType: row.sendType || 'count', sendCount: row.sendCount ?? 10, accountCount: row.accountCount ?? 10,
    permIds: [...(row.permIds || [])], sort: row.sort ?? 0, banEmailType: row.banEmailType ?? 0,
  });
  banEmailStr.value = (row.banEmail || []).join(', ');
  availDomainStr.value = (row.availDomain || []).join(', ');
  editing.value = true; editVisible.value = true;
};
const buildBody = () => ({
  ...form,
  banEmail: banEmailStr.value.split(',').map(s=>s.trim()).filter(Boolean),
  availDomain: availDomainStr.value.split(',').map(s=>s.trim()).filter(Boolean),
  roleId: form.roleId || undefined,
});
const save = async () => {
  if (!form.name) return ElMessage.warning('请输入角色名');
  try {
    if (editing.value) await api.put('/api/role/set', buildBody());
    else await api.post('/api/role/add', buildBody());
    editVisible.value = false; ElMessage.success('已保存'); load();
  } catch (e) { ElMessage.error(e.message); }
};
const setDefault = async (row) => {
  try { await api.put('/api/role/setDefault', { roleId: row.roleId }); ElMessage.success('已设为默认'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const remove = async (row) => {
  try { await ElMessageBox.confirm(`确定删除角色「${row.name}」？`, '警告', { type:'warning' }); }
  catch { return; }
  try { await api.del('/api/role/delete', { roleId: row.roleId }); ElMessage.success('已删除'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
</script>

<style scoped>
.perm-tree { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px,1fr)); gap: 12px; max-height: 320px; overflow-y: auto; }
.perm-group { background: var(--cm-surface-soft); border-radius: 12px; padding: 12px; }
.perm-group-title { font-weight: 700; font-size: 13px; margin-bottom: 8px; color: var(--cm-text-2); }
.perm-group :deep(.el-checkbox) { display: flex; margin: 4px 0; }
</style>
