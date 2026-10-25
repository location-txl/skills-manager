# Skills Manager → Vercel Skills GUI：完整破坏性重构执行方案

> **任务性质：破坏性重构；直接修改代码，不做旧数据迁移。**
>
> **目标仓库：** https://github.com/xingkongliang/skills-manager （以执行时实际 checkout 的默认分支为准）  
> **底层依赖：** https://github.com/vercel-labs/skills （Vercel `skills` CLI）  
> **源码核查基线：** Skills Manager 1.43.0；Vercel `skills` 包 1.7.4、全局锁 v3、项目锁 v1。执行时重新确认版本与行为，不得凭本文假定将来的 CLI 不会改变。  
> **交付目标：** 可编译、可启动、可管理真实全局和项目 Skills 的 Tauri 桌面客户端；没有自己的 Skill 数据库、私有中央库、业务锁文件和第二套安装/更新/部署逻辑。

## 0. 最高优先级指令（Coding Agent 必须遵守）

1. **不迁移、不兼容、不导入旧数据。** 不写 SQLite 迁移、仓库迁移、旧 Preset 转换、旧元数据转换、配置兼容或首次启动导入向导。允许直接删除旧代码、表结构、备份实现及相应测试。
2. **不要自动删除用户磁盘上的旧数据。** “删除旧数据库管理代码”不等于启动时执行 `rm -rf ~/.skills-manager`。应用不再读取/写入旧库，但原文件保留给用户自行处理；不能误删已安装的 Skill。
3. **唯一的 Skill 本体存储**是 Vercel CLI 所采用的共享路径：全局 `~/.agents/skills/<name>/SKILL.md`，项目 `<project>/.agents/skills/<name>/SKILL.md`。兼容 CLI 已识别的其他 Agent 原生安装位置及符号链接；不要假设全部 Skills 都只存在共享根目录。
4. **来源与更新元数据只读兼容 CLI 原生锁文件：** 全局 `$XDG_STATE_HOME/skills/.skill-lock.json`（设置了该变量时）或 `~/.agents/.skill-lock.json`；项目 `<project>/skills-lock.json`。不创建、不维护自己的 `skills-manager.db`、`metadata.json`、`skills.lock`、`manifest.json` 等业务状态文件；不直接写 Vercel 锁文件。
5. **所有安装、更新、卸载由 Vercel CLI 执行。** GUI 不复制/重写 Vercel 的下载、Git 更新、包解析、锁文件更新、符号链接创建/删除实现。
6. **不是纯只读 UI。** 需要实现查看、搜索、安装、卸载、手动更新、Agent 部署信息和项目作用域；GUI 可以通过显式用户操作调用 CLI 修改状态。
7. **功能做减法。** 删除 Preset/Scenario、标签、自有 Git 备份、多设备同步、自动更新 Skill、旧管理器 CLI、独立 Skill 市场安装器、复杂自定义 Agent 注册等所有依赖旧数据库的功能。不能以 JSON 文件或 localStorage 再暗中重建这些状态。
8. **保留可用的原有 Tauri/React 外壳和基础 UI。** 不要求复刻原功能；页面与状态结构应围绕新领域模型重写。主题/语言等纯视觉首选项可以使用浏览器 localStorage，但绝不能持久化 Skill 注册表、来源、部署状态、项目业务数据。
9. **以实际源码与可重复测试为准。** 不能仅凭 CLI 帮助文字就认定某命令只读、支持 JSON 或保证无交互；版本漂移则优先收缩功能并明确报错，禁止做猜测性解析。
10. **边界明确。** Vercel `skills` CLI 是唯一写入执行引擎；文件系统是“实际是否存在/可读”的事实源；CLI 锁文件是来源/版本追踪信息，而不是文件存在的唯一证据。

## 1. 已核查事实及重要陷阱

### 1.1 原项目的主要耦合

- `src-tauri/src/core/central_repo.rs`：私有中央库路径 `~/.skills-manager`、`skills` 子目录、SQLite/缓存目录、库搬迁逻辑。
- `src-tauri/src/core/skill_store.rs` 与 `migrations.rs`：`rusqlite` 数据库，承载 Skill、目标部署、标签、Preset、项目、设置、缓存等。
- `src-tauri/src/core/app_state.rs` 和 `src-tauri/src/lib.rs`：启动时创建 SQLite、重建元数据、修补部署、启动备份和自动更新；旧入口是深度耦合点，不能只修改 `skills_dir()`。
- `src-tauri/src/core/installer.rs`、`sync_engine.rs`、`scenario_service.rs`、`git_backup.rs`：自有安装、同步、分组、备份体系。
- `src-tauri/src/bin/skills-manager-cli.rs`：一整套旧管理器 CLI；与新目标重复。
- `src/context/AppContext.tsx`、`src/lib/tauri.ts`、`src/views/MySkills.tsx`、`WorkspaceView.tsx`、`ProjectDetail.tsx` 等：依赖旧记录 ID、Preset ID、target 表、标签与专用 IPC；不能把新数据硬塞进旧结构。

