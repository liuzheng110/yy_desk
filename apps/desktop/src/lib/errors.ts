// 把任意抛出物规整成可展示的文案（后端 Result<_, String> 报错多为字符串）。
export function toErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error == null) return "未知错误";
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}
