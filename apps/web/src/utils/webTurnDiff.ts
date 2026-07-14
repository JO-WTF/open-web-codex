export type WebDiffLine = {
  type: "add" | "del" | "ctx";
  text: string;
};

export type WebTurnDiff = {
  title: string;
  fileCount: number;
  additions: number;
  deletions: number;
  lines: WebDiffLine[];
};

export function parseWebTurnDiff(diff: string): WebTurnDiff {
  const files = new Set<string>();
  let additions = 0;
  let deletions = 0;
  const lines: WebDiffLine[] = [];

  for (const line of diff.split("\n")) {
    if (!line) continue;
    if (line.startsWith("diff --git ")) {
      const match = line.match(/^diff --git a\/(.+?) b\/(.+)$/);
      files.add(match?.[2] ?? line.slice("diff --git ".length));
    } else if (line.startsWith("+++ b/") && files.size === 0) {
      files.add(line.slice(6));
    }

    if (line.startsWith("+") && !line.startsWith("+++")) additions += 1;
    if (line.startsWith("-") && !line.startsWith("---")) deletions += 1;

    lines.push({
      type: line.startsWith("+") && !line.startsWith("+++")
        ? "add"
        : line.startsWith("-") && !line.startsWith("---")
          ? "del"
          : "ctx",
      text: line.startsWith("+") || line.startsWith("-") ? line.slice(1) : line,
    });
  }

  const fileCount = files.size;
  const filesLabel = `${fileCount} ${fileCount === 1 ? "file" : "files"} changed`;
  return {
    title: `${filesLabel} · +${additions} −${deletions}`,
    fileCount,
    additions,
    deletions,
    lines,
  };
}