### 1.2 Vercel CLI 已确认的行为

- `skills ls -g --json`：返回全局安装的 Skill 列表，包含 `name/path/scope/agents/source/sourceUrl/sourceType`；无 `-g` 时列项目作用域，需将 CLI 进程的工作目录设置为项目目录。
- `skills add <source> -g -a <agent> -s <skill> -y --json`：适合 GUI 的全局非交互安装。`--json` 成功时输出含 `status` 等字段的 JSON 数组；应验证各项结果，不仅看退出码。
- **不能调用** `skills add <source> --list --json`：当前源码明确拒绝 `--list` 与 `--json` 组合。`--list` 返回面向人看的文本，不适合作为结构化仓库预览接口。
- `skills remove <name> -g -y`：全局卸载；支持 `-a` 限定 Agent。当前没有对称的稳定 `--json` 结果合同；应以退出码、日志及操作后的重新扫描判断结果。
- `skills update <name> -g -y`：全局手动更新，输出主要面向人类，不可假设 JSON。
- **特别注意：** 当前 `src/cli.ts` 将 `check/update/upgrade` 都派发到 `runUpdate`。不能把 `skills check` 放到应用启动、定时刷新或后台只读更新检查里；它可能进入更新/移除相关流程。第一版不做自动远端更新检测，也不显示无法保证的“有更新”状态。
- `skills find` 是交互式检索界面，不是已确认的机器可读 API；可以直接请求 CLI 自身使用的 `https://skills.sh/api/search?q=...&limit=20` 来驱动市场搜索列表，再调用 CLI 安装。该 HTTP 接口是外部接口，不作永久稳定假设。
- Vercel 当前 `package.json` 要求 Node.js `>=22.20.0`；初始开发和合同测试固定 `skills@1.7.4`，日后升级必须重新跑测试。

源码：
- https://github.com/vercel-labs/skills/blob/main/src/cli.ts
- https://github.com/vercel-labs/skills/blob/main/src/list.ts
- https://github.com/vercel-labs/skills/blob/main/src/add.ts
- https://github.com/vercel-labs/skills/blob/main/src/update.ts
- https://github.com/vercel-labs/skills/blob/main/src/skill-lock.ts
- https://github.com/vercel-labs/skills/blob/main/src/local-lock.ts
- https://github.com/vercel-labs/skills/blob/main/src/installer.ts
- https://github.com/vercel-labs/skills/blob/main/src/find.ts

## 2. 最终目录与数据归属

```text
~/.agents/
├── skills/
│   ├── my-skill/
│   │   └── SKILL.md
│   └── another-skill/
│       └── SKILL.md
└── .skill-lock.json             # Vercel CLI 拥有（无 XDG_STATE_HOME 时）

$XDG_STATE_HOME/skills/.skill-lock.json  # 如果设置了 XDG_STATE_HOME，锁文件在此处

~/.claude/skills/my-skill -> ~/.agents/skills/my-skill
# 其他 Agent 的专有目录由 CLI 负责；共享扫描型 Agent 不一定需要链接。

<project>/
├── .agents/skills/
│   └── project-skill/SKILL.md
└── skills-lock.json             # Vercel CLI 项目锁文件（v1）

[GUI App]
├── Tauri + React 静态资源
├── 进程内临时状态/监控任务
└── 纯 UI 偏好（可选）
# 无私有 Skill 仓库、数据库、自己的 lock/manifest/backup。
```

**真相来源和冲突处理规则**：

| 情况 | UI 如何处理 |
|---|---|
| 目录存在、`SKILL.md` 有效、锁文件无记录 | 显示为“本地存在 / 来源未知”；允许查看；不能凭空补写 v3。自动更新不可用；删除需先验证 CLI 可以精确定位。 |
| 锁文件有记录、目录不存在 | 不当成“已安装”；可显示独立诊断“锁记录残留”，不自动修改锁文件。 |
| CLI JSON 列表与锁文件字段不一致 | “是否存在/当前哪些 Agent 可看到”以扫描结果为准；来源元数据优先按锁文件的准确键匹配。 |
| 多个 Agent 指向同一物理目录 | 去重为一个 Skill，关联多个 Agent 入口；不能多次计数。 |
| 有断开的软链接 | 显示诊断，不自动删除。 |
| `skills` CLI 不可执行/版本不兼容 | 仍可显示本地文件（降级只读扫描），禁用所有写操作并给出修复入口。 |
| 修改来自外部终端或其他管理器 | 文件变更后重新读取；不写本地缓存来覆盖磁盘结果。 |
| 旧锁文件或未来更高版本 | 不把“解析失败”当成空仓库，更不能重写；显示“元数据不可用/版本未支持”，本体仍以文件系统扫描为准。 |

