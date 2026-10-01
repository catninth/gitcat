import { build } from "esbuild";
import { fileURLToPath } from "node:url";

const entryPoints = [
  fileURLToPath(new URL("../tests/columns.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/commitGraphWindow.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/forge.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/forgeConnections.test.tsx", import.meta.url)),
  fileURLToPath(new URL("../tests/graphPresentation.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/mergeConflicts.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/mutationQueue.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/paths.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/themePresets.test.ts", import.meta.url)),
  fileURLToPath(new URL("../tests/updates.test.ts", import.meta.url)),
];

for (const entryPoint of entryPoints) {
  const result = await build({
    entryPoints: [entryPoint],
    bundle: true,
    format: "esm",
    platform: "node",
    jsx: "automatic",
    // React's server renderer uses require for Node built-ins in this ESM bundle.
    banner: {
      js: `import { createRequire } from "node:module"; const require = createRequire(${JSON.stringify(import.meta.url)});`,
    },
    target: "node20",
    write: false,
  });
  const source = result.outputFiles[0].text;
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
  await import(moduleUrl);
}
