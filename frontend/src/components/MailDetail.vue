<template>
  <el-drawer v-model="visible" :size="drawerSize" :title="mail.subject || '（无主题）'" destroy-on-close>
    <template v-if="mail.emailId">
      <div class="detail-head">
        <div class="detail-row"><span class="k">发件人</span><span class="v">{{ displayFrom }}</span></div>
        <div class="detail-row" v-if="toEmails.length"><span class="k">收件人</span><span class="v">{{ toEmails.join('、') }}</span></div>
        <div class="detail-row" v-if="ccEmails.length"><span class="k">抄送</span><span class="v">{{ ccEmails.join('、') }}</span></div>
        <div class="detail-row"><span class="k">时间</span><span class="v">{{ mail.createTime }}</span></div>
        <div class="detail-row" v-if="mail.code"><span class="k">提取码</span><span class="v code">{{ mail.code }}</span></div>
      </div>

      <div class="detail-actions">
        <el-button :type="mail.isStar ? 'warning' : 'default'" plain @click="$emit('star', mail)">
          <el-icon><Star /></el-icon>&nbsp;{{ mail.isStar ? '取消星标' : '加星标' }}
        </el-button>
        <el-button v-if="mail.type === 0" @click="$emit('reply', mail)"><el-icon><RefreshLeft /></el-icon>&nbsp;回复</el-button>
        <el-button type="danger" plain @click="$emit('delete', mail)"><el-icon><Delete /></el-icon>&nbsp;删除</el-button>
      </div>

      <div v-if="atts.length" class="att-section">
        <div class="sec-title">附件 ({{ atts.length }})</div>
        <div class="att-list">
          <a v-for="a in atts" :key="a.attId" :href="assetUrl(a.key)" target="_blank" class="att-chip">
            <el-icon><Paperclip /></el-icon>
            <span>{{ a.filename || '附件' }}</span>
            <small>{{ fmtSize(a.size) }}</small>
          </a>
        </div>
      </div>

      <div class="sec-title">正文</div>
      <div v-if="mail.content" class="mail-content" v-html="mail.content"></div>
      <pre v-else-if="mail.text" class="mail-text">{{ mail.text }}</pre>
      <el-empty v-else description="无正文" :image-size="80" />
    </template>
  </el-drawer>
</template>

<script setup>
import { computed, ref, watch } from 'vue';
import { assetUrl } from '../api';

const props = defineProps({
  modelValue: Boolean,
  mail: { type: Object, default: () => ({}) },
  isMobile: Boolean,
});
const emit = defineEmits(['update:modelValue', 'star', 'reply', 'delete']);

const visible = computed({
  get: () => props.modelValue,
  set: (v) => emit('update:modelValue', v),
});
const drawerSize = computed(() => props.isMobile ? '100%' : '560px');

const parseList = (s) => {
  try {
    const arr = JSON.parse(s || '[]');
    return arr.map(x => x.address || x.name).filter(Boolean);
  } catch { return []; }
};
const toEmails = computed(() => parseList(props.mail.toEmail ? JSON.stringify([{address: props.mail.toEmail}]) : '[]'));
const ccEmails = computed(() => parseList(props.mail.cc));
const displayFrom = computed(() => {
  const n = props.mail.name;
  return n ? `${n} <${props.mail.sendEmail}>` : (props.mail.sendEmail || '');
});
const atts = computed(() => props.mail.attList || []);
const fmtSize = (n) => {
  if (!n) return '';
  return n > 1024 * 1024 ? (n / 1024 / 1024).toFixed(1) + ' MB' : Math.ceil(n / 1024) + ' KB';
};
</script>

<style scoped>
.detail-head { background: var(--cm-surface-soft); border-radius: 12px; padding: 14px 16px; margin-bottom: 16px; }
.detail-row { display: flex; gap: 12px; font-size: 13px; padding: 4px 0; }
.detail-row .k { color: var(--cm-text-3); flex: 0 0 56px; }
.detail-row .v { color: var(--cm-text); word-break: break-all; }
.detail-row .v.code { font-weight: 700; color: var(--cm-primary); letter-spacing: 1px; }
.detail-actions { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 20px; }
.att-section { margin-bottom: 20px; }
.sec-title { font-size: 13px; font-weight: 700; color: var(--cm-text-2); margin: 18px 0 10px; }
.att-list { display: flex; flex-wrap: wrap; gap: 8px; }
.att-chip {
  display: inline-flex; align-items: center; gap: 6px; padding: 7px 11px;
  background: var(--cm-primary-soft); color: var(--cm-primary); border-radius: 10px;
  font-size: 12px; max-width: 100%;
}
.att-chip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 180px; }
.att-chip small { opacity: .7; }
.mail-content { font-size: 14px; line-height: 1.7; word-break: break-word; overflow-x: auto; }
.mail-content :deep(img) { max-width: 100%; height: auto; }
.mail-text { white-space: pre-wrap; word-break: break-word; font-family: inherit; font-size: 14px; line-height: 1.7; background: var(--cm-surface-soft); padding: 16px; border-radius: 12px; }
</style>
