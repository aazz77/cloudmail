<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">系统设置</h2><div class="cm-subtitle">站点、收发、验证与第三方集成</div></div>
      <el-button class="cm-btn-primary" :loading="saving" @click="save"><el-icon><Check /></el-icon>&nbsp;保存设置</el-button>
    </div>

    <div v-loading="loading">
      <el-tabs v-model="tab" class="settings-tabs">
        <!-- 站点 -->
        <el-tab-pane label="站点" name="site">
          <div class="cm-card cm-pad form-grid">
            <Field label="站点标题" v-model="f.title" />
            <Field label="公告标题" v-model="f.noticeTitle" />
            <Field label="公告内容" v-model="f.noticeContent" />
            <Field label="通知时长(ms)" v-model="f.noticeDuration" type="number" />
            <Field label="登录页透明度" v-model="f.loginOpacity" type="number" />
            <div class="field"><label>公告位置</label><el-select v-model="f.noticePosition" style="width:100%">
              <el-option label="顶部" value="top" /><el-option label="底部" value="bottom" /><el-option label="居中" value="center" />
            </el-select></div>
            <Field label="背景图 URL" v-model="f.background" placeholder="留空使用默认" />
            <div class="field"><label>&nbsp;</label><div class="row-btns">
              <el-button size="small" @click="saveBackground">应用背景</el-button>
              <el-button size="small" plain @click="delBackground">清除背景</el-button>
            </div></div>
          </div>
        </el-tab-pane>

        <!-- 注册与收发 -->
        <el-tab-pane label="注册与收发" name="reg">
          <div class="cm-card cm-pad form-grid">
            <div class="field"><label>注册开关</label><el-select v-model="f.register" style="width:100%">
              <el-option label="开放注册" :value="0" /><el-option label="关闭注册" :value="1" />
            </el-select></div>
            <div class="field"><label>注册码模式</label><el-select v-model="f.regKey" style="width:100%">
              <el-option label="关闭" :value="1" /><el-option label="必须使用" :value="0" /><el-option label="可选" :value="2" />
            </el-select></div>
            <div class="field"><label>添加邮箱</label><el-select v-model="f.addEmail" style="width:100%">
              <el-option label="开启" :value="0" /><el-option label="关闭" :value="1" />
            </el-select></div>
            <div class="field"><label>多邮箱</label><el-select v-model="f.manyEmail" style="width:100%">
              <el-option label="开启" :value="0" /><el-option label="关闭" :value="1" />
            </el-select></div>
            <div class="field"><label>自动刷新</label><el-select v-model="f.autoRefresh" style="width:100%">
              <el-option label="开启" :value="2" /><el-option label="关闭" :value="1" />
            </el-select></div>
            <Field label="邮箱前缀最短长度" v-model="f.minEmailPrefix" type="number" />
            <Field label="邮箱前缀过滤（逗号分隔）" v-model="prefixFilterStr" placeholder="如 admin, root" />
            <Field label="发信总开关" v-model="f.send" type="number" placeholder="0 开启 / 1 关闭" />
            <Field label="接收邮件" v-model="f.receive" type="number" placeholder="0 开启 / 1 关闭" />
            <div class="field"><label>同步删除</label><el-select v-model="f.syncDelete" style="width:100%">
              <el-option label="物理删除" :value="0" /><el-option label="软删除" :value="1" />
            </el-select></div>
          </div>
        </el-tab-pane>

        <!-- 验证 -->
        <el-tab-pane label="人机验证" name="verify">
          <div class="cm-card cm-pad form-grid">
            <div class="field"><label>登录验证</label><el-select v-model="f.loginVerify" style="width:100%">
              <el-option label="关闭" :value="1" /><el-option label="每次验证" :value="0" /><el-option label="计数后验证" :value="2" />
            </el-select></div>
            <Field label="登录免费次数" v-model="f.loginVerifyCount" type="number" />
            <div class="field"><label>注册验证</label><el-select v-model="f.registerVerify" style="width:100%">
              <el-option label="关闭" :value="1" /><el-option label="每次验证" :value="0" /><el-option label="计数后验证" :value="2" />
            </el-select></div>
            <Field label="注册免费次数" v-model="f.regVerifyCount" type="number" />
            <div class="field"><label>加邮箱验证</label><el-select v-model="f.addEmailVerify" style="width:100%">
              <el-option label="关闭" :value="1" /><el-option label="每次验证" :value="0" /><el-option label="计数后验证" :value="2" />
            </el-select></div>
            <Field label="加邮箱免费次数" v-model="f.addVerifyCount" type="number" />
            <Field label="Turnstile Site Key" v-model="f.siteKey" />
            <Field label="Turnstile Secret Key" v-model="secretKeyInput" placeholder="留空表示不修改" type="password" />
          </div>
        </el-tab-pane>

        <!-- 黑名单 -->
        <el-tab-pane label="黑名单" name="blacklist">
          <div class="cm-card cm-pad form-grid">
            <Field label="主题黑名单" v-model="f.blackSubject" placeholder="逗号分隔" />
            <Field label="正文黑名单" v-model="f.blackContent" placeholder="逗号分隔" />
            <Field label="发件人黑名单" v-model="f.blackFrom" placeholder="逗号分隔" />
            <div class="field full"><el-button class="cm-btn-primary" @click="saveBlacklist">保存黑名单</el-button></div>
          </div>
        </el-tab-pane>

        <!-- 通知 -->
        <el-tab-pane label="Telegram" name="tg">
          <div class="cm-card cm-pad form-grid">
            <Field label="Bot Token" v-model="tgTokenInput" placeholder="留空表示不修改" type="password" />
            <Field label="Chat ID" v-model="f.tgChatId" />
            <div class="field"><label>Bot 状态</label><el-select v-model="f.tgBotStatus" style="width:100%">
              <el-option label="开启" :value="0" /><el-option label="关闭" :value="1" />
            </el-select></div>
            <Field label="转发邮箱" v-model="f.forwardEmail" />
            <div class="field"><label>转发状态</label><el-select v-model="f.forwardStatus" style="width:100%">
              <el-option label="开启" :value="0" /><el-option label="关闭" :value="1" />
            </el-select></div>
          </div>
        </el-tab-pane>

        <!-- 第三方登录 -->
        <el-tab-pane label="第三方登录" name="oauth">
          <div class="cm-card cm-pad">
            <div v-for="p in oauthProviders" :key="p.key" class="oauth-block">
              <div class="oauth-head">
                <strong>{{ p.label }}</strong>
                <el-switch v-model="f[p.switch]" :active-value="0" :inactive-value="1" />
              </div>
              <div class="form-grid">
                <Field label="Client ID" v-model="f[p.id]" />
                <Field :label="p.secretLabel" v-model="p.secret" placeholder="留空表示不修改" type="password" />
              </div>
            </div>
            <div class="tip">密钥留空不会覆盖已保存的值。回调地址请在第三方平台配置为 <code>{{ callbackUrl }}</code></div>
          </div>
        </el-tab-pane>

        <!-- 存储 -->
        <el-tab-pane label="存储与发信" name="storage">
          <div class="cm-card cm-pad form-grid">
            <Field label="R2 域名" v-model="f.r2Domain" />
            <Field label="自定义域名" v-model="f.customDomain" />
            <Field label="Bucket" v-model="f.bucket" />
            <Field label="Region" v-model="f.region" />
            <Field label="Endpoint" v-model="f.endpoint" />
            <Field label="S3 Access Key" v-model="s3AccessInput" type="password" placeholder="留空不修改" />
            <Field label="S3 Secret Key" v-model="s3SecretInput" type="password" placeholder="留空不修改" />
            <div class="field"><label>Force Path Style</label><el-switch v-model="f.forcePathStyle" :active-value="1" :inactive-value="0" /></div>
            <Field label="Webhook URL" v-model="f.webhookUrl" />
            <Field label="Webhook Secret" v-model="webhookSecretInput" type="password" placeholder="留空不修改" />
            <div class="field"><label>Webhook 状态</label><el-select v-model="f.webhookStatus" style="width:100%">
              <el-option label="开启" :value="0" /><el-option label="关闭" :value="1" />
            </el-select></div>
          </div>
          <div class="tip">注意：本程序发信通过 <code>config.toml</code> 的 <code>[smtp]</code> 配置，不在此处设置。</div>
        </el-tab-pane>
      </el-tabs>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, computed, onMounted } from 'vue';
