# Test Matrix Verification (T01–T20)

每项给出验收结论与证据（自动化测试 / 代码路径 / 待人工确认）。所有自动化
测试运行于隔离的 HOME / XDG_STATE_HOME / 临时项目目录；真实 `~/.agents`
与 `~/.skills-manager` 绝不被写。

| # | 场景 | 结论 | 证据 |
|---|---|---|---|
| T01 | 空 HOME、无 CLI | 通过 | `contract_tests::empty_home_lists_nothing_and_writes_nothing`（真实 CLI，隔离 HOME 返回 `[]`，不产生自有文件）；无 CLI 时 `runtime_detector` 返回 notes 且 `degraded=true`，`list_skills` 走纯磁盘路径（`build_catalog_from_disk` 空目录 → 空列表） |
| T02 | 全局 Skill 有 v3 锁 | 通过 | `contract_tests::global_install_remove_and_lock_lifecycle`；`skill_catalog` 的 `skill_with_lock_is_installed_and_tracked`（只读锁、不回写） |
| T03 | Skill 有文件无锁 | 通过 | `skill_catalog::skill_without_lock_is_untracked`（status=untracked，不补锁） |
| T04 | 锁有记录但文件不存在 | 通过 | `skill_catalog::lock_record_without_files_is_diagnostic_only`（不计入已安装，诊断提示，不改锁） |
| T05 | XDG_STATE_HOME | 通过 | `contract_tests::xdg_state_home_redirects_the_global_lock` + `skill_catalog::xdg_state_home_lock_location_is_used` + `scope::global_lock_prefers_xdg_state_home` |
| T06 | `skills ls` 非法 JSON/非零退出 | 通过（代码路径） | `list_skills` 捕获失败 → 记录 `last_cli_error` → 返回磁盘版目录 + 诊断（不清空、不写盘）；`parse_json_stdout` 对空/坏 JSON 返回错误。UI 不冻结（async command）。无独立单测（需 mock State），由代码审查覆盖 |
| T07 | 全局多 Agent 安装 | 通过 | `contract_tests::global_install_remove_and_lock_lifecycle`（claude-code + windsurf → canonical + 双符号链接，锁 v3）；catalog 按 name 去重 |
| T08 | 项目目录选择 + 安装 | 通过 | `contract_tests::project_install_uses_cwd_and_writes_v1_lock`（文件落在所选项目、不落 GUI cwd；锁 v1）；`ScopeArg::into_scope` 校验绝对路径/存在/目录/非旧库 |
| T09 | 全局卸载与限定 Agent 卸载 | 通过 | 全量卸载同 T07 第二段；限定 `-a` 卸载：`remove_skill` 对 agents 分支以退出码判定 + 提示共享目录仍可见（消息文案）；CLI 端行为见 remove.ts #287（canonical 保留给仍使用的 agent）。UI 措辞不承诺彻底隐藏 |
| T10 | 更新和部分失败 | 通过 | `contract_tests::update_matches_lock_key_and_stays_installable`；`add --json` 分项结果（installed/failed/skipped）驱动 `OperationResult.success`，退出码 0 ≠ 成功；update 无 JSON → 退出码 + 重扫描 |
| T11 | 手工修改/其他 CLI 安装 | 通过 | `change_watcher` 3s 重扫 + 500ms 防抖发 `skills-changed` → 前端 `refreshAll`；`sync_watch_set_adds_and_removes_watches` 覆盖目录增删；写操作完成后也主动 emit |
| T12 | 软链接断裂、循环、目录重叠 | 通过（断裂） | `skill_catalog::broken_symlink_is_diagnosed_not_listed`（unix）；循环：scan 只 readdir 一层 + SKILL.md 检查，canonicalize 失败即 broken，无递归跟随（不卡死）；重叠：name 去重（copy/native 用例）。不删除任何真实内容（无 rm 路径） |
| T13 | 恶意来源/伪 flag/路径穿越 | 通过 | `validate_argv_value`（拒绝前导 `-`、控制字符、超长）+ `validate_agent_keys`（仅受控表 key，拒绝 displayName 与 `*`）；`vercel.rs` 测试 `argv_values_reject_flags_and_junk`/`agent_keys_validate_against_the_map`；文档读取 `skill_document::rejects_paths_outside_every_root`（scan-root 边界 + 512KB 上限）；无 shell 拼接（`skills_cli` argv 数组） |
| T14 | 无网络 / 超时 / 取消 | 通过 | 超时：`skills_cli::timeout_kills_the_process`（进程组击杀）；取消：`cancel_flag_kills_the_process` + `cancel_operation` IPC；UI 不冻结（async），消息声明磁盘可能部分变化后重扫描；无网络：add 失败 → 非零退出 + stderr 呈现 |
| T15 | Finder 启动探测 PATH | 通过（代码路径） | `runtime_detector::node_candidates/cli_bin_candidates`（PATH → nvm/volta/fnm/homebrew/npm 全局），入口解析为绝对路径 `node <bin/cli.mjs>`；单测覆盖 fake npm 布局解析。真实 Finder 启动需人工确认 |
| T16 | Windows shim/junction | 通过（代码路径） | `.cmd` shim 反解（`resolves_windows_cmd_shim_layout`）、`taskkill /T /F` 树杀、CREATE_NO_WINDOW、argv 数组无注入面。真实 Windows 运行时需 CI/人工验证 |
| T17 | 已有旧 `.skills-manager` 数据 | 通过 | `scope::detects_legacy_repo_paths` + `validate_project_root` 拒绝 `InsideLegacySkillsManager`；全代码无对 `~/.skills-manager` 的读写 |
| T18 | 快速连点多个写按钮 | 通过 | 所有写命令持有 `state.write_queue`（tokio Mutex）串行执行；前端 InstallDialog running 态禁用按钮 |
| T19 | CLI 版本不支持 / 锁新版本 | 通过 | 版本：< 1.7.0 → `writes_supported=false`，写命令拒绝并给出升级指引（`fake_cli_entry_reports_version_and_gating`）；锁：`skill_catalog::unsupported_lock_version_marks_metadata_unsupported`（显示元数据不可用，不重写） |
| T20 | 卸载 GUI 后 Skills 保留 | 通过（结构保证） | 本应用不创建任何自有数据库/锁/仓库；技能与锁完全由 CLI 拥有（合同测试全部文件均由 CLI 产生）；`empty_home` 测试证明 GUI 自身不落任何业务文件 |

## 待人工 / CI 环境确认

- T15：从 Finder/Dock 实际启动（本机 PATH 探测逻辑已有单测）。
- T16：Windows 实机构建与运行（test.yml 覆盖 cargo test，release.yml 覆盖
  构建；junction 行为依赖 NTFS 环境）。
- UI 冒烟：窗口渲染、路由切换、对话框交互（vitest 覆盖组件级，整窗冒烟
  需运行 `npm run tauri:dev`）。
