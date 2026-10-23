/**
 * In-memory saved filters for the dev mock backend. Same rules as
 * `filter_preset.rs`: names trimmed, non-empty and unique case-insensitively,
 * positions are list indexes, a copy is "<name> copy" (then "copy 2", …)
 * right after the original, unknown ids are rejected.
 */
import type { FilterPreset, FilterPresetOp, FilterPresetOutcome, RuleNode } from "../lib/ipc";

export function createMockPresets() {
  let nextId = 1;
  const now = () => new Date().toISOString();
  const list: FilterPreset[] = [];

  const find = (id: number) => {
    const i = list.findIndex((p) => p.id === id);
    if (i < 0) throw new Error(`saved filter ${id} not found`);
    return i;
  };
  function cleanName(name: string, self?: number) {
    const n = name.trim();
    if (n === "") throw new Error("invalid name: must not be empty");
    if (list.some((p) => p.id !== self && p.name.toLowerCase() === n.toLowerCase()))
      throw new Error(`a saved filter named "${n}" already exists`);
    return n;
  }
  function insert(name: string, rule: RuleNode | null, at: number) {
    const t = now();
    const p: FilterPreset = { id: nextId++, name, rule, createdAt: t, updatedAt: t };
    list.splice(Math.min(Math.max(at, 0), list.length), 0, p);
    return p.id;
  }

  function mutate(op: FilterPresetOp): FilterPresetOutcome {
    switch (op.type) {
      case "create":
        return {
          createdId: insert(cleanName(op.name), op.rule, op.position ?? list.length),
        };
      case "rename": {
        const i = find(op.id);
        list[i] = { ...list[i], name: cleanName(op.name, op.id), updatedAt: now() } as FilterPreset;
        return { createdId: null };
      }
      case "setRule": {
        const i = find(op.id);
        list[i] = { ...list[i], rule: op.rule, updatedAt: now() } as FilterPreset;
        return { createdId: null };
      }
      case "duplicate": {
        const i = find(op.id);
        const src = list[i] as FilterPreset;
        let n = 1;
        const copy = () => (n === 1 ? `${src.name} copy` : `${src.name} copy ${n}`);
        while (list.some((p) => p.name.toLowerCase() === copy().toLowerCase())) n += 1;
        return { createdId: insert(copy(), src.rule, i + 1) };
      }
      case "delete":
        list.splice(find(op.id), 1);
        return { createdId: null };
      case "move": {
        const i = find(op.id);
        if (op.position > list.length) throw new Error("position out of range");
        const [p] = list.splice(i, 1);
        list.splice(i < op.position ? op.position - 1 : op.position, 0, p as FilterPreset);
        return { createdId: null };
      }
    }
  }

  return { list: () => list.map((p) => ({ ...p })), mutate };
}
