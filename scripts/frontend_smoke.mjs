import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
const exists = (file) => fs.existsSync(path.join(root, file));
const failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

const app = read("src/App.tsx");
const api = read("src/lib/api.ts");
const mockData = read("src/lib/mockData.ts");
const lib = read("src-tauri/src/lib.rs");
const index = read("dist/index.html");
const ci = read(".github/workflows/ci.yml");

const requiredTabs = ["总览", "项目", "引擎", "流程", "运行", "远程", "编译", "插件", "报告"];
for (const label of requiredTabs) {
  check(app.includes(`label: "${label}"`), `missing tab label: ${label}`);
}

const requiredUiText = [
  "创建项目",
  "导入到 inputs/",
  "生成结构准备包",
  "多引擎参数映射",
  "生成 batch package",
  "启动本地任务",
  "取消任务",
  "刷新 artifact",
  "生成远程执行包",
  "运行构建向导",
  "生成分析包",
  "导出 MD",
  "导出 HTML",
  "导出 PDF",
];
for (const text of requiredUiText) {
  check(app.includes(text), `missing workflow UI text: ${text}`);
}

const commandNames = [...api.matchAll(/call<[^>]+>\("([^"]+)"/g)].map((match) => match[1]);
check(commandNames.length > 25, "expected many Tauri command calls in src/lib/api.ts");

for (const command of commandNames) {
  check(lib.includes(`fn ${command}`), `Tauri command function is missing in lib.rs: ${command}`);
  check(lib.includes(command), `Tauri command is not registered in generate_handler!: ${command}`);
}

const mockFallbackNames = [...api.matchAll(/\(\)\s*=>\s*(mock[A-Za-z0-9_]+)/g)]
  .map((match) => match[1])
  .filter((name) => name !== "mockStartLocalRun" && name !== "mockPlan");
for (const name of mockFallbackNames) {
  const hasFunction = mockData.includes(`function ${name}`) || mockData.includes(`const ${name}`) || mockData.includes(`export const ${name}`);
  check(hasFunction, `mock fallback is imported but not defined: ${name}`);
}

const requiredBuiltAssets = [...index.matchAll(/(?:src|href)="([^"]+)"/g)].map((match) => match[1]);
check(requiredBuiltAssets.length > 0, "dist/index.html does not reference built assets");
for (const asset of requiredBuiltAssets) {
  const normalized = asset.replace(/^\//, "");
  check(exists(path.join("dist", normalized)), `built asset referenced by index.html is missing: ${asset}`);
}

const requiredDocs = [
  "docs/ENGINE_ADAPTERS.md",
  "docs/PROJECT_FORMAT.md",
  "docs/SCIENCE_SIDECAR.md",
  "docs/PLUGIN_MANIFESTS.md",
  "docs/RELEASE.md",
];
for (const doc of requiredDocs) {
  check(exists(doc), `missing documentation file: ${doc}`);
}

const requiredCiText = [
  "ubuntu-22.04",
  "windows-latest",
  "macos-14",
  "npm run check",
  "tauri -- build --no-bundle --ci",
  "tauri:build:app",
  "actions/upload-artifact@v4",
];
for (const text of requiredCiText) {
  check(ci.includes(text), `CI workflow is missing required text: ${text}`);
}

if (failures.length > 0) {
  console.error("AutoMD frontend smoke failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(`AutoMD frontend smoke passed: ${commandNames.length} Tauri commands, ${requiredTabs.length} tabs, ${requiredUiText.length} UI feature labels, CI matrix present.`);