仅允许使用锁文件的已知字段（可选字段应宽容）：全局 v3 的 `source/sourceType/sourceUrl/ref/skillPath/skillFolderHash/installedAt/updatedAt` 等；项目 v1 的 `source/sourceType/sourceUrl/ref/skillPath/computedHash` 等。锁的 schema 是外部实现细节，不允许直接改写或发明字段。

## 3. 目标分层和代码结构

```text
React Views
  ├── Library / Global Skills
  ├── Agent Workspaces
  ├── Project Skills
  ├── Marketplace / Install
  └── Settings / Runtime Diagnostics
          │
          ▼
Frontend domain API (typed DTO, loading/error states)
          │ Tauri invoke / events
          ▼
Rust application service
  ├── runtime_detector.rs    # Node/CLI 路径和版本/健康检查
  ├── skills_cli.rs          # 统一、受控的 CLI 执行器
  ├── skill_catalog.rs       # 聚合 CLI JSON + 磁盘 + 锁文件
  ├── skill_document.rs      # 安全读取 SKILL.md
  ├── lock_reader.rs         # 仅只读解析 v3/v1
  ├── scope.rs               # 全局/项目路径边界
  ├── marketplace.rs         # skills.sh 搜索；不负责安装
  ├── change_watcher.rs      # 文件变更触发重新扫描
  └── commands.rs            # 精简的 Tauri IPC
          │
          ├── Vercel skills CLI  (唯一写入执行引擎)
          ├── ~/.agents/...     (只读扫描)
          └── <project>/...    (只读扫描)
```

目录名是建议，不要求机械照抄；职责和依赖方向必须遵守。Rust 可以保留 `core/error.rs`、少量无状态路径/文件工具及 Tauri 插件，但不得继续依赖 `SkillStore`。不能仅将旧 `SkillStore` 改名后再基于内存 Map 复刻原系统。

### 3.1 统一领域模型（前后端同构语义）

```ts
type Scope =
  | { kind: 'global' }
  | { kind: 'project'; root: string }; // 使用已校验的绝对路径

type SkillStatus =
  | 'installed'
  | 'untracked'
  | 'broken-link'
  | 'metadata-unsupported';

type AgentExposure = 'shared' | 'symlink' | 'copy' | 'native' | 'unknown';

type SkillView = {
  id: string;                     // scope + 规范化路径；仅用于 UI，不另存数据库
  slug: string;                   // 真实文件夹名/CLI 标识
  name: string;                   // SKILL.md/CLI 解析后的展示名
  description: string | null;
  scope: Scope;
  path: string;                   // 完整安装路径
  status: SkillStatus;
  source: string | null;
  sourceUrl: string | null;
  sourceType: string | null;
  installedAt: string | null;
  updatedAt: string | null;
  agents: Array<{ id: string; name: string; exposure: AgentExposure }>;
  isTrackedByVercel: boolean;
};

type OperationResult = {
  success: boolean;
  exitCode: number | null;
  message: string;
  stdout?: string;               // 对非 JSON 命令仅作为人类可读信息
  stderr?: string;
};
```

不可再使用旧数据库 UUID、`preset_ids`、`skill_targets` 行 ID 作为业务身份。CLI `ls --json` 里的 `agents` 是**展示名称**而不是稳定 Agent ID；发给 `-a` 的 key 只能来自受控 Agent 映射，绝不能把展示名称原样当作 key。

### 3.2 CLI 执行器合同

- 优先使用用户明确指定或探测确认的 **绝对路径**，从 `skills --version`、`node --version` 校验健康状态；考虑 macOS 从 Finder 启动时 PATH 不完整。
- 初版要求用户已安装 Node.js 和 Vercel `skills` CLI，例如 `npm install -g skills@1.7.4`；不自动 `npx` 下载依赖，不静默升级全局 npm 包。Node 及 CLI 不存在时有清晰的安装说明。
- Windows 的 `.cmd` shim 不应通过拼接 shell 命令执行；推荐定位 `node.exe` 和实际 CLI `cli.mjs`，用 `Command` / `execFile` 类参数数组调用。macOS/Linux 同样禁止 `sh -c` / 拼接字符串命令。
- `Command` 的工作目录：全局操作固定使用安全有效目录；项目操作必须 `current_dir(projectRoot)`，不能意外落到应用自身工程或安装目录。
- 进程 `stdin=null`；所有 GUI 操作用显式非交互标记；设置 stdout/stderr 大小上限、超时、取消、退出码和分项 JSON 结果校验。
- 写操作经一个全局队列串行化；文件扫描可并发，写操作结束必须重新扫描。外部终端仍可能并发修改，需要操作后重新核验，不声称跨进程独占锁。
- 不接受前端传来的任意 argv；IPC 必须是 `install_skill/remove_skill/update_skill` 等明确操作，Rust 负责构建 allowlist 参数。
- `--yes` 仅在 GUI 已让用户明确确认源、作用域、Agent、覆盖/删除风险之后才可以传给 CLI。
- stdout/stderr 经过敏感信息脱敏再呈现；不记录令牌、带凭证 URL、完整环境变量。拒绝未经确认的危险协议/不可信来源，允许用户在确认后使用 CLI 支持的来源形式。

