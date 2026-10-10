// 全局 Toast。在任何地方 import { toast } 调用即可，无需注入。
// 渲染由 <ToastHost /> 负责（已挂在 App.vue 外壳里）。
import { ref } from "vue";

export type ToastKind = "info" | "success" | "error";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  text: string;
}

const items = ref<ToastItem[]>([]);
let seq = 0;

/** 当前待显示的 Toast 列表（只读，供 <ToastHost /> 消费）。 */
export function useToasts() {
  return items;
}

/** 弹出一条 Toast，返回其 id（可用 dismissToast 提前关闭）。 */
export function toast(text: string, kind: ToastKind = "info", durationMs = 3500): number {
  const id = ++seq;
  items.value = [...items.value, { id, kind, text }];
  if (durationMs > 0) {
    window.setTimeout(() => dismissToast(id), durationMs);
  }
  return id;
}

export function toastSuccess(text: string, durationMs = 3500): number {
  return toast(text, "success", durationMs);
}

export function toastError(text: string, durationMs = 8000): number {
  return toast(text, "error", durationMs);
}

export function dismissToast(id: number) {
  items.value = items.value.filter((item) => item.id !== id);
}
