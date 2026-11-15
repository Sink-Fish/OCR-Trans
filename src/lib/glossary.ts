/** Split only at the first separator: translations may themselves contain equals signs or arrows. */
export function parseGlossary(text: string): { entries: [string, string][]; invalidLines: number[] } {
  const entries: [string, string][] = [];
  const invalidLines: number[] = [];
  text.split("\n").forEach((line, index) => {
    if (!line.trim()) return;
    const match = line.match(/^(.*?)\s*(?:=|→|->|\t)\s*(.*)$/);
    if (!match || !match[1].trim() || !match[2].trim()) invalidLines.push(index + 1);
    else entries.push([match[1].trim(), match[2].trim()]);
  });
  return { entries, invalidLines };
}

export function formatGlossary(entries: [string, string][]): string {
  return entries.map(([source, translation]) => `${source} = ${translation}`).join("\n");
}