## 4. CLI 操作接口和行为

| GUI 动作 | CLI/数据路径 | 规则 |
|---|---|---|
| 检查运行时 | `node --version`、`skills --version` | 解析已验证版本，失败只读降级 |
| 列全局 Skill | `skills ls -g --json` | JSON 数组，读取后与文件和 v3 锁对齐 |
| 列项目 Skill | 在项目根 `skills ls --json` | cwd 必须由文件夹选择器确认；与 v1 锁对齐 |
| 展示 SKILL.md | Rust 只读文件接口 | 验证 Skill 属于当前列出的根目录或可信软链接；限制体积 |
| 安装全局 | `skills add <source> -g -a <agentId...> -s <skillName...> -y --json` | 明确要求用户选择 scope、Agent；不允许无意遗漏 `-g` |
| 安装项目 | `skills add <source> -a <agentId...> -s <skillName...> -y --json`，cwd 为项目 | 不能写到 GUI 自身的 cwd |
| 删除全局 | `skills remove <name> -g -y` | GUI 二次确认；不直接 `rm -rf` |
| 解除某 Agent 部署 | `skills remove <name> -g -a <agentId> -y` | 可能仍通过共享目录被该 Agent 看见；UI 不能承诺彻底禁用 |
| 删除项目 | 在项目根 `skills remove <name> -y` | 明确作用域；CLI 成功后核验 |
| 更新全局 | `skills update <name> -g -y` | 显式用户触发；可能覆盖/重建本地内容 |
| 更新项目 | 在项目根 `skills update <name> -p -y` | 仅手动触发 |
| 搜索市场 | HTTP GET `https://skills.sh/api/search?q=...&limit=20` | 搜索只读；安装仍经 CLI |
| 查看某仓库有哪些 Skills | 可展示 `skills add <source> --list` 的只读文本输出 | 不得与 `--json` 混用；第一版可直接省略结构化预览 |

### 4.1 严禁错误调用

- **严禁后台自动运行** `skills check`、`skills update`、`skills add`、`skills experimental_sync`、`skills experimental_install`。
- 不得把 `skills check` 视为可靠的无副作用远程状态检查；第一版可显示“上次更新时间”，但**不显示未经独立验证的“有更新”标志**。
- 不得把 `skills add --list --json` 当作可用 API。
- 不得解析 CLI 的英文人类输出作为业务成功判据；`add --json` 解析 JSON + 退出码；`remove/update` 使用退出码 + 操作后重新读取目录和锁状态。
- 不得在 GUI 内手工写 Vercel v3/v1 锁文件，即便发现“锁记录残留”。可以显示诊断，不能擅自修复。

### 4.2 安装、更新、卸载的失败恢复

1. 写前刷新目标，显示实际 scope、现存文件夹、Agent 暴露情况以及覆盖提醒。
2. 用户明确确认后，由单一 CLI 执行器执行操作。
3. 捕获每次输出、退出码、JSON 安装分项结果，并检查操作后的文件实际状态；不把退出码为 0 自动等同于所有目标都成功。
4. 若发生部分成功，按每项结果展示并立即刷新全量快照，不做二次盲目写入。
5. 超时/手动取消需终止子进程（含必要子进程组处理），但 UI 要声明磁盘状态可能已经部分变化，随后重新扫描；不自动“回滚”文件系统。
6. 旧 Skill 的本地修改可能被 CLI 更新覆盖；没有可靠的本地变化判断时，统一显示覆盖警告，用户确认后才更新。

## 5. 页面和交互重构（保留视觉资产，不保留旧业务复杂性）

### 5.1 全局技能库（保留 `MySkills` 概念，重写数据绑定）

页面首屏显示安装总数、来源已知/未知数、全局/当前项目作用域、已探测 Agent 数。主列表包含名称、描述、来源、安装路径、相关 Agent、安装状态。支持名称/描述/来源搜索和作用域/Agent 筛选。卡片进入详情时显示 `SKILL.md`、打开文件夹、复制路径、安装来源和安全提示。提供“刷新”、“手动更新”、“卸载”。**移除 Preset 筛选、标签、拖拽排序、私有备份提示、旧数据库同步状态。**

