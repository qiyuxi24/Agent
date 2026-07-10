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
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: string;
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

  // 状态
  ready: boolean;
  persistErrorCount: number;
  sidebarCollapsed: boolean;

  // 对话模式：聊天（纯对话）/ Agent（启用 MCP 工具循环）
  chatMode: "chat" | "agent";

  // 动作 - 对话
  setActiveConversation: (id: string) => void;
  createConversation: () => string;
  addMessage: (conversationId: string, msg: Message) => void;
  updateLastAssistantMessage: (conversationId: string, content: string) => void;
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

  // 动作 - MCP 服务器管理
  addMcpServer: (server: McpServerUI) => void;
  removeMcpServer: (name: string) => void;

  // 便捷方法：获取当前激活的 provider 和 model
  getActiveProvider: () => ModelProvider | undefined;

  // 侧边栏折叠
  toggleSidebar: () => void;

  // 动作 - 对话模式切换
  setChatMode: (mode: "chat" | "agent") => void;

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

  mcpServers: [],

  mcpSeeded: false,

  ready: false,
  persistErrorCount: 0,
  sidebarCollapsed: false,
  chatMode: "agent",

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

  // ---- 对话模式切换 ----

  setChatMode: (mode) => {
    set({ chatMode: mode });
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
      const chatMode = ((await s.get<string>("chatMode")) || "agent") as "chat" | "agent";
      const mcpServers = (await s.get<McpServerUI[]>("mcpServers")) || [];
      const mcpSeeded = (await s.get<boolean>("mcpSeeded")) || false;

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

      // 首次启动时种子内置 Web + Tavily 工具服务器（之后用户删改均持久保留）
      const seededMcp = mcpSeeded ? mcpServers : [DEFAULT_WEB_SERVER, DEFAULT_TAVILY_SERVER];

      set({
        conversations: finalConversations,
        activeConversationId: finalConversations[0].id,
        messages,
        theme,
        language,
        shortcuts,
        providers,
        activeProviderId,
        sidebarCollapsed,
        chatMode,
        mcpServers: seededMcp,
        mcpSeeded: true,
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
      await s.set("chatMode", state.chatMode);
      await s.set("mcpServers", state.mcpServers);
      await s.set("mcpSeeded", state.mcpSeeded);
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
