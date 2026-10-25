#!/usr/bin/env node
// Enforces the locale rules ("i18n" in AGENTS.md). en.json is the source of
// truth and zh.json must match it key for key. Other locales may lag, since
// missing keys fall back (zh-TW to zh, the rest to en), but they must not keep
// keys en.json has dropped, and every string must use en's placeholders.
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

const dir = path.join(process.cwd(), 'src', 'i18n');
const PLURAL_SUFFIX = /_(zero|one|two|few|many|other)$/;
const REQUIRED_COMPLETE = new Set(['zh']);

function flatten(obj, prefix = '', out = new Map()) {
  for (const [key, value] of Object.entries(obj)) {
    const full = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === 'object') flatten(value, full, out);
    else out.set(full, String(value));
  }
  return out;
}

const baseKey = (key) => key.replace(PLURAL_SUFFIX, '');
const placeholders = (text) =>
  new Set([...text.matchAll(/\{\{\s*([\w.]+)/g)].map((m) => m[1]));

// Plural forms differ per language (en has _one/_other, ko only _other), so
// compare by base key, against the placeholders of all of en's forms.
function byBase(entries) {
  const out = new Map();
  for (const [key, text] of entries) {
    const base = baseKey(key);
    if (!out.has(base)) out.set(base, new Set());
    for (const name of placeholders(text)) out.get(base).add(name);
  }
  return out;
}

const read = (file) => flatten(JSON.parse(fs.readFileSync(path.join(dir, file), 'utf8')));
const en = byBase(read('en.json'));
const errors = [];
const summary = [];

// Use the existing TypeScript compiler to parse calls, not regexes that also
// match comments or miss multiline calls. Literal unions cover dynamic UI keys.
const configPath = path.join(process.cwd(), 'tsconfig.app.json');
const config = ts.readConfigFile(configPath, ts.sys.readFile);
if (config.error) throw new Error(ts.flattenDiagnosticMessageText(config.error.messageText, '\n'));
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, process.cwd());
if (parsed.errors.length) throw new Error(ts.formatDiagnosticsWithColorAndContext(parsed.errors, {
  getCanonicalFileName: (file) => file,
  getCurrentDirectory: () => process.cwd(),
  getNewLine: () => '\n',
}));
const program = ts.createProgram(parsed.fileNames, parsed.options);
const checker = program.getTypeChecker();

function literalValues(type) {
  if (type.isStringLiteral()) return [type.value];
  if (type.isUnion()) {
    const values = type.types.map(literalValues);
    if (values.every(Boolean)) return values.flat();
  }
  return null;
}

function keysOf(node) {
  if (!node) return null;
  if (ts.isStringLiteralLike(node)) return [node.text];
  const inferred = literalValues(checker.getTypeAtLocation(node));
  if (inferred) return inferred;
  if (ts.isTemplateExpression(node)) {
    let keys = [node.head.text];
    for (const span of node.templateSpans) {
      const values = keysOf(span.expression);
      if (!values) return null;
      keys = keys.flatMap((key) => values.map((value) => key + value + span.literal.text));
    }
    return keys;
  }
  if (ts.isConditionalExpression(node)) {
    const yes = keysOf(node.whenTrue);
    const no = keysOf(node.whenFalse);
    return yes && no ? [...yes, ...no] : null;
  }
  if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.QuestionQuestionToken) {
    const left = keysOf(node.left);
    const right = keysOf(node.right);
    return left && right ? [...left, ...right] : null;
  }
  // Record<string, string> widens map values. Inspect its initializer so map
  // lookups (such as ShareDialog's status labels) are checked as well.
  if (ts.isElementAccessExpression(node)) {
    const symbol = checker.getSymbolAtLocation(node.expression);
    const declaration = symbol?.valueDeclaration;
    const initializer = declaration && ts.isVariableDeclaration(declaration) && declaration.initializer;
    if (initializer && ts.isObjectLiteralExpression(initializer)) {
      const values = initializer.properties.map((property) =>
        ts.isPropertyAssignment(property) ? keysOf(property.initializer) : null);
      if (values.every(Boolean)) return values.flat();
    }
  }
  return null;
}

let referenceCount = 0;
for (const source of program.getSourceFiles()) {
  const file = path.relative(process.cwd(), source.fileName);
  if (!file.startsWith(`src${path.sep}`) || source.isDeclarationFile || /\.(test|spec)\.[cm]?[jt]sx?$/.test(file)) continue;
  function visit(node) {
    if (ts.isCallExpression(node) && (
      (ts.isIdentifier(node.expression) && node.expression.text === 't') ||
      (ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 't')
    )) {
      const keys = keysOf(node.arguments[0]);
      const { line } = source.getLineAndCharacterOfPosition(node.getStart());
      if (!keys) {
        errors.push(`${file}:${line + 1}: cannot resolve translation key — use string literals or a literal union`);
      } else {
        for (const key of new Set(keys)) {
          referenceCount++;
          if (!en.has(baseKey(key))) errors.push(`${file}:${line + 1}: ${key} is not in en.json`);
        }
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}

for (const file of fs.readdirSync(dir).filter((f) => f.endsWith('.json') && f !== 'en.json').sort()) {
  const locale = file.replace(/\.json$/, '');
  const entries = read(file);
  const seen = new Set();
  for (const [key, text] of entries) {
    const base = baseKey(key);
    seen.add(base);
    const allowed = en.get(base);
    if (!allowed) {
      errors.push(`${file}: ${key} is not in en.json — delete it`);
      continue;
    }
    for (const name of placeholders(text)) {
      if (!allowed.has(name)) errors.push(`${file}: ${key} uses {{${name}}}, which en.json does not`);
    }
    if (!PLURAL_SUFFIX.test(key)) {
      for (const name of allowed) {
        if (!placeholders(text).has(name)) errors.push(`${file}: ${key} drops {{${name}}}`);
      }
    }
  }
  const missing = [...en.keys()].filter((base) => !seen.has(base));
  if (REQUIRED_COMPLETE.has(locale) && missing.length > 0) {
    errors.push(`${file}: missing ${missing.length} key(s) every UI change must add: ${missing.join(', ')}`);
  }
  summary.push(`${locale} ${missing.length} missing`);
}

if (errors.length > 0) {
  console.error(errors.join('\n'));
  console.error(`\ni18n check failed: ${errors.length} problem(s).`);
  process.exit(1);
}
console.log(`i18n check passed (${referenceCount} code references; ${summary.join(', ')}).`);
