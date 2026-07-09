/**
 * 错误处理工具 — 类型安全的异常判断与消息提取
 *
 * 为什么不用 `catch (err: any)`：
 *   JS 允许 throw 任意值（字符串、数字、null、对象）。
 *   `any` 关闭了 TypeScript 的类型检查，err.name 可能是 undefined，
 *   导致运行时条件判断失效。
 *
 * 工业做法：用 `unknown` + 类型守卫函数，显式缩小类型范围。
 */

/** 判断一个值是否具有 Error 的形状（有 message 属性） */
export function isErrorLike(
  value: unknown,
): value is { message: string; name?: string } {
  return typeof value === "object" && value !== null && "message" in value;
}

/** 判断是否为 fetch AbortError（浏览器） */
export function isAbortError(err: unknown): boolean {
  return (
    (err instanceof DOMException && err.name === "AbortError") ||
    (isErrorLike(err) && err.name === "AbortError")
  );
}

/** 从任意类型的异常中安全提取错误消息 */
export function getErrorMessage(err: unknown): string {
  if (isErrorLike(err)) return err.message;
  if (typeof err === "string") return err;
  try {
    return JSON.stringify(err);
  } catch {
    return "未知错误";
  }
}
