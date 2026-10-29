# 发布流程

版本采用 SemVer。0.x 表示开发中；1.0.0 是未来稳定版目标，不等同于当前已发布版本。配置结构版本单独管理。

## 本机检查

1. 同步 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 和 `src/types.ts` 的应用版本。
2. 更新 CHANGELOG，运行 CONTRIBUTING 中的检查。确认 README 展示图来自网页“体验演示”，不含账号资料或私人壁纸。
3. 运行 `npm run desktop:package` 生成当前用户 NSIS 安装包与便携 EXE。
4. 运行 `npm run release:source` 生成按白名单收集的公开源码 ZIP 和文件哈希清单。此命令也运行发布检查，若发现疑似敏感内容则停止，只报告文件名和规则。
5. 从 ZIP 解压到新目录后，运行 `npm ci`、构建与测试，确认源码不依赖本机未导出的文件。检查结果写入本地交接记录。

源码导出不包含本机 HANDOFF、AGENTS、设计过程文件、`.qa`、账号状态、环境文件、旧安装包、编译缓存或第三方依赖目录。发布检查是一层辅助保护，上传前仍需人工检查；它不能保证发现所有秘密。

## Windows 手工回归

- 安装、覆盖升级、卸载；配置与凭据是否沿用，开机启动注册与卸载清理是否正确。
- 完整面板/桌面切换、拖动、托盘恢复、不占任务栏、字体与材质切换。
- 单平台失败、旧值标记、重试恢复、限流等待，另三个平台继续显示。
- 关闭 Codex 聊天窗口后独立运行；Tools 停止时正确显示过期数据。
- 多屏/DPI、Explorer 重启、睡眠恢复、静态/动态壁纸的实际兼容范围。未测试项应保留在发布说明。

## GitHub

首次上传建议使用导出的干净源码目录建立仓库，避免误带本机历史。保留 LICENSE 和第三方授权。启用 Issue 模板、分支保护与私密漏洞报告。

CI 在 Windows 上执行前端、Rust 与界面测试；手动运行 `Package` 工作流只生成可下载构建产物，不自动发布。CI 实际运行结果以 GitHub 页面为准，本地测试不能代替远端验证。

准备好仓库后创建与应用版本匹配的 `v0.x.y` 标签和 Release，上传安装包、便携版、源码 ZIP、SHA256 清单及已知限制；当前 0.x 建议标为预发布。没有代码签名证书时明确说明安装包未签名，Windows 可能提示未知发布者。不要承诺绕过系统安全提示。

## 0.8.0 隐私验收

使用 `npm run desktop:package`，该入口传入编译路径重映射，不要用未映射路径的手工 cargo build 结果作为公开附件。构建后运行 `node scripts/privacy-audit.mjs --history --file src-tauri/target/release/token-plusplus.exe --archive 安装包路径 --archive 源码包路径`；安装包检查使用 7-Zip 实际解包后扫描其 EXE。校验输出不打印凭据内容。真实 Agent2API 部署的访问令牌只在软件中输入，测试报告不保存响应原文。新版验收前保留旧附件；验收后撤下 v0.7.1 两个 EXE、更新其说明及校验清单，保留源码及许可。
