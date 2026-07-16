import { create } from "zustand";
import { Store } from "@tauri-apps/plugin-store";
import i18n from "../i18n";
import { encryptApiKey, decryptApiKey } from "../lib/crypto";

// ========== 类型定义 ==========

export interface Conversation {
  id: string;
  title: string;
  createdAt: string;
  pinned?: boolean;
}

export interface Message {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  timestamp: string;
  /** 深度思考内容（DeepSeek reasoning_content / Claude thinking） */
  thinking?: string;
  /** 思考统计 */
  thinkingStats?: {
    tokens: number;
    durationMs: number;
  };
  /** Agent 工具调用步骤（持久化，可回顾历史工具调用） */
  toolSteps?: Array<{
    name: string;
    args: string;
    status: "running" | "done" | "error";
    result?: string;
    errorCode?: string | null;
    errorCategory?: string | null;
  }>;
  /** LLM 返回的工具调用（用于后续轮次上下文保持） */
  tool_calls?: Array<{
    id: string;
    name: string;
    arguments: string;
  }>;
  /** 工具调用结果的 ID 关联（tool 角色消息必填） */
  tool_call_id?: string;
}

export type ThemeMode = "system" | "light" | "dark";

/** 快捷键动作类型 */
export type ShortcutAction = "newConversation" | "focusInput" | "toggleModelPicker" | "openSettings";

/** 单个快捷键绑定（按键组合，如 ["ctrl", "n"]） */
export interface ShortcutBinding {
  keys: string[];
}

/** 所有快捷键的映射 */
export type ShortcutMap = Record<ShortcutAction, ShortcutBinding>;

/** 默认快捷键绑定 */
export const DEFAULT_SHORTCUTS: ShortcutMap = {
  newConversation:   { keys: ["ctrl", "n"] },
  focusInput:        { keys: ["ctrl", "l"] },
  toggleModelPicker: { keys: ["ctrl", "k"] },
  openSettings:      { keys: ["ctrl", ","] },
};

/** 一个模型提供商（如 OpenAI、DeepSeek、通义千问） */
export interface ModelProvider {
  id: string;          // 唯一 ID
  name: string;        // 显示名称，如 "OpenAI"
  apiBase: string;     // API 地址
  apiKey: string;      // API Key
  models: string[];    // 模型列表，如 ["gpt-4o", "gpt-4o-mini"]
  activeModel: string; // 当前选中的模型
}

/** MCP 服务器配置（持久化，运行时由 Rust 端连接子进程） */
export interface McpServerUI {
  name: string;
  command: string;
  args: string[];
  env?: Record<string, string>;
}

/** 用户自定义 Agent 配置 */
export interface AgentConfig {
  id: string;
  name: string;
  description: string;
  /** Agent 头像表情符号（🦊🤖🐻 等），无 emoji 时显示首字母 */
  icon: string;
  /** 系统提示词（核心行为定义） */
  systemPrompt: string;
  /** 启用的 Skill IDs */
  enabledSkillIds: string[];
  /** 启用的 MCP 服务器名称列表 */
  enabledMcpServerNames: string[];
  /** 绑定的知识库 ID（RAG） */
  knowledgeBaseId: string | null;
  /** 绑定的模型提供商 ID（null=使用全局默认） */
  providerId: string | null;
  /** 绑定的模型名（null=使用该提供商的 activeModel） */
  model: string | null;
  /** LLM 温度 */
  temperature: number;
  /** LLM 最大 Token */
  maxTokens: number;
  createdAt: string;
  updatedAt: string;
}

// ========== Store 类型 ==========

interface AppState {
  // 对话
  conversations: Conversation[];
  activeConversationId: string | null;
  messages: Record<string, Message[]>;

  // 设置
  theme: ThemeMode;
  language: string;
  shortcuts: ShortcutMap;

  // 模型提供商（支持多个）
  providers: ModelProvider[];
  activeProviderId: string | null;

  // MCP 服务器配置
  mcpServers: McpServerUI[];
  mcpSeeded: boolean;

  // 模型配额耗尽记录（key 为 "providerId::model"，value 为错误信息）
  quotaExhaustedModels: Record<string, string>;

  // 状态
  ready: boolean;
  persistErrorCount: number;
  sidebarCollapsed: boolean;

