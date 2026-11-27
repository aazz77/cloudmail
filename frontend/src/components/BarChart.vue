<template>
  <div class="bar-chart">
    <div v-if="!rows.length" class="empty">暂无数据</div>
    <div v-else class="bars">
      <div v-for="r in rows" :key="r.date" class="bar-col" :title="`${r.date}: ${r.total}`">
        <div class="bar-track">
          <div class="bar-fill" :style="{ height: heightPct(r.total), background: color }"></div>
        </div>
        <div class="bar-label">{{ shortDate(r.date) }}</div>
        <div class="bar-val">{{ r.total }}</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';
const props = defineProps({
  rows: { type: Array, default: () => [] },
  color: { type: String, default: '#4f6ef7' },
});
const max = computed(() => Math.max(1, ...props.rows.map(r => r.total)));
const heightPct = (v) => `${Math.max(2, v / max.value * 100)}%`;
const shortDate = (s) => (s || '').slice(5);
</script>

<style scoped>
.bar-chart { height: 190px; }
.empty { color: var(--cm-text-3); text-align: center; padding-top: 70px; font-size: 13px; }
.bars { display: flex; align-items: flex-end; gap: 4px; height: 100%; overflow-x: auto; }
.bar-col { flex: 1 0 24px; display: flex; flex-direction: column; align-items: center; height: 100%; min-width: 24px; }
.bar-track { flex: 1; width: 100%; display: flex; align-items: flex-end; justify-content: center; }
.bar-fill { width: 70%; max-width: 22px; border-radius: 5px 5px 2px 2px; transition: height .4s ease; min-height: 2px; opacity: .85; }
.bar-fill:hover { opacity: 1; }
.bar-label { font-size: 9px; color: var(--cm-text-3); margin-top: 6px; white-space: nowrap; }
.bar-val { font-size: 10px; color: var(--cm-text-2); font-weight: 600; }
</style>