### 5.2 Agent 工作区（简化原 `WorkspaceView`）

显示实际探测到的 Agent，点击展示 CLI 扫描到的可见 Skill。每个 Skill 标注：共享目录直读、私有目录软链接、复制或未知；当无法从当前输出断定链接类型时显示 `unknown`。允许通过 CLI 为 Agent 安装或解除部署，但对兼容共享目录的 Agent，提示“从某 Agent 移除链接并不代表它无法通过 `.agents/skills` 看见”。不要保留旧 Preset/全局 Tool Enable 开关的误导语义。

### 5.3 项目 Skill（简化 `ProjectDetail`）

通过系统文件夹选择器选择项目根目录。展示 `project/.agents/skills`、项目作用域 Agent 目录中真实存在的 Skill，项目 `skills-lock.json` 中对应的来源元数据；提供“安装到此项目”、“从此项目卸载”、“更新”。第一版只维护**当前打开的项目路径（进程内状态）**，不需要原项目注册表/排序/关联工作区；不再支持双向同步中央仓库。需要方便重开时可在后续仅把最近目录作为 UI 首选项，但不是业务数据库。

### 5.4 市场/安装（精简原 `InstallSkills`）

保留搜索、来源链接、关键词过滤和 Git/本地来源输入。市场搜索走 `skills.sh/api/search`，安装始终走 CLI。安装弹窗必须显式选择全局或项目、一个或多个 CLI 支持的 Agent、可选指定 `--skill`；若来源包含多个技能而无法结构化预览，明确说明将由 CLI 处理范围，不要假冒 GUI 已精准预览。默认不使用 `--all`，避免意外大范围部署。完成后自动刷新。

### 5.5 Settings / Diagnostics

展示 Node 路径和版本、CLI 路径和版本、全局 Skill 根目录、锁文件**实际解析后的路径**、当前项目目录、CLI 最近一次错误（仅内存）、测试连接/重新检测按钮。可支持用户通过安全文件选择器指定 CLI 可执行文件路径作为纯 UI 环境配置。不提供“中央仓库改路径”、Git backup 凭证、Preset、自动备份、旧迁移设置。

### 5.6 删除的页面/入口

`Backup` 页面、Preset 管理视图和工具栏、旧首次恢复向导、旧管理器 CLI 安装入口、依赖数据库的托盘 Preset 菜单全部删除。托盘如保留，仅提供打开窗口/退出等纯窗口操作；不启动自动更新 Skill 任务。删除旧 i18n key 和失效路由。

## 6. 代码改造清单（按职责处理，先替换依赖再删除）

### 6.1 后端建议直接删除的旧业务模块

以当前主分支审查结果为起点；文件确切引用需先用 `rg` 查清，凡有保留价值的**纯函数**可独立移到新模块，不得为了保留而维持旧 SkillStore：

- **存储/迁移：** `src-tauri/src/core/skill_store.rs`、`migrations.rs`、`central_repo.rs`、`sync_metadata.rs`、`scenario_service.rs`。
- **旧安装/状态：** `core/installer.rs`、`sync_engine.rs`、`skill_auto_updater.rs`、`auto_backup.rs`、`cli_bridge.rs`、`repo_lock.rs` 及其专用任务代码；`content_hash.rs`、`skill_metadata.rs`、`project_scanner.rs`、`scanner.rs` 等按新只读接口重写或删除。
- **旧备份/Git：** `core/git_backup.rs`、`git2_engine.rs`、`git_command.rs`、`git_credentials.rs`、`github_api.rs`、`merge/`、`crypto.rs` 及只服务于自有 Git 引擎的模块。注意个别代码若在新市场 HTTP 中使用，独立抽取后再删。
- **旧 CLI：** `src-tauri/src/bin/skills-manager-cli.rs`、关联的 `scripts/run-rust-cli.mjs`、`package.json` 中 `cli/cli:build/cli:install` 脚本，以及旧 CLI 发布打包逻辑。
- **旧命令层：** `src-tauri/src/commands/{skills,sync,scan,presets,git_backup,agent_workspace,projects,tools}.rs` 中依赖数据库/中央库的 IPC，采用新的少量 typed commands 替换；旧 `browse.rs`、`settings.rs` 仅提取仍必需的逻辑。
- **启动重写：** `src-tauri/src/core/app_state.rs`、`src-tauri/src/lib.rs` 中所有 `SkillStore` 初始化、备份、自动更新、旧 CLI bridge、Preset 托盘、数据库状态注入，以及旧命令注册。将 AppState 缩到运行时检测、CLI 执行队列、watcher、只读服务。

> 不建议第一步就删除全部文件导致项目无法编译。先实现新的后端 API，切换前端调用，最后删除无引用旧模块，并在每次提交后运行编译和测试。

