/**
 * API Key 本地加密层 (Layer 1)
 *
 * 原理：
 *   - 从应用密钥 + PBKDF2 派生 AES-GCM 密钥
 *   - 每次加密使用随机 salt + 随机 IV
 *   - 密文前 16 字节 = salt，中间 12 字节 = IV，剩余 = ciphertext
 *   - 统一前缀 "ENC:" 区分明文/密文（兼容旧数据迁移）
 *
 * 安全等级：
 *   Layer 1（当前）：防止直接打开 store.json 看到明文。
 *   Layer 2（V0.2）：将加密逻辑移到 Rust 端。
 *   Layer 3（V1.0）：使用操作系统凭据管理器。
 */

const APP_SECRET = "agent-desktop-aes-v1";
const ALGORITHM = "AES-GCM";
const KEY_LENGTH = 256;
const ENC_PREFIX = "ENC:";
const PBKDF2_ITERATIONS = 100_000;

async function deriveKey(salt: Uint8Array): Promise<CryptoKey> {
  const encoder = new TextEncoder();
  const keyMaterial = await crypto.subtle.importKey(
    "raw",
    encoder.encode(APP_SECRET),
    "PBKDF2",
    false,
    ["deriveKey"],
  );

  return crypto.subtle.deriveKey(
    {
      name: "PBKDF2",
      salt,
      iterations: PBKDF2_ITERATIONS,
      hash: "SHA-256",
    },
    keyMaterial,
    { name: ALGORITHM, length: KEY_LENGTH },
    false,
    ["encrypt", "decrypt"],
  );
}

/**
 * 加密 API Key
 * @returns "ENC:<base64>" 格式的密文
 */
export async function encryptApiKey(plaintext: string): Promise<string> {
  // 空值不加密
  if (!plaintext) return plaintext;

  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveKey(salt);

  const encrypted = await crypto.subtle.encrypt(
    { name: ALGORITHM, iv },
    key,
    new TextEncoder().encode(plaintext),
  );

  // 拼接: salt(16) + iv(12) + ciphertext
  const combined = new Uint8Array(
    salt.length + iv.length + encrypted.byteLength,
  );
  combined.set(salt, 0);
  combined.set(iv, salt.length);
  combined.set(new Uint8Array(encrypted), salt.length + iv.length);

  return (
    ENC_PREFIX +
    btoa(String.fromCharCode(...combined))
  );
}

/**
 * 解密 API Key
 * @param stored 存储的值（可能是 "ENC:..." 密文或原始明文）
 * @returns 解密后的原文
 */
export async function decryptApiKey(stored: string): Promise<string> {
  // 空值或非加密数据：直接返回（兼容旧数据）
  if (!stored || !stored.startsWith(ENC_PREFIX)) {
    return stored;
  }

  try {
    const raw = stored.slice(ENC_PREFIX.length);
    const data = Uint8Array.from(atob(raw), (c) => c.charCodeAt(0));

    const salt = data.slice(0, 16);
    const iv = data.slice(16, 28);
    const ciphertext = data.slice(28);

    const key = await deriveKey(salt);
    const decrypted = await crypto.subtle.decrypt(
      { name: ALGORITHM, iv },
      key,
      ciphertext,
    );

    return new TextDecoder().decode(decrypted);
  } catch (err) {
    console.error("[crypto] API Key 解密失败:", err);
    // 解密失败返回空字符串，不阻塞启动
    return "";
  }
}
