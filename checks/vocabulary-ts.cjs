// 2026-09-06: regexp czytał kod między tagami i identyfikatory wewnątrz ${...} jako
// tekst ekranu. Parser już zainstalowanego TypeScriptu rozdziela te granice; polityka
// słownictwa nadal mieszka wyłącznie w vocabulary.sh.
const fs = require('node:fs');
const ts = require('typescript');

function prose(text) {
  return /\s/.test(text) && !/^[./#@]/.test(text) && /[a-z]{3}/i.test(text);
}

function visible(file) {
  const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  if (tree.parseDiagnostics.length !== 0) throw new Error(`Cannot parse ${file} for visible wording`);
  const result = [];
  function visit(node) {
    if (ts.isJsxText(node)) {
      if (node.text.trim()) result.push(node.text.trim());
    } else if (ts.isTemplateExpression(node)) {
      const text = [node.head.text, ...node.templateSpans.map((span) => span.literal.text)].join(' ');
      if (prose(text)) result.push(text);
      // Zagnieżdżony literał nadal jest tekstem; nazwa zmiennej nie jest nim nigdy.
      for (const span of node.templateSpans) visit(span.expression);
      return;
    } else if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      const parent = ts.isJsxExpression(node.parent) ? node.parent.parent : node.parent;
      const attribute = ts.isJsxAttribute(parent) ? parent.name.getText(tree) : null;
      if (attribute !== null && /^(aria-[a-z]+|title|placeholder|alt|label)$/i.test(attribute)) {
        result.push(node.text);
      } else if (prose(node.text)) {
        result.push(node.text);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(tree);
  return result;
}

const files = JSON.parse(fs.readFileSync(0, 'utf8'));
process.stdout.write(JSON.stringify(Object.fromEntries(files.map((file) => [file, visible(file)]))));
