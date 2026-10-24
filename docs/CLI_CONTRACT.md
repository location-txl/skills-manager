# Vercel `skills` CLI Contract

> 上游：https://github.com/vercel-labs/skills
> **合同锚定版本：`skills@1.7.4`**（全局锁 v3、项目锁 v1，Node ≥ 22.20）。
> 本文档记录本 GUI 实际依赖的命令、参数、输出结构与已知限制。升级 CLI
> 版本后必须重跑 `cargo test`（含合同测试）与端到端验证后再发布。

## 1. 调用方式

- 入口：`node <pkg>/bin/cli.mjs <args…>`。`bin/cli.mjs` 必须存在——
  `update` 会以自身包内相对路径重新 spawn `bin/cli.mjs`（缺失则更新失败）。
  因此只支持 npm 完整安装的包，不支持只拷 `dist/cli.mjs`。
- 绝不经过 shell；argv 数组；Windows 不碰 `.cmd` shim（从 shim 反解出
  `node_modules/skills/bin/cli.mjs`）。
- 子进程环境为白名单：`HOME/PATH/XDG_*/代理/GITHUB_TOKEN` 等少数变量 +
  `NO_COLOR=1` + `DISABLE_TELEMETRY=1`（顺带消除退出前最长 5s 的
  telemetry flush 等待）。检测 agent 用的环境变量（`CLAUDECODE`、
  `CURSOR_*` 等）**不在白名单内**——防止 CLI 误以为运行在 agent 里而
  改变交互行为。
- `stdin = null`：任何意外交互提示立即 EOF 失败而不是挂起 UI。写操作
  一律显式 `-y`。
- 超时：读 60s；写 10min（git clone 可能耗时）。可取消（unix 杀整个
  进程组，Windows `taskkill /T /F`）。stdout/stderr 各限 2MB。

## 2. 版本政策

| 能力 | 要求 | 说明 |
|---|---|---|
| 列表 `ls --json` | ≥ 1.5.x | 1.5.x 只有 `name/path/scope/agents` 4 字段；1.7.4 起含 `source/sourceUrl/sourceType`（读自锁文件）。解析按字段缺省容忍 |
| 写操作（add/remove/update） | **≥ 1.7.0** | `add --json` 于 1.7.0 引入。低于此版本 GUI 禁用全部写操作并提示升级 `npm install -g skills@^1.7.4` |
| 锁文件 v3 / v1 | 1.5.x 与 1.7.4 相同 | 路径与 schema 见 §5 |

本机开发环境实测 1.5.1：`ls -g --json` 正常（4 字段），锁 v3 正常；
写操作被正确禁用并提示升级。

## 3. 命令清单

### 3.1 `skills ls -g --json` / 项目 `skills ls --json`（cwd=项目根）

- 输出：JSON 数组，元素 `{name, path, scope, agents[], source?, sourceUrl?, sourceType?}`。
  `name` 是 **SKILL.md frontmatter 的 name（即锁 key）**，`path` 是目录，
  `agents` 是 **displayName 列表**（不是 agent key）。
- 无 `-g` 时列项目作用域（cwd 决定）；`-a <keys>` 可过滤。
- 空结果输出 `[]`；无效 agent 退出码 1。
- `scope` 字段：`global`/`project`。

### 3.2 `skills add <source> -a <keys…> [-s <names…>] -y [-g] [--json] [--copy]`

- **`--json` 前置条件**：必须带 `-y`（或 `--all`），否则退出 1：
  `The --json flag requires --yes (or --all) to run non-interactively.`
- **`--json` 不支持 well-known 源**（任意非 github/gitlab 的 http(s) URL，
  含 skills.sh 页面链接），否则退出 1。GUI 对这类源改用非 JSON 模式 +
  退出码 + 事后重扫描验证。
- `-a` 的值是 agent key（受控映射表，见 §4）或 `'*'`（全部 79 个——
  **全局安装时含必失败的 eve/promptscript，禁用 `'*'`**，GUI 自行展开）。
- `-s` 是技能名（frontmatter name，大小写不敏感）或 `'*'`。
- 成功条目：`{name, status:"installed", source, ref, hash, path, scope, agents[](displayName), mode, security}`；
  失败：`{name, status:"failed", error}`；源中不存在：`{name, status:"skipped", reason}`。
- **退出码**：JSON 模式下任何 failed/skipped → 1。**非 JSON 模式部分
  agent 失败仍退出 0**——绝不能只看退出码。
- 默认 symlink 模式：技能进 canonical 目录（全局 `~/.agents/skills`），
  非 universal agent 目录建相对符号链接；universal agent 直读 canonical。
  `--copy` 改为逐 agent 复制。项目安装时，未显式指定的 agent 若项目根
  目录不存在会被跳过（结果标 skipped）。
- 交互点（技能多选/agent 选择/scope/方式/总确认）全部可被 `-y` +
  显式 `-a/-s/-g/--copy` 跳过。

### 3.3 `skills remove <name> -g -y [-a <key>]`

- `name` 匹配：先按磁盘文件夹名、再按锁 key（经 `sanitizeName` 归一）。
  GUI 一律传文件夹 slug。
- 无 `-a` 时清理所有 agent 的链接；仍被其他 agent 使用的 canonical 目录
  与锁记录保留。
- 输出为人类可读文本；无稳定 `--json`。结果以退出码 + 事后重扫描判定。
- `--all` 与具名技能互斥（退出 1）。GUI 不使用 `--all`。

