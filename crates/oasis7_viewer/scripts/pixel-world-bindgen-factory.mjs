export function isolateBindgenModule(source) {
  if ((source.match(/\blet (?:wasmModule, wasmInstance, )?wasm;/g) || []).length !== 1
      || /\bimport\b\s*(?:[\w*{]|['"])/m.test(source)
      || /\bexport (?:let|var)\b/.test(source)) throw new Error("unsupported wasm-bindgen factory output shape");
  let defaultExports = 0;
  const names = [...source.matchAll(/\bexport (?:class|function|async function|const|let|var) (\w+)/g)].map(match => match[1]);
  const aliases = [];
  const body = source.replace(/export default __wbg_init;/g, () => { defaultExports += 1; return ""; }).replace(/export \{([^}]*)\};?/g, (_, list) => {
    for (const item of list.split(",").map(value => value.trim()).filter(Boolean)) {
      const match = item.match(/^(\w+)(?: as (\w+))?$/);
      if (!match) throw new Error("unsupported wasm-bindgen export alias");
      const name = match[2] || match[1];
      if (name === "default") {
        if (match[1] !== "__wbg_init") throw new Error("unsupported wasm-bindgen default export");
        defaultExports += 1;
      } else aliases.push([name, match[1]]);
    }
    return "";
  }).replace(/\bexport (?=(?:class|function|async function|const|let|var) )/g, "");
  if (defaultExports !== 1 || /\bexport\b/.test(body) || !names.includes("PixelWorldBridge") || !names.includes("build_pixel_world_render_state_json")) throw new Error("unhandled wasm-bindgen export contract");
  const entries = [...names.map(name => [name, name]), ...aliases];
  const allNames = entries.map(([name]) => name);
  if (new Set(allNames).size !== allNames.length) throw new Error("duplicate wasm-bindgen export");
  return `export function createPixelWorldModule() {\n${body}\nreturn { default: __wbg_init, ${entries.map(([name, local]) => `${name}: ${local}`).join(", ")} };\n}\nconst legacyModule = createPixelWorldModule();\nexport default legacyModule.default;\n${allNames.map(name => `export const ${name} = legacyModule.${name};`).join("\n")}\n`;
}
