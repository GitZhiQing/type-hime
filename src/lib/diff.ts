/** 连续 3 行以上的上下文行折叠为省略号（默认只展示变更±3 行的语义不变，视觉更紧凑） */
export function collapseContext(diff: string): string[] {
  const lines = diff.split("\n");
  const out: string[] = [];
  let run: string[] = [];
  const flush = () => {
    if (run.length > 4) {
      out.push(run[0], "⋯", run[run.length - 1]);
    } else {
      out.push(...run);
    }
    run = [];
  };
  for (const line of lines) {
    const isContext =
      !line.startsWith("+") && !line.startsWith("-") && !line.startsWith("@@");
    if (isContext) {
      run.push(line);
    } else {
      flush();
      out.push(line);
    }
  }
  flush();
  return out;
}
