<template>
  <div class="mail-list">
    <div v-if="!list.length" class="cm-empty">
      <div class="big">📭</div>
      <div>{{ emptyText }}</div>
    </div>
    <div
      v-for="m in list" :key="m.emailId"
      class="mail-item" :class="{ unread: !m.unread, starred: m.isStar }"
      @click="$emit('open', m)"
    >
      <div class="mail-avatar" :class="'g' + ((m.emailId % 4) + 1)">{{ (m.name || m.sendEmail || '?')[0].toUpperCase() }}</div>
      <div class="mail-body">
        <div class="mail-row1">
          <span class="mail-from">{{ m.name || m.sendEmail }}</span>
          <span class="mail-time">{{ shortTime(m.createTime) }}</span>
        </div>
        <div class="mail-subject">
          <span v-if="m.type === 1" class="tag">[已发送] </span>{{ m.subject || '（无主题）' }}
        </div>
        <div class="mail-preview">{{ m.listText || plainText(m.text || m.content) || '（无正文）' }}</div>
      </div>
      <el-icon class="mail-star" @click.stop="$emit('star', m)" :title="m.isStar ? '取消星标' : '加星标'">
        <StarFilled v-if="m.isStar" /><Star v-else />
      </el-icon>
    </div>
  </div>
</template>

<script setup>
defineProps({
  list: { type: Array, default: () => [] },
  emptyText: { type: String, default: '暂无邮件' },
});
defineEmits(['open', 'star']);

const shortTime = (s) => {
  if (!s) return '';
  const d = new Date(s.replace(' ', 'T'));
  if (isNaN(d)) return s.slice(5, 16);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) return d.toTimeString().slice(0, 5);
  return `${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
};
const plainText = (html) => {
  if (!html) return '';
  return String(html).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 200);
};
</script>
