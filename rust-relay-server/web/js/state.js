// state.js — 共享状态单例（ES Module）
// 所有模块共享此状态，无全局变量污染

export const state = {
  /** sessionId -> agent object */
  agents: new Map(),

  /** 分屏布局状态 */
  layout: {
    cols: 1,              // 1 | 2 | 3
    panes: [null, null, null], // 每栏绑定的 sessionId
    activeMobilePane: 0,  // 移动端当前激活的面板序号
  },

  /** 当前激活的 pane index（键盘导航用） */
  activePane: 0,
};

/**
 * 注册或更新 agent
 * @param {string} sessionId
 * @param {object} data  { name, status, messages, todos, ws, pendingHitl, pendingAskUser, maxSeq }
 */
export function upsertAgent(sessionId, data) {
  if (state.agents.has(sessionId)) {
    const existing = state.agents.get(sessionId);
    // 保留历史消息和 maxSeq，合并新数据
    state.agents.set(sessionId, {
      ...existing,
      ...data,
      messages: data.messages ?? existing.messages,
      todos: data.todos ?? existing.todos,
      maxSeq: data.maxSeq ?? existing.maxSeq ?? 0,
    });
  } else {
    state.agents.set(sessionId, {
      name: data.name || sessionId.slice(0, 8),
      status: data.status || 'offline',
      messages: data.messages || [],
      todos: data.todos || [],
      ws: data.ws || null,
      pendingHitl: data.pendingHitl || null,
      pendingAskUser: data.pendingAskUser || null,
      maxSeq: data.maxSeq || 0,
      isRunning: data.isRunning ?? false,
    });
  }
}

export function getAgent(sessionId) {
  return state.agents.get(sessionId) || null;
}

export function removeAgent(sessionId) {
  state.agents.delete(sessionId);
}

export function setPaneAgent(paneIdx, sessionId) {
  if (paneIdx >= 0 && paneIdx < 3) {
    state.layout.panes[paneIdx] = sessionId;
  }
}

export function clearPane(paneIdx) {
  if (paneIdx >= 0 && paneIdx < 3) {
    state.layout.panes[paneIdx] = null;
  }
}
