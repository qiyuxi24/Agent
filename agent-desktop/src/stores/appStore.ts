import { create } from "zustand";
import { Store } from "@tauri-apps/plugin-store";

// ========== 类型定义 ==========

export interface Conversation {
  id: string;
  title: string;
  createdAt: string;
}

export interface Message {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: string;
}

export type ThemeMode = "system" | "light" | "dark";

/** 一个模型提供商（如 OpenAI、DeepSeek、通义千问） */
export interface ModelProvider {
  id: string;          // 唯一 ID
  name: string;        // 显示名称，如 "OpenAI"
  apiBase: string;     // API 地址
  apiKey: string;      // API Key
  models: string[];    // 模型列表，如 ["gpt-4o", "gpt-4o-mini"]
  activeModel: string; // 当前选中的模型
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

  // 模型提供商（支持多个）
  providers: ModelProvider[];
  activeProviderId: string | null;

  // 状态
  ready: boolean;

  // 动作 - 对话
  setActiveConversation: (id: string) => void;
  createConversation: () => string;
  addMessage: (conversationId: string, msg: Message) => void;
  updateLastAssistantMessage: (conversationId: string, content: string) => void;
  updateConversationTitle: (id: string, title: string) => void;
  deleteConversation: (id: string) => void;

  // 动作 - 设置
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (lang: string) => void;

  // 动作 - 模型管理
  addProvider: (name: string, apiBase: string, apiKey: string, models: string[]) => void;
  updateProvider: (id: string, updates: Partial<Omit<ModelProvider, "id">>) => void;
  removeProvider: (id: string) => void;
  setActiveProvider: (id: string) => void;
  addModel: (providerId: string, model: string) => void;
  removeModel: (providerId: string, model: string) => void;
  setActiveModel: (providerId: string, model: string) => void;

  // 便捷方法：获取当前激活的 provider 和 model
  getActiveProvider: () => ModelProvider | undefined;

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

// 默认 provider（首次使用）
const defaultProvider = (): ModelProvider => ({
  id: crypto.randomUUID(),
  name: "默认",
  apiBase: "https://api.openai.com/v1",
  apiKey: "",
  models: ["gpt-4o"],
  activeModel: "gpt-4o",
});

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

  providers: [],
  activeProviderId: null,

  ready: false,

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
    setTimeout(() => get().saveToStore(), 100);
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
    setTimeout(() => get().saveToStore(), 100);
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
    if (content.endsWith("\n") || content.length > 200) {
      setTimeout(() => get().saveToStore(), 200);
    }
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
    setTimeout(() => get().saveToStore(), 100);
  },

  // ---- 设置操作 ----

  setTheme: (theme) => {
    set({ theme });
    applyTheme(theme);
    setTimeout(() => get().saveToStore(), 100);
  },

  setLanguage: (language) => {
    set({ language });
    setTimeout(() => get().saveToStore(), 100);
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
    setTimeout(() => get().saveToStore(), 100);
  },

  updateProvider: (id, updates) => {
    set((s) => ({
      providers: s.providers.map((p) =>
        p.id === id ? { ...p, ...updates } : p
      ),
    }));
    setTimeout(() => get().saveToStore(), 100);
  },

  removeProvider: (id) => {
    set((s) => {
      const providers = s.providers.filter((p) => p.id !== id);
      const activeProviderId = s.activeProviderId === id
        ? providers[0]?.id || null
        : s.activeProviderId;
      return { providers, activeProviderId };
    });
    setTimeout(() => get().saveToStore(), 100);
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
    setTimeout(() => get().saveToStore(), 100);
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
    setTimeout(() => get().saveToStore(), 100);
  },

  setActiveModel: (providerId, model) => {
    set((s) => ({
      providers: s.providers.map((p) =>
        p.id === providerId ? { ...p, activeModel: model } : p
      ),
    }));
    setTimeout(() => get().saveToStore(), 100);
  },

  getActiveProvider: () => {
    const state = get();
    return state.providers.find((p) => p.id === state.activeProviderId);
  },

  // ---- 持久化 ----

  loadFromStore: async () => {
    try {
      const s = await getStore();
      const conversations = (await s.get<Conversation[]>("conversations")) || [];
      const messages = (await s.get<Record<string, Message[]>>("messages")) || {};
      const theme = (await s.get<ThemeMode>("theme")) || "system";
      const language = (await s.get<string>("language")) || "zh-CN";
      const providers = (await s.get<ModelProvider[]>("providers")) || [];
      const activeProviderId = (await s.get<string | null>("activeProviderId")) || providers[0]?.id || null;

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

      set({
        conversations: finalConversations,
        activeConversationId: finalConversations[0].id,
        messages,
        theme,
        language,
        providers,
        activeProviderId,
        ready: true,
      });

      applyTheme(theme);
    } catch {
      set({ ready: true });
      applyTheme("system");
    }
  },

  saveToStore: async () => {
    try {
      const s = await getStore();
      const state = get();
      await s.set("conversations", state.conversations);
      await s.set("messages", state.messages);
      await s.set("theme", state.theme);
      await s.set("language", state.language);
      await s.set("providers", state.providers);
      await s.set("activeProviderId", state.activeProviderId);
      // 清理旧字段
      await s.delete("apiKey");
      await s.delete("apiBase");
      await s.delete("model");
      await s.save();
    } catch {
      // 忽略保存错误
    }
  },
}));
