<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div>
        <h2 class="cm-title">收件箱</h2>
        <div class="cm-subtitle" v-if="total !== null">共 {{ total }} 封邮件 · 未读自动标记</div>
      </div>
      <div class="toolbar">
        <el-select v-model="accountId" placeholder="全部账号" clearable style="width:160px" @change="reload">
          <el-option v-for="a in accounts" :key="a.accountId" :label="a.email" :value="a.accountId" />
        </el-select>
        <el-select v-model="searchType" style="width:120px">
          <el-option label="主题" value="subject" />
          <el-option label="正文" value="content" />
          <el-option label="发件人" value="sender" />
          <el-option label="发件地址" value="senderAddress" />
        </el-select>
        <el-input v-model="keyword" placeholder="搜索邮件" clearable style="width:180px" :prefix-icon="Search" @keyup.enter="reload" @clear="reload" />
        <el-button :icon="Search" @click="reload">搜索</el-button>
        <el-tooltip :content="timeSort ? '时间从早到晚' : '时间从晚到早'">
          <el-button :icon="Sort" @click="toggleSort" />
        </el-tooltip>
      </div>
    </div>

    <div class="cm-card cm-pad">
      <MailList :list="list" :empty-text="keyword ? '没有匹配的邮件' : '收件箱空空如也'" @open="openMail" @star="toggleStar" />
      <div class="load-more" v-if="list.length">
        <el-button text :loading="loadingMore" @click="loadMore">加载更多</el-button>
      </div>
    </div>

    <MailDetail v-model="detailVisible" :mail="current" :is-mobile="isMobile"
      @star="toggleStar" @delete="deleteMail" @reply="replyMail" />

    <!-- 回复弹窗 -->
    <el-dialog v-model="replyVisible" title="回复邮件" width="620px">
      <el-form label-position="top">
        <el-form-item label="收件人"><el-input v-model="replyData.receiveEmail" readonly /></el-form-item>
        <el-form-item label="主题"><el-input v-model="replyData.subject" /></el-form-item>
        <el-form-item label="正文"><el-input v-model="replyData.text" type="textarea" :rows="8" /></el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="replyVisible=false">取消</el-button>
        <el-button class="cm-btn-primary" :loading="sending" @click="sendReply">发送</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue';
import { useRouter } from 'vue-router';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Search, Sort } from '@element-plus/icons-vue';
import { api } from '../api';
import MailList from '../components/MailList.vue';
import MailDetail from '../components/MailDetail.vue';

const router = useRouter();
const isMobile = ref(window.innerWidth <= 900);
const list = ref([]);
const total = ref(null);
const accounts = ref([]);
const accountId = ref(null);
// 未选具体账号时收全部（后端 allReceive=1 会忽略 accountId），
// 选了账号则只看该账号（allReceive=0 + accountId）。
const allReceive = () => (accountId.value ? 0 : 1);
const searchType = ref('subject');
const keyword = ref('');
const timeSort = ref(0);
const loadingMore = ref(false);

const detailVisible = ref(false);
const current = ref({});
const replyVisible = ref(false);
const sending = ref(false);
const replyData = ref({ receiveEmail: '', subject: '', text: '', accountId: 0, emailId: 0 });

let pollTimer = null;

const onResize = () => { isMobile.value = window.innerWidth <= 900; };

const buildQuery = (cursor = 0) => ({
  accountId: accountId.value || 0,
  allReceive: allReceive(),
  size: 20,
  emailId: cursor,
  timeSort: timeSort.value,
  full: 1,
  type: 0,
  ...(keyword.value ? { searchType: searchType.value, text: keyword.value } : {}),
});

const load = async (cursor = 0, append = false) => {
  try {
    const data = await api.get('/api/email/list', buildQuery(cursor));
    const rows = data?.list || [];
    list.value = append ? [...list.value, ...rows] : rows;
    total.value = data?.total ?? null;
  } catch (e) { ElMessage.error(e.message); }
};
const reload = () => load(0, false);
const loadMore = async () => {
  const last = list.value[list.value.length - 1];
  if (!last) return;
  loadingMore.value = true;
  try { await load(last.emailId, true); }
  finally { loadingMore.value = false; }
};
const toggleSort = () => { timeSort.value = timeSort.value ? 0 : 1; reload(); };

const openMail = async (mail) => {
  current.value = mail;
  detailVisible.value = true;
  // 标记已读（后端 unread=1 表示已读）
  if (!mail.unread) {
    try { await api.put('/api/email/read', { emailIds: String(mail.emailId) }); mail.unread = 1; }
    catch {}
  }
};

const toggleStar = async (mail) => {
  const was = !!mail.isStar;
  mail.isStar = was ? 0 : 1;
  try {
    if (was) await api.del('/api/star/cancel', { emailId: mail.emailId });
    else await api.post('/api/star/add', { emailId: mail.emailId });
  } catch (e) { mail.isStar = was ? 1 : 0; ElMessage.error(e.message); }
};

const deleteMail = async (mail) => {
  try { await ElMessageBox.confirm('确定删除这封邮件？', '提示', { type: 'warning' }); }
  catch { return; }
  try {
    await api.del('/api/email/delete', { emailIds: String(mail.emailId) });
    detailVisible.value = false;
    ElMessage.success('已删除');
    reload();
  } catch (e) { ElMessage.error(e.message); }
};

const replyMail = (mail) => {
  detailVisible.value = false;
  replyData.value = {
    receiveEmail: mail.sendEmail || '',
    subject: mail.subject?.startsWith('Re:') ? mail.subject : `Re: ${mail.subject || ''}`,
    text: `\n\n---------- 原邮件 ----------\n${mail.text || ''}`,
    accountId: mail.accountId || 0,
    emailId: mail.emailId,
  };
  replyVisible.value = true;
};
const sendReply = async () => {
  sending.value = true;
  try {
    await api.post('/api/email/send', {
      accountId: replyData.value.accountId,
      name: '',
      receiveEmail: [replyData.value.receiveEmail],
      subject: replyData.value.subject,
      text: replyData.value.text,
      sendType: 'reply',
      emailId: replyData.value.emailId,
    });
    replyVisible.value = false;
    ElMessage.success('回复已发送');
  } catch (e) { ElMessage.error(e.message); }
  finally { sending.value = false; }
};

// 轮询新邮件：用 emailId 锚点避免重复
const poll = async () => {
  if (!list.value.length) return;
  const cursor = Math.max(...list.value.map(m => m.emailId));
  try {
    const fresh = await api.get('/api/email/latest', { emailId: cursor, accountId: accountId.value || 0, allReceive: allReceive() });
    if (fresh?.length) {
      fresh.forEach(m => m.unread = m.unread ?? 0);
      list.value = [...fresh, ...list.value];
      ElMessage.info(`${fresh.length} 封新邮件`);
    }
  } catch {}
};

onMounted(async () => {
  window.addEventListener('resize', onResize);
  try { accounts.value = await api.get('/api/account/list', { size: 30 }); } catch {}
  await reload();
  pollTimer = setInterval(poll, 30000);
});
onUnmounted(() => {
  window.removeEventListener('resize', onResize);
  if (pollTimer) clearInterval(pollTimer);
});
</script>

<style scoped>
.toolbar { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
.load-more { text-align: center; padding-top: 8px; }
@media (max-width: 900px) {
  .toolbar { width: 100%; justify-content: stretch; }
  .toolbar :deep(.el-select), .toolbar :deep(.el-input) { width: 100% !important; flex: 1 1 45%; }
}
</style>
