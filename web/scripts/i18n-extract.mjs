// Extracts user-visible English strings from the web sources (JSX text, UI attributes,
// messages passed to setters/errors/toasts, and label-like object fields).
// Usage: node scripts/i18n-extract.mjs (prints JSON); imported by src/i18n/coverage.test.ts.
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../src");
const LABEL_MAP = /LABEL|TEXT|MESSAGE|TITLE/i;
const ATTRS = new Set(["title", "placeholder", "aria-label", "label", "alt", "description", "hint", "emptyText", "message", "fallbackText", "confirmLabel", "cancelLabel"]);
const PROPS = new Set(["label", "title", "description", "message", "hint", "detail", "placeholder", "text", "name"]);
const CALLS = /^(set[A-Z]\w*|toast|showToast|addToast|confirm|alert|notify|push\w*|error|success|info|warning)$/;
let out = new Set();
let templates = new Set();

function looksLikeText(s) {
  const t = s.trim();
  if (t.length < 2) return false;
  if (!/[A-Za-z]{2}/.test(t)) return false;
  if (/^[a-z][a-zA-Z0-9]*$/.test(t)) return false; // identifiers, keys
  if (/^[a-z0-9_.-]+$/.test(t)) return false; // ids, event names
  if (/^(https?|mqtt|wss?):\/\//.test(t) || t.startsWith("/")) return false;
  if (/^[\w-]+\/[\w.+-]+$/.test(t)) return false; // mime
  if (/^var\(|^#[0-9a-f]{3,8}$|px\b|^\d/.test(t) && !/\s[a-z]{3,}\s/.test(t)) return false;
  if (/^[A-Z0-9_]+$/.test(t)) return false; // constants
  if (/[{};]|=>/.test(t)) return false;
  return true;
}

function walk(file) {
  const src = fs.readFileSync(file, "utf8");
  const sf = ts.createSourceFile(file, src, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const add = (s) => {
    const t = s.replace(/\s+/g, " ").trim();
    if (looksLikeText(t)) out.add(t);
  };
  const addTemplate = (node) => {
    let s = node.head.text;
    node.templateSpans.forEach((span, i) => {
      s += `{${i}}` + span.literal.text;
    });
    const t = s.replace(/\s+/g, " ").trim();
    if (looksLikeText(t.replace(/\{\d+\}/g, ""))) templates.add(t);
  };
  const visitValue = (n) => {
    if (!n) return;
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) add(n.text);
    else if (ts.isTemplateExpression(n)) addTemplate(n);
    else if (ts.isConditionalExpression(n)) {
      visitValue(n.whenTrue);
      visitValue(n.whenFalse);
    } else if (ts.isBinaryExpression(n) && (n.operatorToken.kind === ts.SyntaxKind.BarBarToken || n.operatorToken.kind === ts.SyntaxKind.QuestionQuestionToken)) {
      visitValue(n.left);
      visitValue(n.right);
    } else if (ts.isParenthesizedExpression(n)) visitValue(n.expression);
  };
  const visit = (node) => {
    if (ts.isJsxText(node)) add(node.text);
    else if (ts.isJsxExpression(node) && node.parent && (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent))) visitValue(node.expression);
    else if (ts.isJsxAttribute(node)) {
      const name = node.name.getText(sf);
      if (ATTRS.has(name) && node.initializer) {
        if (ts.isStringLiteral(node.initializer)) add(node.initializer.text);
        else if (ts.isJsxExpression(node.initializer)) visitValue(node.initializer.expression);
      }
    } else if (
      ts.isVariableDeclaration(node) &&
      LABEL_MAP.test(node.name.getText(sf)) &&
      node.initializer &&
      ts.isObjectLiteralExpression(node.initializer)
    ) {
      for (const prop of node.initializer.properties)
        if (ts.isPropertyAssignment(prop)) visitValue(prop.initializer);
    } else if (ts.isPropertyAssignment(node)) {
      const name = node.name.getText(sf).replace(/["']/g, "");
      if (PROPS.has(name)) visitValue(node.initializer);
    } else if (ts.isNewExpression(node) && node.expression.getText(sf) === "Error") {
      node.arguments?.forEach(visitValue);
    } else if (ts.isCallExpression(node)) {
      const callee = node.expression.getText(sf).split(".").pop();
      if (CALLS.test(callee) || callee === "t" || callee === "serverMessage" || callee === "describe" && false) node.arguments.forEach(visitValue);
    } else if (ts.isReturnStatement(node) && node.expression && ts.isStringLiteral(node.expression)) {
      add(node.expression.text);
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
}

function files(dir) {
  // i18n holds the dictionaries; api is generated.
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name === "i18n" || e.name === "api") continue;
      files(p);
    } else if (/\.tsx?$/.test(e.name) && !/\.(test|d)\.tsx?$/.test(e.name)) walk(p);
  }
}
/** Visible English strings and `{0}`-style templates in the web sources. */
export function extractUiStrings(root = ROOT) {
  out = new Set();
  templates = new Set();
  files(root);
  return { strings: [...out].sort(), templates: [...templates].sort() };
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(new URL(import.meta.url).pathname)) {
  process.stdout.write(`${JSON.stringify(extractUiStrings(), null, 1)}\n`);
}
