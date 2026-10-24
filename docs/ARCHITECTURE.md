# Architecture

> Skills Manager 2.x — Tauri 桌面 GUI，Vercel `skills` CLI 是唯一的 Skill 写入执行引擎。

## 1. 核心原则

1. **无自有状态**：本应用不创建任何 Skill 数据库、私有中央仓库、业务锁文件或 manifest。
   卸载本应用后，所有已安装 Skills 继续由 `skills` CLI 与各 Agent 直接使用。
2. **唯一写入引擎**：安装、卸载、更新一律调用 Vercel `skills` CLI 完成。GUI 不复制、
   不重写 CLI 的下载、Git 更新、包解析、锁文件维护、符号链接管理逻辑。
3. **事实源分层**：
   - 文件系统（`~/.agents/skills`、`<project>/.agents/skills` 及各 Agent 原生目录）是
     "Skill 是否存在"的事实源；
   - Vercel CLI 锁文件（全局 v3 / 项目 v1）是来源与版本元数据的只读补充；
   - `skills ls --json` 是 Agent 可见性的参考输入。
4. **只读降级**：CLI 不可用或版本不兼容时，GUI 仍可浏览本地文件（降级只读扫描），
   禁用全部写操作并给出修复指引。
5. **不做旧数据迁移**：不读取、不扫描、不删除旧版 `~/.skills-manager` 数据。旧文件
   保留在磁盘上由用户自行处理。

## 2. 数据归属（全部由 Vercel CLI 拥有）

```text
~/.agents/
├── skills/<name>/SKILL.md          # 全局 Skill 本体（CLI 写入）
└── .skill-lock.json                # 全局锁 v3（无 XDG_STATE_HOME 时）

$XDG_STATE_HOME/skills/.skill-lock.json   # 设置了 XDG_STATE_HOME 时的全局锁

~/.claude/skills/<name> -> ~/.agents/skills/<name>   # Agent 链接（CLI 创建）

<project>/
├── .agents/skills/<name>/SKILL.md  # 项目 Skill 本体（CLI 写入）
└── skills-lock.json                # 项目锁 v1（CLI 写入）
```

应用自身只持有：进程内临时状态（运行时探测缓存、CLI 执行队列、最近错误、当前打开
的项目路径）与纯 UI 偏好（主题/语言，localStorage）。锁文件 schema 是外部实现细节，
本应用**只读解析、从不写**；解析失败或版本高于已知版本时降级为"元数据不可用"，
绝不把解析失败当成空仓库，更不重写锁文件。

## 3. 分层结构

```text
React Views（Library / Agents / Project / Marketplace / Settings）
        │ typed DTO
Frontend API 层（src/lib/api.ts，invoke 封装 + loading/error 状态）
        │ Tauri IPC（固定操作名，不接受任意 argv）
Rust 应用服务
  ├── runtime     # node / skills CLI 路径与版本探测、健康检查
  ├── cli         # 统一 CLI 执行器：argv 数组、env 白名单、超时/取消、输出上限、串行写队列
  ├── scope       # 全局/项目作用域与路径边界
  ├── lock        # 只读解析 v3/v1 锁
  ├── catalog     # 聚合：磁盘扫描 + CLI ls + 锁 → SkillView
  ├── skill_md    # 安全读取 SKILL.md（体积上限、路径校验）
  ├── marketplace # skills.sh 搜索（只读 HTTP）
  ├── watcher     # 文件变更 debounce → 重新扫描事件
  └── commands    # 精简的 Tauri IPC 命令
        │
        ├── Vercel skills CLI（唯一写入执行引擎）
        ├── ~/.agents/...（只读扫描）
        └── <project>/...（只读扫描）
```

## 4. 关键行为约定

- **CLI 调用纪律**：全部以 argv 数组调用，不经过 shell；`stdin=null`；写操作显式
  `-y`；全局操作 cwd 固定为安全目录，项目操作 cwd 为用户选择的项目根；结果以
  退出码 + JSON 分项 + 操作后重扫描三重判定，退出码 0 不等于全部成功。
- **写操作串行化**：所有 CLI 写操作经单一队列串行执行；结束后强制重新扫描。外部
  终端可能并发修改，因此不声称跨进程独占，只保证自身不竞争。
- **禁自动后台调用**：`skills check`/`update`/`add`/`experimental_*` 永远只在用户
  显式点击后运行（`check` 在上游实现中与 `update` 派发到同一入口，禁止当作只读探测）。
- **Agent 标识**：CLI `ls --json` 返回的 `agents` 是展示名。所有传给 `-a` 的 key
  来自应用内置的受控 Agent 映射表（快照自 skills@1.7.4），未知展示名只展示不可操作。
- **可见性口径**：兼容共享目录（`.agents/skills`）的 Agent 即使被"从某 Agent 移除
  链接"，仍可能通过共享目录看到该 Skill。UI 措辞不承诺彻底隐藏。
- **删除安全**：GUI 永不 `rm -rf` 用户文件；目录/锁状态异常只显示诊断。

## 5. 兼容性与版本策略

- 合同测试锚定 `skills@1.7.4`（全局锁 v3、项目锁 v1）。升级 CLI 版本必须重跑
  `npm run test`（含 CLI 合同测试）后再发布。
- CLI 缺失 / 版本无法识别 → 只读降级。
- 锁文件版本 > 已知版本 → 元数据不可用，本体仍按文件系统展示。
- 详见 [CLI_CONTRACT.md](./CLI_CONTRACT.md)。
