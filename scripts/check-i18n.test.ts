import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";

const script = path.resolve("scripts/check-i18n.mjs");

function check(source: string, translations: object) {
  const cwd = mkdtempSync(path.join(tmpdir(), "skills-i18n-"));
  try {
    mkdirSync(path.join(cwd, "src/i18n"), { recursive: true });
    writeFileSync(path.join(cwd, "tsconfig.app.json"), JSON.stringify({
      compilerOptions: { strict: true, jsx: "react-jsx", skipLibCheck: true },
      include: ["src"],
    }));
    for (const locale of ["en", "zh"]) {
      writeFileSync(path.join(cwd, `src/i18n/${locale}.json`), JSON.stringify(translations));
    }
    writeFileSync(path.join(cwd, "src/example.tsx"), source);
    const result = spawnSync(process.execPath, [script], { cwd, encoding: "utf8" });
    return { status: result.status, output: result.stdout + result.stderr };
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
}

describe("i18n code references", () => {
  it("fails even when all locales omit the same referenced keys, then passes when added", () => {
    const keys = ["title", "message", "remember", "close", "hide"];
    const source = keys.map((key) => `t("closeAction.${key}");`).join("\n");
    const missing = check(source, {});
    expect(missing.status).toBe(1);
    for (const key of keys) expect(missing.output).toContain(`closeAction.${key} is not in en.json`);
    const complete = check(source, { closeAction: Object.fromEntries(keys.map((key) => [key, key])) });
    expect(complete.status).toBe(0);
    expect(complete.output).toContain("5 code references");
  });

  it("parses multiline calls and ignores comments and unrelated string literals", () => {
    const source = '// t("missing.comment");\nconst example = \'t("missing.text")\';\ni18n.t(\n `common.close`\n);';
    const result = check(source, { common: { close: "Close" } });
    expect(result.status).toBe(0);
    expect(result.output).toContain("1 code references");
    const missing = check(source, {});
    expect(missing.status).toBe(1);
    expect(missing.output).toContain("src/example.tsx:3: common.close");
  });

  it("checks each dynamic template variant and widened lookup-map value", () => {
    const source = [
      'const mode = null as unknown as "light" | "dark";',
      't(`theme.${mode}`);',
      'const labels: Record<string, string> = { done: "status.done", failed: "status.failed" };',
      'declare const status: string;',
      't(labels[status] ?? "status.failed");',
    ].join("\n");
    const translations = { theme: { light: "Light", dark: "Dark" }, status: { done: "Done", failed: "Failed" } };
    expect(check(source, translations).status).toBe(0);
    const missing = check(source, { theme: { light: "Light" }, status: { done: "Done" } });
    expect(missing.status).toBe(1);
    expect(missing.output).toContain("theme.dark is not in en.json");
    expect(missing.output).toContain("status.failed is not in en.json");
  });

  it("accepts plural keys through their base key", () => {
    expect(check('t("items", { count: 2 });', { items_one: "One", items_other: "Many" }).status).toBe(0);
  });

  it("fails explicitly instead of silently skipping unresolvable dynamic keys", () => {
    const result = check('declare const key: string; t(key);', {});
    expect(result.status).toBe(1);
    expect(result.output).toContain("cannot resolve translation key");
  });
});
