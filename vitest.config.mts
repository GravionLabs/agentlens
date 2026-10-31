import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.spec.ts", "site/tests/**/*.spec.ts"],
    environment: "node",
    setupFiles: ["./vitest.setup.ts"],
    // Node 25+ defines a global localStorage that shadows the one from jsdom.
    execArgv: ["--no-experimental-webstorage"],
  },
});
