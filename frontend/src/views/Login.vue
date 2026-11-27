<template>
  <div class="login-page">
    <div class="login-box">
      <div class="login-brand">
        <div class="logo"><el-icon><Message /></el-icon></div>
        <h1>Cloud Mail</h1>
        <p>轻盈 · 现代的云端邮箱</p>
      </div>

      <div class="login-card cm-card cm-pad">
        <el-tabs v-model="tab" stretch>
          <el-tab-pane label="登录" name="login">
            <el-form @submit.prevent="doLogin" label-position="top">
              <el-form-item label="邮箱">
                <el-input v-model="login.email" size="large" placeholder="you@example.com" :prefix-icon="Message" autocomplete="username" />
              </el-form-item>
              <el-form-item label="密码">
                <el-input v-model="login.password" type="password" size="large" show-password placeholder="请输入密码" :prefix-icon="Lock" autocomplete="current-password" @keyup.enter="doLogin" />
              </el-form-item>
              <el-button class="cm-btn-primary" style="width:100%;height:44px" :loading="busy" @click="doLogin">登 录</el-button>
            </el-form>
          </el-tab-pane>

          <el-tab-pane label="注册" name="register" v-if="config.register !== 1">
            <el-form label-position="top">
              <el-form-item label="邮箱">
                <el-input v-model="reg.email" size="large" placeholder="you@example.com" :prefix-icon="Message" />
              </el-form-item>
              <el-form-item label="密码">
                <el-input v-model="reg.password" type="password" size="large" show-password placeholder="6–30 位" :prefix-icon="Lock" />
              </el-form-item>
              <el-form-item v-if="config.regKey !== 1" :label="config.regKey === 2 ? '注册码（选填）' : '注册码'">
                <el-input v-model="reg.code" size="large" placeholder="如有注册码请填写" :prefix-icon="Key" />
              </el-form-item>
              <el-button class="cm-btn-primary" style="width:100%;height:44px" :loading="busy" @click="doRegister">注 册</el-button>
            </el-form>
          </el-tab-pane>
        </el-tabs>
      </div>

      <div class="login-foot" v-if="config.title">{{ config.title }}</div>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { ElMessage } from 'element-plus';
import { Message, Lock, Key } from '@element-plus/icons-vue';
import { api, setToken } from '../api';

const router = useRouter();
const tab = ref('login');
const busy = ref(false);
const config = ref({});
const login = reactive({ email: '', password: '', token: '' });
const reg = reactive({ email: '', password: '', code: '', token: '' });

onMounted(async () => {
  try { config.value = await api.get('/api/setting/websiteConfig', {}, { auth: false }); }
  catch { /* 网站配置可选 */ }
});

const doLogin = async () => {
  if (!login.email || !login.password) return ElMessage.warning('请输入邮箱和密码');
  busy.value = true;
  try {
    const data = await api.post('/api/login', { email: login.email, password: login.password, token: login.token }, { auth: false });
    if (!data || !data.token) throw new Error('登录响应缺少 token，请稍后重试');
    setToken(data.token);
    ElMessage.success('登录成功');
    router.push('/inbox');
  } catch (e) {
    ElMessage.error(e?.message || '登录失败，请稍后重试');
  } finally { busy.value = false; }
};

const doRegister = async () => {
  if (!reg.email || !reg.password) return ElMessage.warning('请填写邮箱和密码');
  busy.value = true;
  try {
    await api.post('/api/register', { email: reg.email, password: reg.password, code: reg.code, token: reg.token }, { auth: false });
    ElMessage.success('注册成功，请登录');
    tab.value = 'login';
    login.email = reg.email;
  } catch (e) {
    ElMessage.error(e?.message || '注册失败，请稍后重试');
  } finally { busy.value = false; }
};
</script>

<style scoped>
.login-foot { text-align: center; color: var(--cm-text-3); font-size: 12px; margin-top: 18px; }
:deep(.el-tabs__header) { margin-bottom: 20px; }
:deep(.el-form-item__label) { font-weight: 600; color: var(--cm-text-2); }
</style>