### 3.4 `skills update <name> -g -y`（项目：`-p -y`，cwd=项目根）

- **匹配规则：锁 key（frontmatter name），大小写不敏感全等**，不匹配
  文件夹名。GUI 传 `SkillView.name`，只对 `isTrackedByVercel` 的技能开放。
- `check`/`upgrade` 是 `update` 的别名——**同一入口，绝不自动调用**
  （不会出现在启动、刷新、后台任务中）。
- 无 `--json`；输出人类可读。退出码 0/1。
- 更新机制 = 重新 `add`：上游哈希变化才重装，**无本地修改提示**，直接
  `rm -rf` 重建 canonical 目录。GUI 在触发前统一显示覆盖警告。
- 非 TTY 且无 `-y` 时自动选 scope（cwd 有 `skills-lock.json` 或
  `.agents/skills` 内容 → project，否则 global）；GUI 恒显式 `-g`/`-p`。

### 3.5 禁用命令

`check`、`experimental_sync`、`experimental_install`、`use`、`init`、
`find`（交互式 TUI）——GUI 一律不调用。市场搜索直连
`https://skills.sh/api/search`（见 §6），与 `find` 相同的端点。

## 4. Agent 映射（skills@1.7.4 快照，79 个）

`ls --json` 的 `agents` 是 displayName；`-a` 要 agent key。GUI 内置
`core/agents.rs` 静态映射表做翻译：

- 已知 key：可显示、可传给 `-a`。
- 未知 displayName（更新的 CLI 新增的 agent）：显示为 unknown，**不可操作**。
- 1.7.4 的 79 键中 `eve`、`promptscript` 无全局作用域；全局"全部 agent"
  展开为排除二者后的列表。
- `sanitizeName`（installer.ts）：`lowercase → 非 [a-z0-9._] 连续段折叠为
  单个 '-' → 去首尾 '.'/'-' → 截 255 → 空则 'unnamed-skill'`。锁 key 保留
  原名，文件夹用 sanitize 后的名字。

## 5. 锁文件（只读，绝不写）

- 全局 v3：`$XDG_STATE_HOME/skills/.skill-lock.json`（变量非空时），否则
  `~/.agents/.skill-lock.json`。`{version:3, skills:{<name>:{source,
  sourceType, sourceUrl, ref?, skillPath?, skillFolderHash, installedAt,
  updatedAt, pluginName?, …}}, dismissed?, lastSelectedAgents?}`。
- 项目 v1：`<project>/skills-lock.json`。`{version:1, skills:{<name>:{source,
  sourceUrl?, ref?, sourceType, skillPath?, computedHash, …}}}`（无时间戳）。
- CLI 对 version<当前版的锁按空处理（下次写入时重置）；本 GUI 对
  version≠锚定值显示"元数据不可用"诊断，**不重写**。
- 锁记录残留（有记录无文件）：仅诊断，不修改。

## 6. skills.sh HTTP API（外部接口，非永久承诺）

- 搜索：`GET https://skills.sh/api/search?q=<query>&limit=20[&owner=<owner>]`
  → `{skills:[{id, name, installs, source}]}`。`id` 形如
  `owner/repo@skill`，`source` 是可安装的 `owner/repo`。无分页。
- 本 GUI 仅使用该搜索端点；下载/安装一律走 CLI。
- `owner` 校验：`^[a-z0-9](?:[a-z0-9-]{0,38})$/i`。

## 7. 目录布局（CLI 拥有）

```
~/.agents/skills/<sanitize(name)>/SKILL.md     # 全局 canonical
~/.agents/.skill-lock.json                      # 全局锁（无 XDG 时）
$XDG_STATE_HOME/skills/.skill-lock.json         # 全局锁（有 XDG 时）
~/.claude/skills/<name> -> ~/.agents/skills/<name>   # 非 universal agent 链接
<project>/.agents/skills/<name>/SKILL.md       # 项目 canonical
<project>/skills-lock.json                      # 项目锁 v1
```

universal agent（`skillsDir == .agents/skills`，24 个）项目作用域直读
canonical 目录；全局作用域各有自己的目录（其中 9 个的 global 目录就是
`~/.agents/skills` 本身——这些 agent 始终能看见共享目录内容，UI 措辞
不得承诺"从某 agent 移除后它就看不见了"）。

## 8. 与旧版（1.5.1，本机实测）的差异备忘

- `add --json` 完全不存在（1.7.0 引入）；`--metadata`/`--subagent` 同。
- agent 表 45 个 vs 79 个；codex/kilo/pi/droid/kimi 的目录布局有变。
- 搜索 API：1.5.1 `limit=10`、无 `owner`。
- telemetry 域名不同；禁用变量相同。
- agent 自动检测（CLAUDECODE 等自动 `-y`）是 1.6+ 行为；1.5.1 只认显式 `-y`。

## 9. 已知限制

- `update` 无结构化输出：成功与否靠退出码 + 重扫描；"更新了什么"只能
  展示 CLI 文本摘要。
- 非 JSON `add` 的部分失败退出 0（本 GUI 对 well-known 源只能走此路径）：
  依赖事后扫描差异报告真实结果。
- `remove` 限定 `-a` 时其他 agent 的链接保留，canonical 目录是否删除由
  CLI 依据"是否仍被使用"决定——GUI 不预测，重扫描后如实展示。
- CLI 的 `ls`/`remove` 只扫到"检测为已安装"的 agent + 磁盘上实际存在的
  agent 目录；映射表快照漂移时以 CLI 输出为准。
