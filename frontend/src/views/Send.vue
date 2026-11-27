<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">写邮件</h2><div class="cm-subtitle">收件人将以你选择的账号发出</div></div>
    </div>
    <div class="cm-card cm-pad" style="max-width:900px">
      <el-form label-position="top" @submit.prevent>
        <el-form-item label="发件账号">
          <el-select v-model="form.accountId" placeholder="选择发件邮箱" style="width:100%" filterable>
            <el-option v-for="a in accounts" :key="a.accountId" :label="a.email" :value="a.accountId" />
          </el-select>
        </el-form-item>
        <el-form-item label="显示名称">
          <el-input v-model="form.name" placeholder="发件人显示名（留空取邮箱前缀）" />
        </el-form-item>
        <el-form-item label="收件人">
          <el-select v-model="form.receiveEmail" multiple filterable allow-create default-first-option
            placeholder="输入邮箱后回车" style="width:100%" :reserve-keyword="false">
          </el-select>
        </el-form-item>
        <el-form-item label="抄送">
          <el-select v-model="form.ccEmail" multiple filterable allow-create default-first-option placeholder="选填" style="width:100%" />
        </el-form-item>
        <el-form-item label="密送">
          <el-select v-model="form.bccEmail" multiple filterable allow-create default-first-option placeholder="选填" style="width:100%" />
        </el-form-item>
        <el-form-item label="主题">
          <el-input v-model="form.subject" placeholder="邮件主题" />
        </el-form-item>
        <el-form-item label="正文">
          <div class="editor-bar">
            <el-radio-group v-model="mode" size="small">
              <el-radio-button value="text">纯文本</el-radio-button>
              <el-radio-button value="html">HTML</el-radio-button>
            </el-radio-group>
          </div>
          <el-input v-if="mode==='text'" v-model="form.text" type="textarea" :rows="12" placeholder="输入正文…" />
          <el-input v-else v-model="form.content" type="textarea" :rows="12" placeholder="输入 HTML…" />
        </el-form-item>
        <el-form-item label="附件">
          <input type="file" multiple @change="onFiles" ref="fileInput" />
          <div class="att-preview" v-if="atts.length">
            <el-tag v-for="(a,i) in atts" :key="i" closable @close="atts.splice(i,1)" style="margin:4px 6px 0 0">
              {{ a.filename }} ({{ fmtSize(a.size) }})
            </el-tag>
          </div>
        </el-form-item>
        <el-form-item>
          <el-checkbox v-model="form.manyTypeDivide">每封单独发送（不显示抄送密送）</el-checkbox>
        </el-form-item>
      </el-form>
      <div class="send-actions">
        <el-button class="cm-btn-primary" :loading="sending" @click="send">
          <el-icon><Promotion /></el-icon>&nbsp;发送邮件
        </el-button>
        <el-button @click="reset">清空</el-button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ElMessage } from 'element-plus';
import { api } from '../api';

const route = useRoute();
const router = useRouter();
const accounts = ref([]);
const sending = ref(false);
const mode = ref('text');
const fileInput = ref(null);
const atts = ref([]);

const form = reactive({
  accountId: 0, name: '', receiveEmail: [], ccEmail: [], bccEmail: [],
  subject: '', text: '', content: '', manyTypeDivide: false,
  sendType: '', emailId: 0,
});

onMounted(async () => {
  try {
    accounts.value = await api.get('/api/account/list', { size: 30 });
    if (accounts.value.length) form.accountId = accounts.value[0].accountId;
  } catch {}
  // 从收件箱跳转的回复
  if (route.query.replyId) {
    form.receiveEmail = route.query.to ? [route.query.to] : [];
    form.subject = route.query.subject || '';
    form.accountId = Number(route.query.accountId) || form.accountId;
    form.sendType = 'reply';
    form.emailId = Number(route.query.replyId);
  }
});

const onFiles = async (e) => {
  const files = Array.from(e.target.files || []);
  for (const f of files) {
    if (f.size > 10 * 1024 * 1024) { ElMessage.error(`${f.name} 超过 10MB`); continue; }
    const base64 = await new Promise(res => {
      const r = new FileReader();
      r.onload = () => res(String(r.result).split(',')[1]);
      r.readAsDataURL(f);
    });
    atts.value.push({ content: base64, filename: f.name, size: f.size, mimeType: f.type });
  }
  e.target.value = '';
};
const fmtSize = (n) => n > 1024*1024 ? (n/1024/1024).toFixed(1)+'MB' : Math.ceil(n/1024)+'KB';

const send = async () => {
  if (!form.accountId) return ElMessage.warning('请选择发件账号');
  if (!form.receiveEmail.length) return ElMessage.warning('请填写收件人');
  sending.value = true;
  try {
    const body = {
      accountId: form.accountId,
      name: form.name,
      receiveEmail: form.receiveEmail,
      ccEmail: form.ccEmail,
      bccEmail: form.bccEmail,
      subject: form.subject,
      text: mode.value === 'text' ? form.text : '',
      content: mode.value === 'html' ? form.content : '',
      attachments: atts.value,
      manyType: form.manyTypeDivide ? 'divide' : '',
    };
    if (form.sendType === 'reply') { body.sendType = 'reply'; body.emailId = form.emailId; }
    const data = await api.post('/api/email/send', body);
    const failed = data?.failedRecipients;
    if (failed?.length) ElMessage.warning(`部分收件人发送失败：${failed.join(', ')}`);
    else ElMessage.success('邮件已发送');
    router.push('/sent');
  } catch (e) { ElMessage.error(e.message); }
  finally { sending.value = false; }
};

const reset = () => {
  Object.assign(form, { name:'', receiveEmail:[], ccEmail:[], bccEmail:[], subject:'', text:'', content:'', manyTypeDivide:false, sendType:'', emailId:0 });
  atts.value = [];
};
</script>

<style scoped>
.editor-bar { margin-bottom: 8px; }
.att-preview { margin-top: 4px; }
.send-actions { display: flex; gap: 10px; margin-top: 10px; }
:deep(.el-form-item__label) { font-weight: 600; color: var(--cm-text-2); }
</style>
