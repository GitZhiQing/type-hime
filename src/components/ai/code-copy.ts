import { toast } from "sonner";

/**
 * 代码块复制增强（拦截 Streamdown 复制按钮的原始行为）。
 * 根因：navigator.clipboard.writeText 在文档未聚焦时抛 NotAllowedError
 * （内嵌浏览器/IDE 预览常见），且 Streamdown 对失败无任何反馈——「点了没反应」。
 * 方案：接管点击 → clipboard API 失败时回退 execCommand('copy') → 统一 toast 反馈。
 */

/** 剪贴板写入（clipboard API 失败时回退 execCommand）；返回是否成功。 */
export async function writeClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* 文档未聚焦等场景回退 execCommand */
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.top = "0";
    ta.style.left = "0";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}

let installed = false;

/** 安装一次（document 级捕获监听，拦截复制按钮点击并接管反馈）。 */
export function installCodeCopyEnhancer() {
  if (installed || typeof document === "undefined") return;
  installed = true;
  document.addEventListener(
    "click",
    (e) => {
      const target = e.target as HTMLElement | null;
      const btn = target?.closest?.('[data-streamdown="code-block-copy-button"]');
      if (!btn) return;
      // 阻断 Streamdown 自身的处理（避免其内部 check 态与静默失败路径）
      e.preventDefault();
      e.stopPropagation();
      const block = btn.closest('[data-streamdown="code-block"]');
      const body = block?.querySelector('[data-streamdown="code-block-body"]');
      const text = body?.textContent ?? "";
      if (!text) return;
      void writeClipboard(text).then((ok) => {
        if (ok) toast.success("已复制到剪贴板");
        else toast.error("复制失败：剪贴板不可用");
      });
    },
    true,
  );
}