  // 用户自定义 Agent
  agentConfigs: AgentConfig[];
  /** 当前对话绑定的 Agent ID（null=普通模式） */
  activeAgentId: string | null;

  // 工作空间
  workspaceId: string | null;
  workspaceName: string;
  workspacePath: string;

  // 动作 - 对话
  setActiveConversation: (id: string) => void;
  createConversation: () => string;
  addMessage: (conversationId: string, msg: Message) => void;
  replaceMessages: (conversationId: string, msgs: Message[]) => void;
  updateLastAssistantMessage: (conversationId: string, content: string) => void;
  updateLastAssistantThinking: (conversationId: string, thinking: string, stats?: { tokens: number; durationMs: number }) => void;
  updateLastAssistantToolSteps: (conversationId: string, toolSteps: Message["toolSteps"]) => void;
  updateConversationTitle: (id: string, title: string) => void;
  deleteConversation: (id: string) => void;
  togglePinConversation: (id: string) => void;

  // 动作 - 设置
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (lang: string) => void;
  setShortcut: (action: ShortcutAction, keys: string[]) => void;

  // 动作 - 模型管理
  addProvider: (name: string, apiBase: string, apiKey: string, models: string[]) => void;
  updateProvider: (id: string, updates: Partial<Omit<ModelProvider, "id">>) => void;
  removeProvider: (id: string) => void;
  setActiveProvider: (id: string) => void;
  addModel: (providerId: string, model: string) => void;
  removeModel: (providerId: string, model: string) => void;
  setActiveModel: (providerId: string, model: string) => void;

  // 模型配额耗尽管理
  markModelExhausted: (providerId: string, model: string, errorMsg: string) => void;
  clearModelExhausted: (providerId: string, model: string) => void;
  clearAllExhausted: () => void;
  /** 查找下一个可用的（未耗尽的）模型，优先同 provider 的同系列模型，再跨 provider */
  findNextAvailableModel: () => { providerId: string; model: string } | null;

  // 动作 - MCP 服务器管理
  addMcpServer: (server: McpServerUI) => void;
  removeMcpServer: (name: string) => void;

  // 便捷方法：获取当前激活的 provider 和 model
  getActiveProvider: () => ModelProvider | undefined;

  // 侧边栏折叠
  toggleSidebar: () => void;

  // 动作 - Agent 管理
  addAgentConfig: (config: Omit<AgentConfig, "id" | "createdAt" | "updatedAt">) => string;
  updateAgentConfig: (id: string, updates: Partial<Omit<AgentConfig, "id" | "createdAt">>) => void;
  removeAgentConfig: (id: string) => void;
  setActiveAgent: (agentId: string | null) => void;

  // 动作 - 工作空间
  setWorkspace: (id: string, name: string, path: string) => void;
  clearWorkspace: () => void;

  // 动作 - 持久化
  loadFromStore: () => Promise<void>;
  saveToStore: () => Promise<void>;
}

// ========== 持久化引擎 ==========

let store: Store | null = null;

async function getStore(): Promise<Store> {
  if (!store) {
    store = await Store.load("store.json");
  }
  return store;
}

function applyTheme(theme: ThemeMode) {
  const root = document.documentElement;
  root.classList.remove("theme-light", "theme-dark");

  if (theme === "system") {
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    root.classList.add(prefersDark ? "theme-dark" : "theme-light");
  } else {
    root.classList.add(`theme-${theme}`);
  }
}

/** 延迟写盘（2 秒 debounce）：所有 action 共用一个 timer，避免流式对话时频繁写盘 */
let saveTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    useAppStore.getState().saveToStore();
  }, 2000);
}

// 默认 provider（首次使用）
const defaultProvider = (): ModelProvider => ({
  id: crypto.randomUUID(),
  name: "默认",
  apiBase: "https://api.openai.com/v1",
  apiKey: "",
  models: ["gpt-4o"],
  activeModel: "gpt-4o",
});

// 内置 Web 工具 MCP Server（联网搜索 + 网页爬取，零依赖、无需 API Key）
// 使用相对路径，Rust 端会在运行时解析为绝对路径
const DEFAULT_WEB_SERVER: McpServerUI = {
  name: "web",
  command: "node",
  args: ["./mcp-servers/web/index.mjs"],
};

