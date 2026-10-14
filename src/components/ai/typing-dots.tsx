/**
 * 发送中/生成中指示器（D51）：三点跳动。
 * shadcn 无 typing-dots 预设（其 Chat 示例同为三点自绘；Spinner 面向按钮场景），
 * 故用项目自有 keyframes（index.css .typing-dots）——替代此前孤零零的「…」文本。
 */
export function TypingDots() {
  return (
    <span className="typing-dots inline-flex items-center gap-1 py-1" aria-label="正在生成">
      <span />
      <span />
      <span />
    </span>
  );
}
