# 参与 Token++

欢迎完善 Windows 桌面体验、修复额度解析问题和贡献平台适配器。提交前先查阅 [平台支持](docs/PROVIDERS.md)，讨论新增平台的数据来源与查询方式。

## 本地开发

需要 Windows 10/11 x64、Node.js 24、Rust stable（MSVC）、Visual Studio C++ Build Tools 与 WebView2 Runtime。锁文件随代码提交。

```powershell
npm ci
npm run desktop
```

`npm run dev` 提供不连接真实账户的网页预览，点击“体验演示”使用示例数据。

## 提交检查

```powershell
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
npm run test:ui
npm run release:check
```

界面测试使用本机 Microsoft Edge。涉及 Windows 桌面挂载、托盘、启动项的修改还需在桌面应用中验证，写明实际测试范围。`scripts/check-widget.mjs` 等本机集成脚本会访问本机配置、登录和启动项，只在了解其行为的开发机上运行，不用于无账号 CI。

## 数据与行为约定

- 查询失败、缺失字段、未接入必须保留未知状态；失败可展示带过期标记的最近成功数据。
- 金额在 Rust 中使用十进制；不同币种、代金券和订阅额度不混加。
- 不把已到重置时间当成额度已恢复；下一次查询成功后再确认。
- 凭据只放系统安全存储；不要提交真实响应、账号截图、登录令牌或完整本机配置。
- HTTP 错误给出可理解的脱敏信息；独立重试必须遵循限流等待。
- 新平台按 [适配指南](docs/ADDING_PROVIDER.md) 实现，并补充有意义的解析与边界测试。

PR 说明问题、最终行为、验证和局限。项目使用 SemVer；配置结构版本独立维护，改变结构时提供迁移。

## 许可证

提交代码意味着同意按本项目 [MIT 许可证](LICENSE) 分发贡献。第三方资源保留其原授权与来源，见 [第三方声明](THIRD_PARTY_NOTICES.md)。
