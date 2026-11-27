import { createRouter, createWebHashHistory } from 'vue-router';
import { getToken } from './api';

const routes = [
  { path: '/login', name: 'login', component: () => import('./views/Login.vue'), meta: { public: true } },
  { path: '/', redirect: '/inbox' },
  { path: '/inbox', name: 'inbox', component: () => import('./views/Inbox.vue'), meta: { title: '收件箱' } },
  { path: '/sent', name: 'sent', component: () => import('./views/Sent.vue'), meta: { title: '发件箱' } },
  { path: '/star', name: 'star', component: () => import('./views/Star.vue'), meta: { title: '星标邮件' } },
  { path: '/all', name: 'all', component: () => import('./views/AllMail.vue'), meta: { title: '全部邮件', perm: 'mail' } },
  { path: '/send', name: 'send', component: () => import('./views/Send.vue'), meta: { title: '写邮件' } },
  { path: '/accounts', name: 'accounts', component: () => import('./views/Accounts.vue'), meta: { title: '邮箱账号' } },
  { path: '/analysis', name: 'analysis', component: () => import('./views/Analysis.vue'), meta: { title: '数据分析', perm: 'analysis' } },
  { path: '/users', name: 'users', component: () => import('./views/Users.vue'), meta: { title: '用户管理', perm: 'user' } },
  { path: '/roles', name: 'roles', component: () => import('./views/Roles.vue'), meta: { title: '角色权限', perm: 'role' } },
  { path: '/regkeys', name: 'regkeys', component: () => import('./views/RegKeys.vue'), meta: { title: '注册码', perm: 'regkey' } },
  { path: '/settings', name: 'settings', component: () => import('./views/Settings.vue'), meta: { title: '系统设置', perm: 'setting' } },
  { path: '/profile', name: 'profile', component: () => import('./views/Profile.vue'), meta: { title: '个人中心' } },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

router.beforeEach((to) => {
  if (to.meta.public) return true;
  if (!getToken()) return { name: 'login' };
  return true;
});