### 6.2 前端文件

- **重写：** `src/lib/tauri.ts`（替换旧 DTO / invoke 合同）、`src/context/AppContext.tsx`（改成全局/项目列表查询及刷新）、`src/views/{Dashboard,MySkills,WorkspaceView,ProjectDetail,InstallSkills,Settings}.tsx`（必要时直接拆分新视图）。
- **删除或重写：** `src/views/Backup.tsx`、`src/components/PresetBar.tsx`、`CreatePresetDialog.tsx`、`RenamePresetDialog.tsx`、`BatchSyncAgentDialog.tsx`、`FirstRunRestoreDialog.tsx` 等只为旧 Preset/备份/同步存在的组件；依赖旧 `preset_ids` 的卡片逻辑也要移除。
- **保留并适配：** `Layout`、`ThemeContext`、`SkillMarkdown`、`AgentIcon`、通用 Dialog/Toaster、搜索与详情界面中的纯展示部分、i18n 基础结构。
- **清理：** `src/App.tsx` 旧路由、侧栏导航及失效的事件监听；旧接口/DTO 残留、旧翻译键；`localStorage` 中已不再使用的 Preset 键访问（无需迁移其值）。
- **不要保留空壳：** 被删除功能必须从 UI 完全移除，不显示假按钮、失效 Tooltip 或“稍后支持”的伪入口。

### 6.3 构建、依赖、发布

- `Cargo.toml` 移除 `rusqlite`、旧 `git2`/数据库/加密/旧 clap CLI 等确认无引用的依赖；按 `cargo check`、`cargo tree` 实际裁剪，不要机械删除新代码仍使用的 `serde/reqwest/notify` 等。
- 移除原应用内旧二进制桥接与旧发布工件；不要继续分发 `skills-manager-cli`。
- **如果这是一个独立 fork，务必移除上游 Tauri updater 的旧签名、公钥、更新 URL 或换成自己的签名/服务。** 原仓库 `src-tauri/tauri.conf.json` 指向 `xingkongliang/skills-manager` 的 Release 更新端点，未经调整不能发布改造版。
- 若发布为独立应用，更新产品名称、bundle identifier、README、截图、安装说明。禁止在新包里声称自带 Vercel CLI，除非真的打包了运行时。
- 先确保 macOS 环境可用，再验证 Windows 和 Linux 的编译及运行时探测；Windows `.cmd`/junction 等边缘行为必须有合同测试。

## 7. 安全边界与性能要求

**执行安全：** 严格 argv 数组调用、不启 shell；CLI 源地址和 Skill 标识要参数校验，不能允许恶意 `--` 选项注入；URL 不得带凭证直接存进 UI 日志。Tauri IPC 只暴露固定操作，不能开放“前端传 shell 命令”。非交互进程的环境变量继承最小化，不打印敏感变量。

**文件安全：** 所有文件阅读通过后端由合法列表项定位；防止目录穿越、循环链接、无效 UTF-8、超大 `SKILL.md`、符号链接逃逸后未经确认读取其他私有文件。对 CLI 管理的外部软链接允许只读显示合理的目标，但不开放任意路径访问。Markdown 作为不可信数据渲染，不启用原始 HTML/任意脚本执行。

**写入安全：** 只由 CLI 修改文件和原生锁；GUI 不用 `rm -rf` 去“修复状态”；对覆盖、删除、更新显示明确范围，操作完成读取真实磁盘后再报告最终结果。共享目录的 Agent 可见性无法逐 Agent 完全隔离，必须准确提示。

**性能：** 首屏先显示本地已有快照或直接加载状态，读文件/CLI 在 Tauri 异步任务里完成，绝不阻塞主线程；按需读取 `SKILL.md`；文件监听做 debounce（建议 300–800ms），只读刷新不触发 CLI 写操作。监听 CLI 锁文件、全局/项目 Skill 根目录及必要 Agent 路径；目录被删除重建后能恢复监听。禁用持续全盘轮询和重型 Git 同步。

## 8. 分阶段执行顺序（每阶段提交可编译）

### Phase A：冻结合同与新的后端骨架

**任务：** checkout 当前源码、运行旧项目测试建立基线；阅读 Vercel `skills` CLI 的 `cli.ts/list.ts/add.ts/remove.ts/update.ts/skill-lock.ts/local-lock.ts/installer.ts/agents.ts`，确定 CLI 版本。先写 `docs/ARCHITECTURE.md`、`docs/CLI_CONTRACT.md`，标明为什么 `check` 禁自动调用、`--list --json` 不支持。新建 Runtime Detector、CLI Runner、Scope 验证、只读 Lock Reader 与 Catalog，不调用旧 `SkillStore`。

