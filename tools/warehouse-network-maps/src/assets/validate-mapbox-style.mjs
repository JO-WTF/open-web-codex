import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const nodeRoot = process.env.OPEN_WEB_CODEX_MAPS_NODE_ENV;
if (!nodeRoot) throw new Error("OPEN_WEB_CODEX_MAPS_NODE_ENV is required");
const require = createRequire(import.meta.url);
const validatorPath = require.resolve("@mapbox/mapbox-gl-style-spec", {
  paths: [nodeRoot],
});
const { validate } = await import(pathToFileURL(validatorPath));

let input = "";
for await (const chunk of process.stdin) input += chunk;

try {
  const style = JSON.parse(input);
  const diagnostics = validate(style).map((diagnostic) => ({
    severity:
      diagnostic?.constructor?.name === "ValidationWarning" ? "warning" : "error",
    message: String(diagnostic?.message ?? diagnostic),
  }));
  process.stdout.write(JSON.stringify({ diagnostics }));
} catch (error) {
  process.stdout.write(
    JSON.stringify({
      diagnostics: [
        {
          severity: "error",
          message:
            error instanceof Error ? error.message : "Mapbox style validation failed",
        },
      ],
    }),
  );
}
