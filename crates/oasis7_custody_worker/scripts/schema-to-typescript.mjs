// Deliberately bounded converter. Runtime constraints stay in the exported schema.
const annotations = ['$schema', '$defs', 'title', 'description', 'default', 'examples'];
function keys(schema, allowed) {
  for (const key of Object.keys(schema)) {
    if (![...annotations, ...allowed].includes(key)) throw new Error('Unsupported schema keyword: ' + key);
  }
}
export function schemaType(schema) {
  if (schema === true) return 'unknown'; // serde_json::Value: standard WebAuthn data only.
  if (schema === false) return 'never';
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) throw new Error('Invalid schema');
  if (schema.$ref) {
    keys(schema, ['$ref']);
    if (!/^#\/\$defs\/[A-Za-z][A-Za-z0-9_]*$/.test(schema.$ref)) throw new Error('Unsupported schema reference');
    return schema.$ref.split('/').at(-1);
  }
  for (const union of ['oneOf', 'anyOf']) {
    if (schema[union]) {
      keys(schema, [union]);
      if (!Array.isArray(schema[union]) || !schema[union].length) throw new Error('Empty schema union');
      return '(' + schema[union].map(schemaType).join(' | ') + ')';
    }
  }
  if (schema.enum || Object.hasOwn(schema, 'const')) {
    keys(schema, ['enum', 'const', 'type']);
    const values = schema.enum ?? [schema.const];
    if (values.some(v => v !== null && !['string', 'number', 'boolean'].includes(typeof v)))
      throw new Error('Unsupported enum value');
    return values.map(v => JSON.stringify(v)).join(' | ');
  }
  if (Array.isArray(schema.type)) {
    return '(' + schema.type.map(type => schemaType({ ...schema, type })).join(' | ') + ')';
  }
  switch (schema.type) {
    case 'null': keys(schema, ['type']); return 'null';
    case 'string': keys(schema, ['type', 'pattern', 'minLength', 'maxLength']); return 'string';
    case 'boolean': keys(schema, ['type']); return 'boolean';
    case 'number': case 'integer':
      keys(schema, ['type', 'format', 'minimum', 'maximum', 'exclusiveMinimum', 'exclusiveMaximum', 'multipleOf']);
      if (schema.format && !['uint8', 'uint32', 'int32', 'double', 'float'].includes(schema.format))
        throw new Error('Unsupported numeric format: ' + schema.format);
      return 'number';
    case 'array': {
      keys(schema, ['type', 'items', 'prefixItems', 'minItems', 'maxItems', 'uniqueItems']);
      if (schema.prefixItems) {
        if (schema.minItems !== schema.prefixItems.length || schema.maxItems !== schema.prefixItems.length)
          throw new Error('Only fixed tuples are supported');
        return '[' + schema.prefixItems.map(schemaType).join(', ') + ']';
      }
      if (!Object.hasOwn(schema, 'items')) throw new Error('Array items missing');
      return 'Array<' + schemaType(schema.items) + '>';
    }
    case 'object':
      keys(schema, ['type', 'properties', 'required', 'additionalProperties']);
      if (schema.additionalProperties !== false) throw new Error('Open objects are unsupported');
      if ((schema.required ?? []).some(key => !Object.hasOwn(schema.properties ?? {}, key)))
        throw new Error('Required property lacks schema');
      return '{\n' + Object.entries(schema.properties ?? {}).map(([key, value]) =>
        '  ' + JSON.stringify(key) + (schema.required?.includes(key) ? '' : '?') + ': ' + schemaType(value) + ';').join('\n') + '\n}';
    default:
      if (Object.keys(schema).every(key => annotations.includes(key))) return 'unknown';
      throw new Error('Unsupported schema construct');
  }
}

// Compare validation structure, not the deliberately lossy TypeScript rendering.
// Only top-level annotations/definition containers are removed: every root's
// definitions are independently collected and validated below, even if unused.
function sameSchema(first, second) {
  const canonical = value => {
    if (Array.isArray(value)) return value.map(canonical);
    if (value && typeof value === 'object')
      return Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])]));
    return value;
  };
  const validation = schema => schema && typeof schema === 'object' && !Array.isArray(schema)
    ? Object.fromEntries(Object.entries(schema).filter(([key]) => !annotations.includes(key))) : schema;
  return JSON.stringify(canonical(validation(first))) === JSON.stringify(canonical(validation(second)));
}

export function declarations(roots) {
  const definitions = new Map();
  const collect = schema => {
    if (!schema || typeof schema !== 'object' || Array.isArray(schema)) return;
    for (const [name, value] of Object.entries(schema.$defs ?? {})) {
      if (definitions.has(name) && !sameSchema(definitions.get(name), value))
        throw new Error('Conflicting schema definition: ' + name);
      definitions.set(name, value);
      collect(value);
    }
    // Follow schema positions only: defaults/examples/property names are data,
    // not definition containers. Nested unused definitions must also be checked.
    for (const value of Object.values(schema.properties ?? {})) collect(value);
    for (const key of ['items', 'additionalProperties']) collect(schema[key]);
    for (const key of ['oneOf', 'anyOf', 'prefixItems'])
      if (Array.isArray(schema[key])) schema[key].forEach(collect);
  };
  Object.values(roots).forEach(collect);
  for (const [name, value] of Object.entries(roots)) {
    if (definitions.has(name) && !sameSchema(definitions.get(name), value))
      throw new Error('Root collides with definition: ' + name);
    definitions.set(name, value);
  }
  // Traverse every definition, including otherwise unreferenced ones, so unsupported
  // constructs can never disappear behind a reference or silently widen consumers.
  const rendered = [...definitions].map(([name, schema]) => {
    const type = schemaType(schema);
    for (const ref of JSON.stringify(schema).matchAll(/"\$ref":"#\/\$defs\/([A-Za-z][A-Za-z0-9_]*)"/g))
      if (!definitions.has(ref[1])) throw new Error('Unresolved schema reference: ' + ref[1]);
    return 'export type ' + name + ' = ' + type + ';\n';
  });
  return '// Generated from actual Rust handler DTOs. Do not edit.\n' + rendered.join('');
}
