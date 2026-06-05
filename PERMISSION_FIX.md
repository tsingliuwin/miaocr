# 屏幕录制权限问题修复总结

## 🔍 问题根源

Gemini 无法解决的核心问题：

```rust
// ❌ 问题代码：函数定义了但从未被调用
#[cfg(target_os = "macos")]
#[allow(dead_code)]  // <- 标记为"死代码"
fn request_macos_screen_capture_permission() {
    // ... 请求权限的代码
}
```

**关键问题：**
1. 权限请求函数从未被调用
2. 只检查权限（`CGPreflightScreenCaptureAccess`）不请求权限（`CGRequestScreenCaptureAccess`）
3. 截图时直接失败，系统不会弹出权限对话框

---

## ✅ 修复方案（主流实践）

### 1️⃣ 新增综合权限管理函数

```rust
#[cfg(target_os = "macos")]
fn ensure_screen_capture_permission() -> bool {
    let (has_perm, supported) = check_macos_screen_capture_permission();
    
    if !supported {
        return true;  // 旧系统，假设有权限
    }
    
    if has_perm {
        return true;  // 已有权限
    }
    
    // ✅ 关键：主动请求权限，触发系统弹窗
    runtime_log("[PERM] Requesting screen capture permission...");
    request_macos_screen_capture_permission();
    
    // 再次检查权限状态
    let (has_perm_after, _) = check_macos_screen_capture_permission();
    has_perm_after
}
```

### 2️⃣ 应用启动时主动请求

```rust
// 在 main() 函数中初始化 FloatApp 时
mac_permission_error: {
    #[cfg(target_os = "macos")]
    {
        runtime_log("[PERM] Checking on startup...");
        !ensure_screen_capture_permission()  // ✅ 启动时就请求
    }
    #[cfg(not(target_os = "macos"))]
    { false }
}
```

**效果：** 用户首次运行应用时立即看到系统权限对话框 ✨

### 3️⃣ 截图前二次确认

```rust
if self.select_step == 1 {
    #[cfg(target_os = "macos")]
    {
        // ✅ 截图前再次确认权限
        if !ensure_screen_capture_permission() {
            self.mac_permission_error = true;
            self.select_step = 0;
            return;  // 提前中止，避免截图失败
        }
    }
    
    // 继续截图流程...
}
```

### 4️⃣ 增强用户界面

```rust
// 新增"重新请求权限"按钮
if ui.button("🔄 重新请求权限").clicked() {
    if ensure_screen_capture_permission() {
        self.mac_permission_error = false;  // ✅ 成功后自动关闭提示
    }
}
```

**效果：** 用户在系统设置中授权后，无需重启应用即可重试 🎯

---

## 📊 与主流应用对比

| 应用 | 启动时请求 | 使用前检查 | 手动重试 | 详细指引 |
|------|-----------|-----------|---------|---------|
| **OBS Studio** | ✅ | ✅ | ✅ | ✅ |
| **Raycast** | ✅ | ✅ | ✅ | ✅ |
| **CleanShot X** | ✅ | ✅ | ✅ | ✅ |
| **修复前的 miaocr** | ❌ | ❌ | ❌ | ✅ |
| **修复后的 miaocr** | ✅ | ✅ | ✅ | ✅ |

---

## 🚀 快速测试

### 方法 1: 使用测试脚本（推荐）

```bash
./scripts/test_permission.sh
```

选择「完整测试」选项，脚本会自动：
1. ✅ 重新构建应用
2. ✅ 重置权限（模拟首次运行）
3. ✅ 启动应用
4. ✅ 显示测试检查清单

### 方法 2: 手动测试

```bash
# 1. 重置权限
tccutil reset ScreenCapture com.tsingliu.miaocr

# 2. 构建并打包
cargo build --release
./scripts/package.sh

# 3. 运行应用
open target/release/bundle/osx/miaocr.app

# 4. 查看权限日志
tail -f .log/runtime_*.log | grep PERM
```

---

## 📝 预期行为