// 内置 Tavily MCP Server（AI 搜索引擎 + 内容提取，需设置 TAVILY_API_KEY 环境变量）
// Key 获取: https://tavily.com
const DEFAULT_TAVILY_SERVER: McpServerUI = {
  name: "tavily",
  command: "node",
  args: ["./mcp-servers/tavily/index.mjs"],
};

// 内置 Windows MCP Server（Windows 原生 UI 自动化）
// 基于 sbroenne/mcp-windows，独立的 .exe 二进制（无需 Node.js / Python / .NET）
// 提供: ui_find/ui_click/ui_type/ui_read/screenshot/mouse/keyboard/window_management
// 仅在 Windows 上可用；非 Windows 平台连接会失败但不影响应用运行
const DEFAULT_WINDOWS_MCP_SERVER: McpServerUI = {
  name: "windows",
  command: "./binaries/windows-mcp/windows-mcp-server.exe",
  args: [],
};

// ========== Store 实现 ==========

export const useAppStore = create<AppState>((set, get) => ({
  conversations: [
    {
      id: "default",
      title: "新对话",
      createdAt: new Date().toISOString(),
    },
  ],
  activeConversationId: "default",
  messages: {},

  theme: "system",
  language: "zh-CN",
  shortcuts: { ...DEFAULT_SHORTCUTS },

  providers: [],
  activeProviderId: null,
  quotaExhaustedModels: {},

  mcpServers: [],

  mcpSeeded: false,

  agentConfigs: [],
  activeAgentId: null,

  ready: false,
  persistErrorCount: 0,
  sidebarCollapsed: false,
  workspaceId: null,
  workspaceName: "",
  workspacePath: "",

  // ---- 对话操作 ----

  setActiveConversation: (id) => set({ activeConversationId: id }),

  createConversation: () => {
    const state = get();
    const currentMsgs = state.messages[state.activeConversationId || ""] || [];
    if (currentMsgs.length === 0 && state.activeConversationId) {
      return state.activeConversationId;
    }

    const id = crypto.randomUUID();
    const conv: Conversation = {
      id,
      title: "新对话",
      createdAt: new Date().toISOString(),
    };
    set((s) => ({
      conversations: [conv, ...s.conversations],
      activeConversationId: id,
    }));
    scheduleSave();
    return id;
  },

  addMessage: (conversationId, msg) => {
    set((state) => ({
      messages: {
        ...state.messages,
        [conversationId]: [...(state.messages[conversationId] || []), msg],
      },
    }));
    const state = get();
    const msgs = state.messages[conversationId] || [];
    const conv = state.conversations.find((c) => c.id === conversationId);
    if (conv && conv.title === "新对话" && msgs.length === 2) {
      const firstUser = msgs.find((m) => m.role === "user");
      if (firstUser) {
        const title = firstUser.content.slice(0, 30);
        set((s) => ({
          conversations: s.conversations.map((c) =>
            c.id === conversationId ? { ...c, title } : c
          ),
        }));
      }
    }
    scheduleSave();
  },

  replaceMessages: (conversationId, msgs) => {
    set((state) => ({
      messages: {
        ...state.messages,
        [conversationId]: msgs,
      },
    }));
    scheduleSave();
  },

  updateLastAssistantMessage: (conversationId, content) => {
    set((state) => {
      const msgs = state.messages[conversationId] || [];
      if (msgs.length === 0) return state;
      const updated = [...msgs];
      const last = updated[updated.length - 1];
      if (last.role === "assistant") {
        updated[updated.length - 1] = { ...last, content };
      }
      return { messages: { ...state.messages, [conversationId]: updated } };
    });
    scheduleSave();
  },

  updateLastAssistantThinking: (conversationId, thinking, stats) => {
    set((state) => {
      const msgs = state.messages[conversationId] || [];
      if (msgs.length === 0) return state;
      const updated = [...msgs];
      const last = updated[updated.length - 1];
      if (last.role === "assistant") {
        updated[updated.length - 1] = {
          ...last,
          thinking: thinking || (last.thinking ?? "") + (thinking ?? ""),
          thinkingStats: stats ?? last.thinkingStats,
        };
      }
      return { messages: { ...state.messages, [conversationId]: updated } };
    });
    scheduleSave();
  },

  updateLastAssistantToolSteps: (conversationId, toolSteps) => {
    set((state) => {
      const msgs = state.messages[conversationId] || [];
      if (msgs.length === 0) return state;
      const updated = [...msgs];
      const last = updated[updated.length - 1];
      if (last.role === "assistant") {
        updated[updated.length - 1] = { ...last, toolSteps };
      }
      return { messages: { ...state.messages, [conversationId]: updated } };
    });
    scheduleSave();
  },

  updateConversationTitle: (id, title) =>
    set((state) => ({
      conversations: state.conversations.map((c) =>
        c.id === id ? { ...c, title } : c
      ),
    })),

  deleteConversation: (id) => {
    set((state) => {
      const conversations = state.conversations.filter((c) => c.id !== id);
      const { [id]: _, ...restMessages } = state.messages;
      const activeId =
        state.activeConversationId === id
          ? conversations[0]?.id || null
          : state.activeConversationId;
      return { conversations, messages: restMessages, activeConversationId: activeId };
    });
    scheduleSave();
  },

  togglePinConversation: (id) => {
    set((state) => ({
      conversations: state.conversations.map((c) =>
        c.id === id ? { ...c, pinned: !c.pinned } : c,
      ),
    }));
    scheduleSave();
  },

  // ---- 设置操作 ----

  setTheme: (theme) => {
    set({ theme });
    applyTheme(theme);
    scheduleSave();
  },

  setLanguage: (language) => {
    i18n.changeLanguage(language);
    set({ language });
    scheduleSave();
  },

  setShortcut: (action, keys) => {
    set((s) => ({
      shortcuts: { ...s.shortcuts, [action]: { keys } },
    }));
    scheduleSave();
  },

  // ---- 模型管理 ----

  addProvider: (name, apiBase, apiKey, models) => {
    const id = crypto.randomUUID();
    const provider: ModelProvider = {
      id,
      name,
      apiBase,
      apiKey,
      models,
      activeModel: models[0] || "",
    };
    set((s) => ({
      providers: [...s.providers, provider],
      activeProviderId: s.activeProviderId || id,
    }));
    scheduleSave();
  },

  updateProvider: (id, updates) => {
    set((s) => ({
      providers: s.providers.map((p) =>
        p.id === id ? { ...p, ...updates } : p
      ),
    }));
    scheduleSave();
  },

  removeProvider: (id) => {
    set((s) => {
      const providers = s.providers.filter((p) => p.id !== id);
      const activeProviderId = s.activeProviderId === id
        ? providers[0]?.id || null
        : s.activeProviderId;
      return { providers, activeProviderId };
    });
    scheduleSave();
  },

  setActiveProvider: (id) => set({ activeProviderId: id }),

  addModel: (providerId, model) => {
    set((s) => ({
      providers: s.providers.map((p) =>
        p.id === providerId && !p.models.includes(model)
          ? { ...p, models: [...p.models, model] }
          : p
      ),
    }));
    scheduleSave();
  },

  removeModel: (providerId, model) => {
    set((s) => ({
      providers: s.providers.map((p) =>
        p.id === providerId
          ? {
              ...p,
              models: p.models.filter((m) => m !== model),
              activeModel: p.activeModel === model ? p.models[0] : p.activeModel,
            }
          : p
      ),
    }));
    scheduleSave();
  },

  setActiveModel: (providerId, model) => {
    set((s) => ({
      providers: s.providers.map((p) =>
        p.id === providerId ? { ...p, activeModel: model } : p
      ),
    }));
    scheduleSave();
  },

  // ---- 模型配额耗尽管理 ----

  /** 标记模型为额度已耗尽 */
  markModelExhausted: (providerId, model, errorMsg) => {
    const key = `${providerId}::${model}`;
    set((s) => ({
      quotaExhaustedModels: { ...s.quotaExhaustedModels, [key]: errorMsg },
    }));
    scheduleSave();
  },

  /** 清除单个模型的耗尽标记（例如用户手动恢复） */
  clearModelExhausted: (providerId, model) => {
    const key = `${providerId}::${model}`;
    set((s) => {
      const { [key]: _, ...rest } = s.quotaExhaustedModels;
      return { quotaExhaustedModels: rest };
    });
    scheduleSave();
  },

  /** 清除所有模型的耗尽标记 */
  clearAllExhausted: () => {
    set({ quotaExhaustedModels: {} });
    scheduleSave();
  },

  /** 查找下一个可用的模型（按优先级）：
   *  1. 同 provider 的下一个未耗尽的模型
   *  2. 其他 provider 的第一个未耗尽的模型
   *  返回 null = 所有模型都已耗尽 */
  findNextAvailableModel: () => {
    const state = get();
    const activeP = state.providers.find((p) => p.id === state.activeProviderId);

    // 先在同 provider 内找下一个模型
    if (activeP) {
      const currentIdx = activeP.models.indexOf(activeP.activeModel);
      for (let i = currentIdx + 1; i < activeP.models.length; i++) {
        const m = activeP.models[i];
        if (!state.quotaExhaustedModels[`${activeP.id}::${m}`]) {
          return { providerId: activeP.id, model: m };
        }
      }
    }

    // 同 provider 没有可用模型 → 跨 provider 找
    for (const p of state.providers) {
      if (p.id === state.activeProviderId && activeP) {
        // 刚才已经搜过同 provider 了，跳过
        // 但如果没有 activeP（当前没有激活的 provider），第一次循环也要搜
        continue;
      }
      for (const m of p.models) {
        if (!state.quotaExhaustedModels[`${p.id}::${m}`]) {
          return { providerId: p.id, model: m };
        }
      }
    }

    return null;
  },

  addMcpServer: (server) => {
    set((s) => {
      const exists = s.mcpServers.some((m) => m.name === server.name);
      if (exists) {
        return {
          mcpServers: s.mcpServers.map((m) => (m.name === server.name ? server : m)),
        };
      }
      return { mcpServers: [...s.mcpServers, server] };
    });
    scheduleSave();
  },

  removeMcpServer: (name) => {
    set((s) => ({
      mcpServers: s.mcpServers.filter((m) => m.name !== name),
    }));
    scheduleSave();
  },

  getActiveProvider: () => {
    const state = get();
    return state.providers.find((p) => p.id === state.activeProviderId);
  },

  toggleSidebar: () => {
    set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed }));
    scheduleSave();
  },

  // ---- 自定义 Agent 管理 ----

  addAgentConfig: (config) => {
    const id = crypto.randomUUID();
    const now = new Date().toISOString();
    const agent: AgentConfig = {
      id,
      ...config,
      createdAt: now,
      updatedAt: now,
    };
    set((s) => ({ agentConfigs: [...s.agentConfigs, agent] }));
    scheduleSave();
    return id;
  },

  updateAgentConfig: (id, updates) => {
    set((s) => ({
      agentConfigs: s.agentConfigs.map((a) =>
        a.id === id
          ? { ...a, ...updates, updatedAt: new Date().toISOString() }
          : a,
      ),
    }));
    scheduleSave();
  },

  removeAgentConfig: (id) => {
    set((s) => ({
      agentConfigs: s.agentConfigs.filter((a) => a.id !== id),
      activeAgentId: s.activeAgentId === id ? null : s.activeAgentId,
    }));
    scheduleSave();
  },

  setActiveAgent: (agentId) => {
    set({ activeAgentId: agentId });
    scheduleSave();
  },

  // ---- 对话模式切换 ----

  // ---- 工作空间操作 ----

  setWorkspace: (id, name, path) => {
    set({ workspaceId: id, workspaceName: name, workspacePath: path });
    scheduleSave();
  },

  clearWorkspace: () => {
    set({ workspaceId: null, workspaceName: "", workspacePath: "" });
    scheduleSave();
  },

  // ---- 持久化 ----

  loadFromStore: async () => {
    try {
      const s = await getStore();
      const conversations = (await s.get<Conversation[]>("conversations")) || [];
      const messages = (await s.get<Record<string, Message[]>>("messages")) || {};
      const theme = (await s.get<ThemeMode>("theme")) || "system";
      const language = (await s.get<string>("language")) || "zh-CN";
      const shortcuts = (await s.get<ShortcutMap>("shortcuts")) || DEFAULT_SHORTCUTS;
      let providers = (await s.get<ModelProvider[]>("providers")) || [];

      // 解密 API Key（兼容旧明文数据）
      providers = await Promise.all(
        providers.map(async (p) => ({
          ...p,
          apiKey: await decryptApiKey(p.apiKey),
        })),
      );
      const activeProviderId = (await s.get<string | null>("activeProviderId")) || providers[0]?.id || null;
      const sidebarCollapsed = (await s.get<boolean>("sidebarCollapsed")) || false;
      const mcpServers = (await s.get<McpServerUI[]>("mcpServers")) || [];
      const mcpSeeded = (await s.get<boolean>("mcpSeeded")) || false;
      const quotaExhaustedModels = (await s.get<Record<string, string>>("quotaExhaustedModels")) || {};
      const workspaceId = (await s.get<string | null>("workspaceId")) || null;
      const workspaceName = (await s.get<string>("workspaceName")) || "";
      const workspacePath = (await s.get<string>("workspacePath")) || "";
      const agentConfigs = (await s.get<AgentConfig[]>("agentConfigs")) || [];
      const activeAgentIdFromStore = (await s.get<string | null>("activeAgentId")) || null;

      // 迁移旧数据：如果有 apiKey/apiBase/model 但没 providers，自动创建默认 provider
      if (providers.length === 0) {
        const oldApiKey = (await s.get<string>("apiKey")) || "";
        const oldApiBase = (await s.get<string>("apiBase")) || "";
        const oldModel = (await s.get<string>("model")) || "";
        if (oldApiKey) {
          const p = defaultProvider();
          p.apiKey = oldApiKey;
          if (oldApiBase) p.apiBase = oldApiBase;
          if (oldModel) p.models = [oldModel];
          p.activeModel = oldModel || p.models[0];
          providers.push(p);
        }
      }

      const finalConversations =
        conversations.length > 0
          ? conversations
          : [{ id: "default", title: "新对话", createdAt: new Date().toISOString() }];

      // 首次启动时种子内置 MCP 服务器（之后用户删改均持久保留）
      const seededMcp = mcpSeeded
        ? mcpServers
        : [DEFAULT_WEB_SERVER, DEFAULT_TAVILY_SERVER, DEFAULT_WINDOWS_MCP_SERVER];

      set({
        conversations: finalConversations,
        activeConversationId: finalConversations[0].id,
        messages,
        theme,
        language,
        shortcuts,
        providers,
        activeProviderId,
        quotaExhaustedModels,
        sidebarCollapsed,
        mcpServers: seededMcp,
        mcpSeeded: true,
        workspaceId,
        workspaceName,
        workspacePath,
        agentConfigs,
        activeAgentId: activeAgentIdFromStore,
        ready: true,
      });

      applyTheme(theme);
      i18n.changeLanguage(language);
    } catch (err) {
      console.error(
        "[persist] 加载配置失败:",
        err instanceof Error ? err.message : err,
      );
      set({ ready: true });
      applyTheme("system");
    }
  },

  saveToStore: async () => {
    try {
      const s = await getStore();
      const state = get();

      // 加密 API Key 后再持久化
      const safeProviders = await Promise.all(
        state.providers.map(async (p) => ({
          ...p,
          apiKey: await encryptApiKey(p.apiKey),
        })),
      );

      await s.set("conversations", state.conversations);
      await s.set("messages", state.messages);
      await s.set("theme", state.theme);
      await s.set("language", state.language);
      await s.set("shortcuts", state.shortcuts);
      await s.set("providers", safeProviders);
      await s.set("activeProviderId", state.activeProviderId);
      await s.set("sidebarCollapsed", state.sidebarCollapsed);
      await s.set("mcpServers", state.mcpServers);
      await s.set("mcpSeeded", state.mcpSeeded);
      await s.set("quotaExhaustedModels", state.quotaExhaustedModels);
      await s.set("workspaceId", state.workspaceId);
      await s.set("workspaceName", state.workspaceName);
      await s.set("workspacePath", state.workspacePath);
      await s.set("agentConfigs", state.agentConfigs);
      await s.set("activeAgentId", state.activeAgentId);
      // 清理旧字段
      await s.delete("apiKey");
      await s.delete("apiBase");
      await s.delete("model");
      await s.save();

      // 成功后清零错误计数
      if (state.persistErrorCount > 0) {
        set({ persistErrorCount: 0 });
      }
    } catch (err) {
      console.error(
        "[persist] 保存失败:",
        err instanceof Error ? err.message : err,
      );
      set({ persistErrorCount: get().persistErrorCount + 1 });
    }
  },
}));