**验收：** 能用隔离的 HOME 和临时项目路径调用 `skills --version`、`skills ls -g --json`、项目 `skills ls --json`；能读取 v3/v1 锁；CLI 缺失仍能显示只读状态；没有误写用户真实 HOME。

### Phase B：真正可写的 CLI 操作层

**任务：** 新增 `install/remove/update` typed IPC；源/作用域/Agent 参数校验、受控进程调用、取消/超时、结果解析、刷新机制；增加 skills.sh 只读市场搜索。首先使用临时 HOME/项目目录进行端到端验证。

**验收：** 全局和项目安装/卸载/更新成功；结果与 `skills ls --json`、实际目录、官方锁文件一致；GUI 不写锁；部分失败有准确报告；安装不会落到程序 cwd；Windows 不依赖拼接 `.cmd` shell 命令。

### Phase C：前端替换

**任务：** 将 AppContext、API DTO、全局列表、详情、Agent 视图、项目目录选择和市场安装全部改为新模型；接入运行时异常、只读降级、进度和确认 UI；删除 Preset/标签/Backup 的路由与入口。此阶段可暂时保留未引用的旧 Rust 文件，但新 GUI 不能再调用旧 IPC。

**验收：** 全局/项目切换准确，外部 `npx skills` 改动能在刷新后展示；运行时缺失不白屏；所有可见按钮真实可用且不会触发旧存储。

### Phase D：删旧系统并完成去依赖

**任务：** 删除旧 CLI、SQLite/迁移、私有中央库、Scenario/Preset、Tags、Git 备份/合并、专有更新器、旧命令注册、托盘 Preset 交互、旧 UI 组件和无引用依赖。重写 Tauri startup 和打包。删除旧单元测试，替换成适用于新职责的测试；保留通用安全测试。

**验收：** `cargo check`、`cargo test`、`npm run build`、`npm run test`、`npm run lint`、`npm run tauri:build` 通过；全仓库搜索不再存在实际生效的 `SkillStore`、`rusqlite`、`skills-manager.db`、`ensure_central_repo`、`scenario_service`、旧 Git backup/自有 CLI bridge 引用。新运行不会创建 `~/.skills-manager/skills` 或 `~/.skills-manager/skills-manager.db`。

### Phase E：平台检查、回归测试与文档

**任务：** 用 macOS 完成 UI 冒烟及 CLI 环境探测；用 Windows/Linux 检查构建、启动和路径差异。修订 README、CHANGELOG、docs/ARCHITECTURE.md、CLI 依赖安装说明；更新 UI 截图或移除过时图片。测试异常退出、源文件编辑冲突、锁与磁盘不同步、无网络、无法写目录。

**验收：** 全量验收清单通过，文档与 UI 没有旧功能宣称；能在全新环境只装 Node + Vercel CLI 后使用 GUI，卸载 GUI 不影响 Vercel 已安装 Skills 和它的锁文件。

## 9. 必须覆盖的测试矩阵

| 编号 | 场景 | 验收标准 |
|---|---|---|
| T01 | 空 HOME、无 CLI | GUI 正常打开，提示安装 CLI，扫描空目录，不创建自有 DB/中央库 |
| T02 | 全局现成 Skill 有 v3 锁 | 正确展示来源、路径、Agent；仅阅读不会改锁 |
| T03 | Skill 有文件无锁 | 显示为来源未知，SKILL.md 可看，不擅自补锁 |
| T04 | 锁有记录但文件不存在 | 不计入已安装；展示独立诊断 |
| T05 | 设置 `XDG_STATE_HOME` | 读取 XDG 锁而不是错误固定到 HOME |
| T06 | `skills ls` 输出非法 JSON / 非零退出 | 保留当前 UI 可用状态、报错，不清空磁盘和锁 |
| T07 | 全局多 Agent 安装 | CLI 实际创建共享文件/链接，UI 去重且状态准确 |
| T08 | 项目目录选择 + 安装 | 文件落在所选项目而不是 GUI cwd，v1 锁由 CLI 管理 |
| T09 | 全局卸载与限定 Agent 卸载 | GUI 显示正确范围；不会错误承诺共享可见性消失 |
| T10 | 更新和部分失败 | 明确确认后才运行；退出码 + 二次扫描决定结果 |
| T11 | 手工修改/其他 CLI 安装 | 不重启即可刷新显示；监听与手动刷新一致 |
| T12 | 软链接断裂、循环、目录重叠 | 不卡死、不错误删除真实内容 |
| T13 | 恶意来源字符串/伪 flag/路径穿越 | 不能注入 CLI 参数或读取任意文件 |
| T14 | 无网络 / CLI 超时 / 主动取消 | UI 不冻结，状态重新读取，不虚报成功 |
| T15 | 从 Finder 启动 macOS App | 能按配置找到真实 Node/CLI，不假设 shell PATH |
| T16 | Windows CLI shim/junction | 无命令注入，文件/链接状态准确 |
| T17 | 用户已有旧 `.skills-manager` 数据 | 新版完全忽略，**不扫描、不迁移、不删除** |
| T18 | 同时多个按钮快速点击 | 写操作串行，不因竞争创建两套锁文件/产生虚假状态 |
| T19 | CLI 版本不支持/锁文件新版本 | 清晰兼容性错误，禁用写操作；不重写锁 |
| T20 | 安装 / 卸载 GUI | CLI 管理的 `~/.agents/skills` 与 v3 锁完全保留 |

