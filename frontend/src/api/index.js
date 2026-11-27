// 统一 API 层：严格对齐后端契约
// - 所有响应 HTTP 恒 200，业务成败看 body.code === 200
// - Authorization 是裸 JWT，无 Bearer 前缀
// - 401 统一清 token 并跳登录

const TOKEN_KEY = 'cm_token';

export const getToken = () => localStorage.getItem(TOKEN_KEY) || '';
export const setToken = (t) => t ? localStorage.setItem(TOKEN_KEY, t) : localStorage.removeItem(TOKEN_KEY);

export class ApiError extends Error {
  constructor(message, code) {
    super(message);
    this.code = code;
  }
}

// 错误码 -> 中文提示（后端返回的是错误键）
const ERROR_TEXT = {
  IncorrectPwd: '邮箱或密码错误',
  emailAndPwdEmpty: '请输入邮箱和密码',
  loginLocked: '失败次数过多，请稍后再试',
  regDisabled: '注册已关闭',
  notEmail: '邮箱格式不正确',
  notEmailDomain: '邮箱域名不在允许范围',
  pwdMinLength: '密码长度需 6–30 位',
  isRegAccount: '该邮箱已存在',
  emptyRecipient: '请填写收件人',
  disabledSend: '发信功能已关闭',
  bannedSend: '发信被限制',
  onlyInternalSend: '当前仅允许站内发信',
  noSendProvider: '未配置发信通道',
  daySendLimit: '今日发信额度用尽',
  totalSendLimit: '发信额度用尽',
  recipientLimit: '收件人数量超限',
  authExpired: '登录已过期，请重新登录',
  unauthorized: '没有权限执行此操作',
  rateLimited: '操作太频繁，请稍后再试',
  emptyRoleName: '角色名不能为空',
  emptyRegKey: '注册码不能为空',
  isExistRegKye: '注册码已存在',
  roleNotExist: '角色不存在',
  delDefRole: '不能删除默认角色',
};

export const errText = (key, code) => ERROR_TEXT[key] || key || `请求失败(${code})`;

async function request(path, { method = 'GET', query, body, auth = true, raw = false } = {}) {
  let url = path;
  if (query && Object.keys(query).length) {
    const qs = new URLSearchParams();
    Object.entries(query).forEach(([k, v]) => {
      if (v !== undefined && v !== null && v !== '') qs.append(k, String(v));
    });
    url += (url.includes('?') ? '&' : '?') + qs.toString();
  }
  const headers = {};
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (auth && getToken()) headers['Authorization'] = getToken();

  let res;
  try {
    res = await fetch(url, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  } catch (e) {
    throw new ApiError('网络连接失败，请检查服务是否运行', -1);
  }

  if (raw) return res;

  let json;
  try {
    json = await res.json();
  } catch (e) {
    throw new ApiError('服务返回异常', res.status);
  }

  if (json.code === 200) return json.data;

  if (json.code === 401) {
    setToken('');
    if (!location.hash.includes('/login')) location.hash = '#/login';
  }
  throw new ApiError(errText(json.message, json.code), json.code);
}

export const api = {
  get: (path, query, opts = {}) => request(path, { method: 'GET', query, ...opts }),
  post: (path, body, opts = {}) => request(path, { method: 'POST', body, ...opts }),
  put: (path, body, opts = {}) => request(path, { method: 'PUT', body, ...opts }),
  del: (path, query, opts = {}) => request(path, { method: 'DELETE', query, ...opts }),
  delBody: (path, body, opts = {}) => request(path, { method: 'DELETE', body, ...opts }),
};

// 附件 / 对象存储访问地址
export const assetUrl = (key) => {
  if (!key) return '';
  if (/^https?:\/\//i.test(key) || key.startsWith('data:')) return key;
  return '/api/' + String(key).replace(/^\/+/, '');
};