### ✅ 首次运行
1. 应用启动
2. **立即弹出系统权限对话框** ← 这是关键！
3. 用户点击「允许」→ 正常使用
4. 用户点击「不允许」→ 显示权限提示界面

### ✅ 选择区域
1. 点击「选择区域」按钮
2. 如果没有权限 → 立即显示提示（不尝试截图）
3. 如果有权限 → 正常进入选择模式

### ✅ 权限恢复
1. 在系统设置中撤销权限
2. 回到应用尝试功能 → 显示权限提示
3. 在系统设置中重新授权
4. 点击「重新请求权限」→ **无需重启应用**

---

## 🔧 技术细节

### macOS 屏幕录制权限 API

| API | 功能 | 触发弹窗 |
|-----|------|---------|
| `CGPreflightScreenCaptureAccess()` | 检查权限状态 | ❌ 不触发 |
| `CGRequestScreenCaptureAccess()` | 请求权限 | ✅ 触发 |

**Gemini 的错误：** 只用了 `CGPreflightScreenCaptureAccess`，从未调用 `CGRequestScreenCaptureAccess`

### 调试日志

修复后的应用会输出详细日志：

```
[PERM] Checking and requesting screen capture permission on startup...
[PERM] Screen capture permission granted successfully
[PERM] Failed to obtain screen capture permission before screenshot
[PERM] User manually requesting screen capture permission...
```

查看日志：
```bash
cat .log/runtime_*.log | grep PERM
```

---

## ❓ 常见问题

### Q: 为什么要在启动时就请求权限？

**A:** 这是 Apple 官方推荐的最佳实践：
- ✅ 首次运行时就展示，说明为何需要权限
- ✅ 用户体验更好（vs 使用功能时才弹窗）
- ✅ 遵循 HIG (Human Interface Guidelines)

### Q: 为什么需要在截图前再次检查？

**A:** 防御性编程：
- 用户可能在应用运行时撤销权限
- 系统可能在后台更新权限状态
- 提前检查避免截图失败的复杂错误处理

### Q: `CGRequestScreenCaptureAccess` 返回 true 就有权限了吗？

**A:** ❌ 不一定！
- 返回 true 只表示「显示了对话框」
- 用户可能点击了「不允许」
- 必须用 `CGPreflightScreenCaptureAccess` 再次检查

### Q: 修改权限后必须重启应用吗？

**A:** 通常不需要（本修复已支持动态重新请求）
- 但某些 macOS 版本可能缓存权限状态
- 点击「重新请求权限」通常就能生效
- 如果仍不行，重启应用即可

---

## 📚 参考资料

- [Apple 文档：Requesting Authorization to Capture Screens](https://developer.apple.com/documentation/avfoundation/capture_setup/requesting_authorization_to_capture_and_save_media)
- [CGRequestScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgrequestscreencaptureaccess)
- [TN3110: Resolving Common Screen Recording Issues](https://developer.apple.com/documentation/technotes/tn3110-resolving-common-screen-recording-issues)

---

## 🎯 总结

### 修复前（Gemini 方案）
```
应用启动 → 仅检查权限 → 不弹窗
用户选择区域 → 尝试截图 → 失败 → 显示提示
```

### 修复后（主流方案）
```
应用启动 → 主动请求权限 → ✅ 弹窗
用户选择区域 → 先检查权限 → 再截图 → ✅ 成功
```

**核心区别：** 真正调用了 `CGRequestScreenCaptureAccess` API！

---

## 💡 关键代码变更

1. ✅ 移除 `#[allow(dead_code)]`，让权限请求函数真正被使用
2. ✅ 新增 `ensure_screen_capture_permission()` 统一管理权限
3. ✅ 启动时调用权限请求（不是仅仅检查）
4. ✅ 截图前二次确认（防御性编程）
5. ✅ UI 增加「重新请求权限」按钮（用户友好）
6. ✅ 增加详细日志（便于调试）

---

**修复完成！现在可以像 OBS、Raycast 等主流应用一样正确处理 macOS 屏幕录制权限了。** 🎉
