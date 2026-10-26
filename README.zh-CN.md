<p align="center">
  <img src="assets/icon.png" width="80" />
</p>

<h1 align="center">Skills Manager</h1>

<p align="center">
  开源 <a href="https://github.com/vercel-labs/skills">Vercel <code>skills</code> CLI</a> 的桌面前端。
</p>

<p align="center">
  <a href="./README.md">English</a>
</p>

---

Skills Manager 2.x 是 Vercel `skills` CLI 的**薄 GUI**：

- **没有自己的数据库。** 技能存放在 CLI 管理的位置——全局
  `~/.agents/skills`，项目内 `<project>/.agents/skills` +
  `skills-lock.json`——没有本应用也能被 CLI 和所有兼容 Agent 继续使用。
- **CLI 是唯一的写入者。** 安装、更新、卸载一律通过 `skills add` /
  `update` / `remove` 执行。GUI 绝不复制技能文件、绝不改写 CLI 的锁文件。
- **降级只读。** CLI 缺失或版本过旧时，应用仍能浏览磁盘上的内容，并
  给出修复指引。

> **破坏性变更：** 2.0 是对 1.x 架构的破坏性重写（SQLite 存储、私有中央
> 仓库、Preset、标签、Git 备份、多设备同步和自带的 `skills-manager-cli`
> 均已删除）。`~/.skills-manager` 下的旧数据**不会被读取、迁移或删除**——
> 确认不再需要后请自行处理。作为 fork，应用内自动更新已移除，请从本
> 仓库下载发布版本。

## 环境要求

- Node.js ≥ 22
- 支持写操作的 Vercel skills CLI：

  ```bash
  npm install -g skills@^1.7.0
  ```

  旧版本 CLI（如 1.5.x）仍可列出技能，但写操作会禁用直至升级。支持的
  命令、输出结构见 [docs/CLI_CONTRACT.md](docs/CLI_CONTRACT.md)。

## 功能

- **技能库** —— 全局技能（共享目录 + 各 Agent 目录）去重合并展示，来源
  元数据读自 CLI 锁文件，按 Agent 展示可见性，提供安全的安装/更新/卸载。
- **Agent** —— CLI 已知的 Agent 哪些已安装、各自能看到什么
  （共享 / 链接 / 复制 / 原生）。
- **项目** —— 一次打开一个文件夹，管理其 `.agents/skills` 与锁文件，
  没有项目注册表。
- **市场** —— skills.sh 搜索；安装仍走 CLI，作用域与 Agent 显式选择。
- **诊断** —— Node/CLI 路径与版本、锁文件位置、最近 CLI 错误、panic
  日志与日志摘录。

## 开发

```bash
npm install
npm run tauri:dev    # 开发模式桌面应用
npm test             # vitest
npm run lint         # eslint + UI 规范 + i18n 检查
npm run build        # 前端构建

cd src-tauri
cargo test           # 单元 + 合同测试（隔离 HOME，真实 skills CLI）
```

合同测试会把 `skills@1.7.4` 安装到一次性 npm 前缀，并在隔离的 HOME 中
执行真实的安装/卸载/更新流程——绝不会碰你真实的 `~/.agents`。

## 架构

分层、数据归属与安全规则见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)；
CLI 集成合同见 [docs/CLI_CONTRACT.md](docs/CLI_CONTRACT.md)。

## 致谢

Fork 自 [xingkongliang/skills-manager](https://github.com/xingkongliang/skills-manager)
（MIT），围绕 Vercel skills CLI 重建。技能基础设施来自
[vercel-labs/skills](https://github.com/vercel-labs/skills)。
