# Bug: 鼠标光标在应用上不显示

## 发现时间
2026-06-05

## 现象
鼠标移动到应用窗口上时，光标不显示。用户无法看到鼠标指针位置，影响交互体验。

## 根因
`src/index.css` 中对 `html, body, #root` 设置了：
```css
cursor: default !important;
pointer-events: auto;
```

问题分析：
1. `!important` 强制覆盖所有子元素的光标样式，导致交互元素（按钮、链接、输入框等）无法显示正确的光标类型（如 `cursor: pointer`、`cursor: text` 等）
2. 在 Tauri v2 的无装饰窗口（`decorations: false`）环境下，`pointer-events: auto` 显式设置在 `html` 元素上可能与 WebKit/GTK 的光标渲染机制冲突，导致光标完全不显示
3. `html, body, #root` 同时设置这些属性会产生叠加效应，干扰系统级光标管理

## 解决方案
修改 `src/index.css`，将光标样式简化并移除有问题的属性：

**修复前：**
```css
html,
body,
#root {
  cursor: default !important;
  pointer-events: auto;
}
```

**修复后：**
```css
body {
  cursor: default;
}
```

改动说明：
- 移除 `!important`，避免强制覆盖交互元素的光标样式
- 移除 `pointer-events: auto`，避免与 WebKit 光标渲染冲突
- 只在 `body` 上设置 `cursor: default`，`html` 和 `#root` 继承即可
- 保留 body 的默认光标，同时允许子元素（如按钮的 `cursor: pointer`、输入框的 `cursor: text`）正常显示

## 影响范围
- 整个应用窗口的光标显示
- 所有交互组件的光标反馈（按钮、链接、输入框、拖拽区域等）

## 修复文件
- `src/index.css`

## 验证方法
1. 启动 Tauri 桌面应用
2. 移动鼠标到应用窗口上，确认光标可见
3. 悬停在按钮、链接、输入框等交互元素上，确认光标类型正确变化（如 pointer、text、col-resize 等）
