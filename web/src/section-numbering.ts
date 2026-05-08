import type { SectionRecord } from "./api/types";

export function buildSectionNumbers(sections: SectionRecord[]) {
  const byParent = new Map<string | null, SectionRecord[]>();

  for (const section of sections) {
    const group = byParent.get(section.parent_id) ?? [];
    group.push(section);
    byParent.set(section.parent_id, group);
  }

  for (const group of byParent.values()) {
    group.sort((a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at));
  }

  const result = new Map<string, { short: string; full: string }>();

  function visit(parentId: string | null, prefix: number[]) {
    const children = byParent.get(parentId) ?? [];
    children.forEach((child, index) => {
      const nextPrefix = [...prefix, index + 1];
      result.set(child.id, {
        short: String(index + 1),
        full: nextPrefix.join("."),
      });
      visit(child.id, nextPrefix);
    });
  }

  visit(null, []);
  return result;
}
