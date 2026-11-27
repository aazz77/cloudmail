<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">全部邮件</h2><div class="cm-subtitle">全站邮件总览（管理员）</div></div>
      <el-button v-if="selected.length" type="danger" plain @click="batchDelete"><el-icon><Delete /></el-icon>&nbsp;删除选中 ({{ selected.length }})</el-button>
    </div>

    <div class="cm-card cm-pad">
      <div class="filters">
        <el-select v-model="type" style="width:130px" @change="reload">
          <el-option label="收件" value="receive" /><el-option label="发件" value="send" />
          <el-option label="已删除" value="delete" /><el-option label="无人认领" value="noone" />
        </el-select>
        <el-input v-model="keyword" placeholder="搜索" clearable style="width:180px" :prefix-icon="Search" @keyup.enter="reload" @clear="reload" />
        <el-button :icon="Search" @click="reload">搜索</el-button>
      </div>

      <el-table :data="list" row-key="emailId" v-loading="loading" @selection-change="selected=$event" style="width:100%">
        <el-table-column type="selection" width="44" />
        <el-table-column prop="createTime" label="时间" width="150" />
        <el-table-column label="用户" width="140">
          <template #default="{row}"><span class="cell-ellipsis">{{ row.userEmail || '-' }}</span></template>
        </el-table-column>
        <el-table-column label="发件人" min-width="150">
          <template #default="{row}"><span class="cell-ellipsis">{{ row.name || row.sendEmail }}</span></template>
        </el-table-column>
        <el-table-column label="主题" min-width="180">
          <template #default="{row}"><span class="cell-ellipsis" @click="open(row)">{{ row.subject || '（无主题）' }}</span></template>
        </el-table-column>
        <el-table-column label="状态" width="100">
          <template #default="{row}"><el-tag :type="statusType(row.status)" size="small">{{ statusText(row.status) }}</el-tag></template>
        </el-table-column>
        <el-table-column label="操作" width="90" fixed="right">
          <template #default="{row}"><el-button text type="danger" size="small" @click="remove(row)">删除</el-button></template>
        </el-table-column>
      </el-table>
      <div class="pager">
        <el-pagination background layout="prev, pager, next" :total="total" :page-size="20" v-model:current-page="page" @current-change="load" />
      </div>
    </div>

    <MailDetail v-model="detailVisible" :mail="current" :is-mobile="false" />
  </div>
</template>

<script setup>
import { ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Search } from '@element-plus/icons-vue';
import { api } from '../api';
import MailDetail from '../components/MailDetail.vue';

const list = ref([]);
const total = ref(0);
const page = ref(1);
const type = ref('receive');
const keyword = ref('');
const selected = ref([]);
const loading = ref(false);
const current = ref({});
const detailVisible = ref(false);

const load = async () => {
  loading.value = true;
  try {
    const data = await api.get('/api/allEmail/list', {
      type: type.value, size: 20, emailId: 0, full: 0,
      ...(keyword.value ? { searchType: 'subject', text: keyword.value } : {}),
    });
    list.value = data?.list || [];
    total.value = data?.total || 0;
  } catch (e) { ElMessage.error(e.message); }
  finally { loading.value = false; }
};
const reload = () => { page.value = 1; load(); };
const open = (row) => { current.value = { ...row, attList: [] }; detailVisible.value = true; };
const remove = async (row) => {
  try { await ElMessageBox.confirm('确定物理删除这封邮件？', '警告', { type: 'warning' }); }
  catch { return; }
  try { await api.del('/api/allEmail/delete', { emailIds: String(row.emailId) }); ElMessage.success('已删除'); load(); }
  catch (e) { ElMessage.error(e.message); }
};
const batchDelete = async () => {
  try { await ElMessageBox.confirm(`确定删除选中的 ${selected.value.length} 封邮件？`, '警告', { type: 'warning' }); }
  catch { return; }
  try {
    await api.del('/api/allEmail/delete', { emailIds: selected.value.map(x => x.emailId).join(',') });
    ElMessage.success('已删除'); selected.value = []; load();
  } catch (e) { ElMessage.error(e.message); }
};
const statusText = (s) => ({ 0:'收件',1:'已发送',2:'已投递',3:'退信',7:'无主' }[s] || `状态${s}`);
const statusType = (s) => s===7 ? 'warning' : (s===3 ? 'danger' : 'success');
</script>

<style scoped>
.filters { display: flex; gap: 8px; margin-bottom: 16px; flex-wrap: wrap; }
.pager { display: flex; justify-content: center; padding-top: 16px; }
.cell-ellipsis { display: block; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
</style>
