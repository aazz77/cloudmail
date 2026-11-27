import { createApp } from 'vue';
import ElementPlus from 'element-plus';
import 'element-plus/dist/index.css';
import zhCn from 'element-plus/es/locale/lang/zh-cn';
import * as Icons from '@element-plus/icons-vue';

import App from './App.vue';
import { router } from './router';
import './styles/theme.css';

const app = createApp(App);

// 全局注册图标组件（Element Plus 图标名如 Edit / Delete 直接可用）
Object.entries(Icons).forEach(([name, comp]) => app.component(name, comp));

app.use(ElementPlus, { locale: zhCn });
app.use(router);
app.mount('#app');
