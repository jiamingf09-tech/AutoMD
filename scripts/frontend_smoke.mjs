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

// UI was split out of App.tsx into components/; scan the whole frontend surface.
function collectFrontendSources(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      collectFrontendSources(full, out);
    } else if (/\.(tsx?|jsx?)$/.test(entry.name)) {
      out.push(fs.readFileSync(full, "utf8"));
    }
  }
  return out;
}
const frontendSource = [app, ...collectFrontendSources(path.join(root, "src"))].join("\n");

const requiredTabs = ["项目", "流程", "运行", "远程", "报告", "引擎", "插件"];
for (const label of requiredTabs) {
  check(app.includes(`label: "${label}"`), `missing tab label: ${label}`);
}
check(app.includes(`activeTab === "pluginDetail"`), "missing dynamic plugin detail view");
check(!app.includes(`label: "总览"`), "old 总览 tab should not be present");

const requiredUiText = [
  "请先检查引擎配置",
  "快速切换项目",
  "打开文件夹",
  "创建项目",
  "浏览",
  "导入到 inputs/",
  "等待结构导入",
  "自动查找",
  "自动扫描",
  "手动查找",
  "手动登记",
  "自动安装",
  "不适用",
  "生成结构准备文件",
  "高级：当前引擎原生参数预览",
  "一键安装/修复科学环境",
  "生产模拟长度 (ns)",
  "参数检查",
  "生成批量实验包",
  "启动本地任务",
  "取消任务",
  "刷新 artifact",
  "生成远程执行包",
  "运行高级部署",
  "目标设备",
  "连接服务器 / HPC",
  "远程助手",
  "安装远程助手",
  "确认要跑的计划",
  "预检并提交",
  "上传并提交作业",
  "下载结果到本地",
  "生成分析包",
  "导出 MD",
  "导出 HTML",
  "导出 PDF",
  "打开插件目录",
  "导入插件",
  "快速创建并启用",
  "用户插件",
  "插件详情",
  "沙盒运行",
  "直接运行",
  "直接运行插件？",
  "插件构建与接入指引",
  "内置插件只读",
  "插件目录由当前系统的应用数据目录动态生成",
  "后台任务",
  "GPU 状态检测中",
  "未选中结构",
  "拒绝生成或发送分子动力学运行指令",
];
for (const text of requiredUiText) {
  check(frontendSource.includes(text), `missing workflow UI text: ${text}`);
}
check(!frontendSource.includes("远程 profile 模板"), "old remote profile-template panel should not be present");
check(frontendSource.includes("function defaultRemoteWorkdir") || frontendSource.includes("export function defaultRemoteWorkdir"), "missing username-aware remote workdir defaulting");
check(frontendSource.includes("/home/${user}/automd"), "non-root remote users should default to their home workdir");
check(frontendSource.includes("isAutoManagedRemoteWorkdir"), "remote username changes should preserve manually edited workdirs");

check(
  mockData.includes("mkdir -p logs && (nohup bash") && mockData.includes("< /dev/null & echo $!)"),
  "mock SSH direct submit command must detach nohup from stdin and echo the PID inside the subshell"
);
check(
  !mockData.includes("logs/automd-ssh.err & echo $!'"),
  "mock SSH direct submit command still contains the old non-detaching nohup form"
);

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
  "Desktop installers",
  "deb,appimage",
  "nsis,msi",
  // macOS CI ships a zipped .app (headless DMG create is unreliable on runners)
  "bundles: app",
  "bundle/deb/*.deb",
  "bundle/appimage/*.AppImage",
  "bundle/nsis/*-setup.exe",
  "bundle/msi/*.msi",
  "AutoMD_*.app.zip",
  "codesign --verify",
  "actions/upload-artifact@v4",
];
for (const text of requiredCiText) {
  check(ci.includes(text), `CI workflow is missing required text: ${text}`);
}
check(!ci.includes("--no-bundle --ci"), "CI workflow should upload installer bundles, not plain binaries");

if (failures.length > 0) {
  console.error("AutoMD frontend smoke failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(`AutoMD frontend smoke passed: ${commandNames.length} Tauri commands, ${requiredTabs.length + 1} tab/views, ${requiredUiText.length} UI feature labels, CI matrix present.`);