自动化测试全部使用临时 HOME、XDG_STATE_HOME、cwd，以及可控的 mock CLI / mock 文件系统；**绝对不能在开发者真实 `~/.agents` 上跑 destructive e2e 测试**。必要时配合真实隔离的 `skills@1.7.4` CLI 做少量端到端合同测试，捕获上游行为变化。

## 10. 最终交付物与“完成”定义

必须交付：

1. 新版源码及清晰提交记录，旧 DB/私有中央仓库/备份/自有 CLI 代码已实际删除，而不是保留弃用。
2. 可工作的 Tauri 桌面 GUI：全局列表、Skill 详情、Agent 工作区、项目 Skill、来源安装、显式卸载和手动更新、基本市场搜索、运行时诊断。
3. `docs/ARCHITECTURE.md`：唯一真相源、读写边界、无状态限制与共享可见性说明。
4. `docs/CLI_CONTRACT.md`：固定测试过的上游版本、支持的命令、参数、JSON 结构、限制和兼容性策略。
5. 新单元测试、CLI 合同测试、端到端测试和上文 T01–T20 的验证记录。
6. 更新后的 README 与构建/安装说明；清除原上游 updater 地址、旧功能文档和过时图片；如是 fork，确认 bundle ID 和签名配置。

**严格完成条件：** 新版运行全过程不生成自己的 Skill 数据库/锁文件/中央库；所有 Skill 写操作仅经 Vercel CLI；应用删除后 Skill 可继续被 `skills` CLI 使用；CLI 不可用时安全只读降级；没有旧数据迁移代码；所有可见功能都真实可用。

## 11. 建议分工与执行要求

- **Agent A（架构/后端）**：Phase A、B，制定 CLI 合同并完成 Rust Runtime/Catalog/IPC/进程安全边界。
- **Agent B（前端）**：基于合同执行 Phase C，重用 Tauri/React 视觉基础，删除 Preset/Backup/Tag UI；不要自行发明数据库字段。
- **Agent C（清理与构建）**：Phase D，在 A/B 接口稳定后删除旧 Rust/JS 模块、整理依赖、更新打包与 updater。
- **Agent D（测试与审查）**：为 A/B 提供隔离的合同测试/安全测试，执行 Phase E 和最终验收。

如果只有一个 coding agent，请严格按 A→B→C→D 的依赖次序串行执行。不得为了保留原 UI 假造旧 Preset/target 数据；功能冲突时优先满足无状态架构。每阶段提交可编译版本，完成后输出：删改文件清单、验证命令/结果、与上游 CLI 版本相关的已知限制、剩余风险。**不要提前要求用户确认迁移策略：本任务明确不做迁移。**

## 12. 补充：自管软链共享（2026-10-25，对第 7 节"写入安全"的修订）

已安装技能的**共享给其他 Agent**不走 `skills add`，由 GUI 自行在目标
Agent 技能目录创建**相对目录软链**，指向该技能现有文件夹
（`core/symlink_share.rs`，`share_skill` 命令）。理由：`add` 对已装技能
会重建 canonical、单目标目录强制复制、且发现目标为实体目录时先递归
删除再建链——共享场景三者都是错的。

安全护栏（继承自 phase D 删除前的旧 sync_engine，见模块头注释）：

- 目标分类只用 `symlink_metadata`；分类后按类型删除，杜绝 TOCTOU。
- `NoClobber` 默认策略：目标处的实体目录/外链/文件一律拒绝，仅当
  GUI 弹窗确认（`replaceCopies`）才允许替换；源目录本身所在 Agent
  永远跳过，绝不自删。
- 链接一律相对（基于目录物理路径计算），与 CLI 自身布局一致；Windows
  上 symlink 失败回退 junction（绝对目标）。
- 共享目录型 Agent（目录 == `~/.agents/skills` / `<root>/.agents/skills`）
  跳过并说明原因，避免一条链接把技能泄漏给全部 universal Agent。
- 该操作不进 CLI 锁文件：`skills update` 不会动这些链接；卸载走
  `skills remove`（按目录扫描）仍可清理；GUI 目录按 symlink exposure
  展示。降级模式（CLI 不可用）下共享依然可用。
