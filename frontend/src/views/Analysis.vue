<template>
  <div class="cm-page">
    <div class="cm-page-head">
      <div><h2 class="cm-title">数据分析</h2><div class="cm-subtitle">近 15 天邮件与用户统计</div></div>
      <div class="tz">
        <span>时区</span>
        <el-select v-model="timeZone" style="width:170px" @change="load">
          <el-option label="北京时间 (UTC+8)" value="Asia/Shanghai" />
          <el-option label="UTC" value="UTC" />
          <el-option label="东京 (UTC+9)" value="Asia/Tokyo" />
          <el-option label="伦敦" value="Europe/London" />
          <el-option label="纽约" value="America/New_York" />
        </el-select>
      </div>
    </div>

    <div v-loading="loading">
      <div class="stat-grid">
        <div class="cm-card stat-card"><div class="label">收件总数</div><div class="value">{{ n.receiveTotal }}</div><div class="ico"><el-icon><Message /></el-icon></div></div>
        <div class="cm-card stat-card c2"><div class="label">发件总数</div><div class="value">{{ n.sendTotal }}</div><div class="ico"><el-icon><Promotion /></el-icon></div></div>
        <div class="cm-card stat-card c3"><div class="label">用户总数</div><div class="value">{{ n.userTotal }}</div><div class="ico"><el-icon><User /></el-icon></div></div>
        <div class="cm-card stat-card c4"><div class="label">今日已发</div><div class="value">{{ data.daySendTotal }}</div><div class="ico"><el-icon><TrendCharts /></el-icon></div></div>
      </div>

      <div class="chart-grid">
        <div class="cm-card cm-pad chart-card">
          <div class="chart-title">每日收件趋势</div>
          <BarChart :rows="data.emailDayCount?.receiveDayCount || []" color="#4f6ef7" />
        </div>
        <div class="cm-card cm-pad chart-card">
          <div class="chart-title">每日发件趋势</div>
          <BarChart :rows="data.emailDayCount?.sendDayCount || []" color="#22c55e" />
        </div>
        <div class="cm-card cm-pad chart-card">
          <div class="chart-title">新增用户趋势</div>
          <BarChart :rows="data.userDayCount || []" color="#8b5cf6" />
        </div>
        <div class="cm-card cm-pad chart-card">
          <div class="chart-title">高频发件人 Top 6</div>
          <div v-if="(data.receiveRatio?.nameRatio || []).length" class="rank-list">
            <div v-for="(r,i) in data.receiveRatio.nameRatio" :key="i" class="rank-row">
              <span class="rank-no">{{ i+1 }}</span>
              <span class="rank-name">{{ r.name }}</span>
              <div class="rank-bar"><div :style="{width: barWidth(r.total, maxRatio)}"></div></div>
              <span class="rank-val">{{ r.total }}</span>
            </div>
          </div>
          <el-empty v-else description="暂无数据" :image-size="60" />
        </div>
      </div>

      <div class="cm-card cm-pad detail-stats">
        <div class="chart-title">邮件状态明细</div>
        <div class="kv-grid">
          <div v-for="(v,k) in detailItems" :key="k"><span>{{ k }}</span><strong>{{ v }}</strong></div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, computed, onMounted } from 'vue';
import { ElMessage } from 'element-plus';
import { api } from '../api';
import BarChart from '../components/BarChart.vue';

const timeZone = ref('Asia/Shanghai');
const loading = ref(false);
const data = ref({});
const n = reactive({});

const maxRatio = computed(() => Math.max(1, ...(data.value.receiveRatio?.nameRatio || []).map(r => r.total)));
const barWidth = (v, max) => `${Math.max(3, v / max * 100)}%`;
const detailItems = computed(() => {
  const src = data.value.numberCount || {};
  const map = {
    normalReceiveTotal: '正常收件', normalSendTotal: '正常发件',
    delReceiveTotal: '删除收件', delSendTotal: '删除发件',
    normalUserTotal: '正常用户', delUserTotal: '删除用户',
    normalAccountTotal: '正常账号', delAccountTotal: '删除账号',
  };
  const out = {};
  Object.entries(map).forEach(([k, label]) => { if (k in src) out[label] = src[k]; });
  return out;
});

const load = async () => {
  loading.value = true;
  try {
    data.value = await api.get('/api/analysis/echarts', { timeZone: timeZone.value });
    Object.assign(n, data.value.numberCount || {});
  } catch (e) { ElMessage.error(e.message); }
  finally { loading.value = false; }
};
onMounted(load);
</script>

<style scoped>
.tz { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--cm-text-2); }
.chart-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 16px; margin-top: 16px; }
.chart-card { min-height: 260px; }
.chart-title { font-size: 14px; font-weight: 700; margin-bottom: 18px; }
.detail-stats { margin-top: 16px; }
.kv-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px,1fr)); gap: 12px; }
.kv-grid > div { display: flex; justify-content: space-between; background: var(--cm-surface-soft); padding: 10px 14px; border-radius: 10px; font-size: 13px; }
.kv-grid span { color: var(--cm-text-2); }
.rank-list { display: flex; flex-direction: column; gap: 10px; }
.rank-row { display: flex; align-items: center; gap: 10px; font-size: 13px; }
.rank-no { width: 20px; height: 20px; border-radius: 6px; background: var(--cm-primary-soft); color: var(--cm-primary); display: grid; place-items: center; font-size: 11px; font-weight: 700; }
.rank-name { width: 90px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.rank-bar { flex: 1; height: 8px; background: var(--cm-surface-soft); border-radius: 8px; overflow: hidden; }
.rank-bar > div { height: 100%; background: linear-gradient(90deg,#4f6ef7,#8b5cf6); border-radius: 8px; transition: width .4s; }
.rank-val { width: 28px; text-align: right; color: var(--cm-text-2); }
</style>