import { ElMessage } from 'element-plus';
import { api } from '../api';
import Field from '../components/Field.vue';

const tab = ref('site');
const loading = ref(false);
const saving = ref(false);
const f = reactive({});
const prefixFilterStr = ref('');
const secretKeyInput = ref('');
const tgTokenInput = ref('');
const s3AccessInput = ref('');
const s3SecretInput = ref('');
const webhookSecretInput = ref('');
const callbackUrl = computed(() => `${location.origin}/api/oauth/callback`);

const oauthProviders = reactive([
  { key:'linuxdo', label:'LinuxDo', id:'linuxdoClientId', switch:'linuxdoSwitch', secret:'', secretLabel:'Client Secret' },
  { key:'github', label:'GitHub', id:'githubClientId', switch:'githubSwitch', secret:'', secretLabel:'Client Secret' },
  { key:'google', label:'Google', id:'googleClientId', switch:'googleSwitch', secret:'', secretLabel:'Client Secret' },
]);

const load = async () => {
  loading.value = true;
  try {
    const d = await api.get('/api/setting/query');
    Object.assign(f, d);
    prefixFilterStr.value = Array.isArray(d.emailPrefixFilter) ? d.emailPrefixFilter.join(', ') : '';
    oauthProviders.forEach(p => { p.secret = ''; });
  } catch (e) { ElMessage.error(e.message); }
  finally { loading.value = false; }
};
onMounted(load);

