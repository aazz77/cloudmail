<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">发件箱</h2><div class="cm-subtitle">已发送的邮件</div></div>
      <el-button class="cm-btn-primary" @click="$router.push('/send')"><el-icon><EditPen /></el-icon>&nbsp;写邮件</el-button>
    </div>
    <div class="cm-card cm-pad">
      <MailList :list="list" empty-text="还没有发送过邮件" @open="open" @star="toggleStar" />
      <div class="load-more" v-if="list.length"><el-button text @click="loadMore">加载更多</el-button></div>
    </div>
    <MailDetail v-model="detailVisible" :mail="current" :is-mobile="isMobile" @star="toggleStar" />
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue';
import { ElMessage } from 'element-plus';
import { api } from '../api';
import MailList from '../components/MailList.vue';
import MailDetail from '../components/MailDetail.vue';

const isMobile = ref(window.innerWidth <= 900);
const list = ref([]);
const current = ref({});
const detailVisible = ref(false);
const onResize = () => { isMobile.value = window.innerWidth <= 900; };

const load = async (cursor = 0, append = false) => {
  try {
    const data = await api.get('/api/email/list', { emailId: cursor, size: 20, full: 1, type: 1, allReceive: 1 });
    const rows = data?.list || [];
    list.value = append ? [...list.value, ...rows] : rows;
  } catch (e) { ElMessage.error(e.message); }
};
const loadMore = async () => {
  const last = list.value[list.value.length - 1];
  if (last) await load(last.emailId, true);
};
const open = (m) => { current.value = m; detailVisible.value = true; };
const toggleStar = async (m) => {
  const was = !!m.isStar;
  m.isStar = was ? 0 : 1;
  try {
    if (was) await api.del('/api/star/cancel', { emailId: m.emailId });
    else await api.post('/api/star/add', { emailId: m.emailId });
  } catch (e) { m.isStar = was ? 1 : 0; ElMessage.error(e.message); }
};

onMounted(() => { window.addEventListener('resize', onResize); load(); });
onUnmounted(() => window.removeEventListener('resize', onResize));
</script>

<style scoped>.load-more{text-align:center;padding-top:8px}</style>
