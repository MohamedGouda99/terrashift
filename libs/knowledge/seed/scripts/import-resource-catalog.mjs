// Bulk-convert an external TypeScript resource catalog into Terrashift's
// seed bundle layout (per-resource JSON files under
// <dst>/<provider>/<category>/<terraform_resource>.json).
//
// Expected source shape: each .ts file exports an object with these
// fields (anything else is ignored):
//
//   {
//     terraform_resource: 'aws_vpc',
//     description: '...',
//     inputs: {
//       required: [{ name, type, description }, ...],
//       optional: [{ name, type, description }, ...],
//     },
//   }
//
// Usage:
//   node import-resource-catalog.mjs --src <ts-catalog-root> --dst <seed-dir>
//
// Pure regex-based parser — no TypeScript runtime needed.

import { readFileSync, readdirSync, mkdirSync, writeFileSync, statSync } from 'node:fs';
import { join, basename, relative, extname } from 'node:path';
import { argv } from 'node:process';

const args = parseArgs(argv.slice(2));
if (!args.src || !args.dst) {
  console.error('Usage: import-resource-catalog.mjs --src <ts-catalog-root> --dst <seed-dir>');
  process.exit(2);
}

console.log(`Source: ${args.src}`);
console.log(`Dest  : ${args.dst}`);

const skipFiles = new Set(['index.ts', 'types.ts', 'validation.ts', 'icons.ts']);
const supportedProviders = ['aws', 'azure', 'azurerm', 'gcp', 'google'];

let converted = 0;
let skipped = 0;
const errors = [];

walk(args.src, (filePath) => {
  if (extname(filePath) !== '.ts') return;
  if (skipFiles.has(basename(filePath))) return;

  // Infer provider from path: <src>/<provider>/<category>/<resource>.ts
  const rel = relative(args.src, filePath);
  const parts = rel.split(/[\\/]/);
  if (parts.length < 2) return;

  const sourceProvider = parts[0];
  // Map source provider key to Terrashift provider key.
  // External catalogs sometimes use 'azure' / 'gcp'; Terraform's
  // canonical keys are 'azurerm' / 'google'.
  const targetProvider =
    ({ azure: 'azurerm', gcp: 'google' })[sourceProvider] || sourceProvider;
  if (!supportedProviders.includes(targetProvider)) {
    skipped++;
    return;
  }

  const category = parts.length >= 3 ? parts[1] : 'misc';

  let raw;
  try {
    raw = readFileSync(filePath, 'utf8');
  } catch (e) {
    errors.push({ file: filePath, err: `read failed: ${e.message}` });
    return;
  }

  const def = extractServiceDefinition(raw);
  if (!def || !def.terraform_resource) {
    skipped++;
    return;
  }

  const schema = {
    name: def.terraform_resource,
    description: def.description || null,
    attributes: {},
  };

  for (const input of def.required || []) {
    schema.attributes[input.name] = attrSchema(input, true);
  }
  for (const input of def.optional || []) {
    if (!schema.attributes[input.name]) {
      schema.attributes[input.name] = attrSchema(input, false);
    }
  }

  const outDir = join(args.dst, targetProvider, category);
  mkdirSync(outDir, { recursive: true });
  const outPath = join(outDir, `${def.terraform_resource}.json`);
  writeFileSync(outPath, JSON.stringify(schema, null, 2) + '\n', 'utf8');
  converted++;
});

console.log(`\nConverted: ${converted}`);
console.log(`Skipped  : ${skipped}`);
if (errors.length > 0) {
  console.log(`Errors   : ${errors.length}`);
  for (const { file, err } of errors.slice(0, 10)) {
    console.log(`  - ${file}: ${err}`);
  }
}

// helpers

function parseArgs(items) {
  const out = {};
  for (let i = 0; i < items.length; i++) {
    if (items[i] === '--src') out.src = items[++i];
    else if (items[i] === '--dst') out.dst = items[++i];
  }
  return out;
}

function walk(dir, fn) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) walk(path, fn);
    else fn(path);
  }
}

// Pull terraform_resource, description, inputs.required, inputs.optional
// out of a ServiceDefinition .ts file via regex. Skips fields that are
// not relevant to Terrashift's ResourceSchema.
function extractServiceDefinition(src) {
  const tfMatch = src.match(/terraform_resource\s*:\s*['"]([\w]+)['"]/);
  if (!tfMatch) return null;
  const terraform_resource = tfMatch[1];

  const descMatch = src.match(/description\s*:\s*['"`]([^'"`]+)['"`]/);
  const description = descMatch ? descMatch[1] : null;

  const required = extractInputArray(src, 'required');
  const optional = extractInputArray(src, 'optional');

  return { terraform_resource, description, required, optional };
}

function extractInputArray(src, key) {
  const re = new RegExp(`${key}\\s*:\\s*\\[`, 'g');
  const m = re.exec(src);
  if (!m) return [];
  const start = m.index + m[0].length;
  const end = findMatchingBracket(src, start - 1);
  if (end < 0) return [];
  return parseInputObjects(src.slice(start, end));
}

function findMatchingBracket(src, openIdx) {
  let depth = 0;
  for (let i = openIdx; i < src.length; i++) {
    if (src[i] === '[') depth++;
    else if (src[i] === ']') {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

function parseInputObjects(body) {
  const objects = [];
  let depth = 0;
  let start = -1;
  for (let i = 0; i < body.length; i++) {
    if (body[i] === '{') {
      if (depth === 0) start = i;
      depth++;
    } else if (body[i] === '}') {
      depth--;
      if (depth === 0 && start >= 0) {
        const obj = parseSingleInput(body.slice(start, i + 1));
        if (obj) objects.push(obj);
        start = -1;
      }
    }
  }
  return objects;
}

function parseSingleInput(text) {
  const nameMatch = text.match(/name\s*:\s*['"`]([\w]+)['"`]/);
  if (!nameMatch) return null;
  const typeMatch = text.match(/type\s*:\s*['"`]([^'"`]+)['"`]/);
  const descMatch = text.match(/description\s*:\s*['"`]([^'"`]+)['"`]/);
  return {
    name: nameMatch[1],
    type: typeMatch ? typeMatch[1] : 'string',
    description: descMatch ? descMatch[1] : null,
  };
}

function attrSchema(input, required) {
  return {
    name: input.name,
    attribute_type: input.type || 'string',
    required: !!required,
    optional: !required,
    computed: false,
    sensitive: false,
    deprecated: null,
    description: input.description || null,
  };
}