// 白名单字段：只提交后端认识且用户确实改动的键；脱敏密钥仅当用户输入了新值才提交
const buildBody = () => {
  const keys = [
    'title','noticeTitle','noticeContent','noticeType','noticeDuration','noticePosition','noticeWidth','noticeOffset','loginOpacity','background',
    'register','regKey','addEmail','manyEmail','autoRefresh','minEmailPrefix','send','receive','syncDelete','loginDomain',
    'loginVerify','loginVerifyCount','registerVerify','regVerifyCount','addEmailVerify','addVerifyCount','siteKey',
    'blackSubject','blackContent','blackFrom','tgChatId','tgBotStatus','forwardEmail','forwardStatus',
    'linuxdoClientId','linuxdoSwitch','githubClientId','githubSwitch','googleClientId','googleSwitch',
    'r2Domain','customDomain','bucket','region','endpoint','forcePathStyle','webhookUrl','webhookStatus','webhookRetry',
  ];
  const body = {};
  keys.forEach(k => { if (k in f) body[k] = f[k]; });
  if (prefixFilterStr.value) body.emailPrefixFilter = prefixFilterStr.value.split(',').map(s=>s.trim()).filter(Boolean);
  if (secretKeyInput.value) body.secretKey = secretKeyInput.value;
  if (tgTokenInput.value) body.tgBotToken = tgTokenInput.value;
  if (s3AccessInput.value) body.s3AccessKey = s3AccessInput.value;
  if (s3SecretInput.value) body.s3SecretKey = s3SecretInput.value;
  if (webhookSecretInput.value) body.webhookSecret = webhookSecretInput.value;
  // OAuth secret 仅当用户输入了新值才提交（脱敏值不可回写）
  if (oauthProviders[0].secret) body.linuxdoClientSecret = oauthProviders[0].secret;
  if (oauthProviders[1].secret) body.githubClientSecret = oauthProviders[1].secret;
  if (oauthProviders[2].secret) body.googleClientSecret = oauthProviders[2].secret;
  Object.keys(body).forEach(k => body[k] === undefined && delete body[k]);
  return body;
};

const save = async () => {
  saving.value = true;
  try {
    await api.put('/api/setting/set', buildBody());
    ElMessage.success('设置已保存');
    secretKeyInput.value = ''; tgTokenInput.value = ''; s3AccessInput.value = ''; s3SecretInput.value = ''; webhookSecretInput.value = '';
    oauthProviders.forEach(p => p.secret = '');
    load();
  } catch (e) { ElMessage.error(e.message); }
  finally { saving.value = false; }
};
const saveBlacklist = async () => {
  try {
    const d = await api.put('/api/setting/setBlacklist', { blackSubject: f.blackSubject, blackContent: f.blackContent, blackFrom: f.blackFrom });
    Object.assign(f, d); ElMessage.success('黑名单已保存');
  } catch (e) { ElMessage.error(e.message); }
};
const saveBackground = async () => {
  try { const key = await api.put('/api/setting/setBackground', { background: f.background || '' }); f.background = key; ElMessage.success('背景已应用'); }
  catch (e) { ElMessage.error(e.message); }
};
const delBackground = async () => {
  try { await api.del('/api/setting/deleteBackground'); f.background = ''; ElMessage.success('背景已清除'); }
  catch (e) { ElMessage.error(e.message); }
};
</script>

<style scoped>
.form-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px,1fr)); gap: 16px 20px; }
.field { display: flex; flex-direction: column; gap: 7px; }
.field.full { grid-column: 1/-1; }
.field label { font-size: 13px; font-weight: 600; color: var(--cm-text-2); }
.row-btns { display: flex; gap: 8px; }
.tip { margin-top: 16px; font-size: 12px; color: var(--cm-text-2); line-height: 1.7; }
.tip code { background: var(--cm-primary-soft); color: var(--cm-primary); padding: 2px 6px; border-radius: 6px; }
.oauth-block { padding: 16px 0; border-bottom: 1px solid var(--cm-border); }
.oauth-block:last-of-type { border-bottom: none; }
.oauth-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.settings-tabs :deep(.el-tabs__header) { margin-bottom: 18px; }
</style>
