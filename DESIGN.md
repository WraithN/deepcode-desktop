# DeepHarness Desktop — 设计规范

> 本文件为项目 UI/UX 设计的单一事实来源（Single Source of Truth）。**任何涉及界面设计、样式调整、新增组件的变更，必须先阅读并遵循本文档。**

---

## 1. 设计哲学

### 1.1 核心定位

**VS Code 风格的开发者工具界面** — 深色、高密度、信息优先、低干扰。目标是为开发者提供一个沉浸式的 AI 编码辅助环境。

### 1.2 设计原则

| 原则 | 说明 |
|------|------|
| **深色优先** | 默认深色主题，所有组件在深色背景下设计 |
| **高密度信息** | 小字号、紧凑间距、充分利用屏幕空间 |
| **低视觉噪音** | 无圆角卡片阴影、无渐变装饰、无边框过重 |
| **工具感** | 像专业 IDE 一样精确、可靠、响应迅速 |
| **一致性** | 同一类元素在全局使用完全相同的样式模式 |

---

## 2. 色彩系统

### 2.1 CSS 变量（HSL）

所有颜色通过 CSS 自定义属性定义，使用 HSL 格式：

```css
:root {
  --radius: 0.125rem;
  --background: 222 47% 11%;       /* 深蓝灰背景 */
  --foreground: 210 40% 96%;        /* 近白文字 */
  --card: 222 47% 11%;              /* 卡片背景 */
  --primary: 213 94% 68%;           /* 主色：蓝色 */
  --primary-foreground: 222 47% 11%;
  --secondary: 220 13% 18%;         /* 次级背景 */
  --muted: 220 13% 18%;             /* 静音区背景 */
  --muted-foreground: 215 20% 65%;  /* 次要文字 */
  --border: 220 13% 18%;            /* 边框 */
  --destructive: 0 63% 31%;         /* 错误/危险 */
  --success: 142 71% 45%;           /* 成功 */
  --warning: 38 92% 50%;            /* 警告 */
}
```

### 2.2 主题色切换

支持 5 种主题色，通过 JavaScript 动态修改 `--primary`：

| 主题 | HSL 值 | 适用场景 |
|------|--------|----------|
| Blue（默认） | `213 94% 68%` | 通用默认 |
| Green | `142 71% 45%` | 成功/安全相关 |
| Orange | `25 95% 53%` | 警告/高亮 |
| Purple | `270 60% 55%` | 创意/特殊功能 |
| Pink | `340 75% 55%` | 强调/活跃状态 |

主题色存储在 `localStorage.theme_color`，由 `App.tsx` 的 `useEffect` 应用。

### 2.3 语义化颜色使用

- **主操作**：`bg-primary text-primary-foreground`
- **次级操作/背景**：`bg-secondary text-secondary-foreground`
- **边框/分割线**：`border-border`
- **次要文字**：`text-muted-foreground`
- **错误/危险**：`text-destructive` / `bg-destructive`
- **成功**：`text-green-400` / `bg-green-400/5`
- **警告**：`text-yellow-400` / `bg-yellow-400/5`

---

## 3. 字体系统

### 3.1 字体栈

```css
font-family: 'JetBrains Mono', 'Noto Sans SC', 'Noto Sans JP', 'Noto Sans KR', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
```

- **JetBrains Mono**：主要字体，营造开发者工具感
- **Noto Sans SC/JP/KR**：CJK 字符子集化加载（通过 `unicode-range`）
- 回退到系统字体栈

### 3.2 字号层级

| 级别 | Tailwind 类 | 用途 |
|------|-------------|------|
| 页面标题 | `text-[18px] font-semibold` | 页面级标题 |
| 卡片标题 | `text-[16px] font-medium` | 卡片/区块标题 |
| 正文 | `text-sm` | 大多数 UI 文字、描述、标签 |
| 辅助文字 | `text-xs` | 元数据、状态提示、时间戳 |
| 微文字 | `text-[10px]` | 优先级标签、极小标注 |

### 3.3 代码/等宽文字

```css
code, pre, .font-mono {
  font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Consolas', 'Monaco', monospace;
}
```

---

## 4. 间距与布局

### 4.1 间距约定

- **页面内边距**：`p-8`（大留白）或 `px-4 py-6`（紧凑）
- **卡片内边距**：`p-6`
- **表单组间距**：`space-y-4`
- **标签/输入对**：`space-y-2`
- **Flex 间距**：`gap-2`、`gap-4`
- **列表项间距**：`space-y-1.5`

### 4.2 布局模式

- **页面级**：`min-h-screen flex flex-col bg-background`
- **居中内容**：`flex flex-1 items-center justify-center` + `max-w-md`
- **三栏工作区**：左侧导航栏（固定宽度）+ 中间内容区（flex-1）+ 右侧边栏（固定宽度）
- **卡片网格**：`grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4`

### 4.3 容器

不使用全局容器包装。每个页面/区块自行控制 `max-w-*`：
- 登录表单：`max-w-md`
- 选择页面：`max-w-2xl`
- 工作区：全宽，内部按需限制

---

## 5. 圆角与边框

### 5.1 圆角系统

| Token | 值 | 用途 |
|-------|-----|------|
| `--radius` | `0.125rem`（2px） | 全局基准 |
| `rounded-sm` | ~0px | 极小元素 |
| `rounded-md` | `calc(var(--radius) - 2px)` | 按钮、输入框 |
| `rounded-lg` | `0.5rem` | 卡片、弹窗 |
| `rounded-xl` | 较大 | 大卡片、模态框 |

**原则**：整体风格偏锐利，圆角极小，突出工具感。

### 5.2 边框约定

- **默认边框**：`border` 或 `border-border`
- **边框颜色极淡**：在深色模式下几乎不可见（`220 13% 18%`）
- **单边边框工具类**：`.border-t-solid`、`.border-r-dashed` 等（自定义 Tailwind 工具）
- **分割线**：`border-b border-border`

---

## 6. 阴影与层次

### 6.1 阴影使用

| 类名 | 用途 |
|------|------|
| `shadow-sm` | 输入框、次级按钮、选中标签 |
| `shadow` | 卡片、默认按钮、下拉菜单 |

**禁止**：无大阴影（`shadow-lg`、`shadow-xl`）、无彩色阴影、无发光效果。

### 6.2 层次管理

通过背景和边框区分层次，而非阴影：
- **背景层次**：`bg-background` → `bg-card` → `bg-secondary`
- **边框分隔**：相邻区域使用 `border-border` 分隔

---

## 7. 组件规范

### 7.1 shadcn/ui 组件

本项目使用 **shadcn/ui New York 风格**。所有 UI 组件位于 `src/components/ui/`，由 `components.json` 管理。

**核心约定**：
- 使用 `cn()` 工具函数合并类名（来自 `clsx` + `tailwind-merge`）
- CVA（class-variance-authority）管理组件变体
- SVG 图标统一尺寸：`[&_svg]:size-4`
- 焦点状态：`focus-visible:ring-1 focus-visible:ring-ring`

### 7.2 按钮

```tsx
// 变体
<Button variant="default">默认</Button>       // bg-primary
<Button variant="secondary">次级</Button>      // bg-secondary
<Button variant="outline">描边</Button>        // border-input bg-background
<Button variant="ghost">幽灵</Button>          // hover:bg-accent
<Button variant="destructive">危险</Button>    // bg-destructive

// 尺寸
<Button size="sm">小</Button>    // h-8 px-3 text-xs
<Button size="default">默认</Button>  // h-9 px-4
<Button size="lg">大</Button>    // h-10 px-8
<Button size="icon">图标</Button>  // h-9 w-9
```

### 7.3 输入框

```tsx
<Input className="h-9 rounded-md border border-input bg-transparent px-3 py-1 text-sm" />
```

### 7.4 卡片

```tsx
<Card className="rounded-xl border bg-card text-card-foreground shadow">
```

### 7.5 弹窗/对话框

```tsx
<DialogContent className="sm:rounded-lg shadow-lg">
```

---

## 8. 图标系统

### 8.1 图标来源

- **主要**：Lucide React (`lucide-react`)
- **辅助**：@lobehub/icons（AI 相关品牌图标）

### 8.2 图标尺寸

| 场景 | 尺寸 |
|------|------|
| 按钮内图标 | `w-3.5 h-3.5` |
| 列表项图标 | `w-3.5 h-3.5` |
| 步骤/状态图标 | `w-3 h-3` |
| 标题栏图标 | `w-4 h-4` |
| 大图标 | `w-5 h-5` 或 `w-6 h-6` |

---

## 9. 状态与反馈

### 9.1 交互状态

| 状态 | 样式 |
|------|------|
| Hover | `hover:bg-primary/90`、`hover:bg-secondary/80` |
| Focus | `focus-visible:ring-1 focus-visible:ring-ring` |
| Disabled | `disabled:opacity-40` 或 `disabled:opacity-50` |
| Active/Selected | `bg-primary/15 text-primary border-primary/25` |

### 9.2 加载状态

- 使用 `Loader2` 图标 + `animate-spin`
- 进度条使用自定义 `loading-bar` 动画

### 9.3 Toast 通知

- 使用 `sonner` 库（非 `@/hooks/use-toast`）
- 成功：`toast.success()`
- 错误：`toast.error()`
- 信息：`toast.info()`

---

## 10. 动效与过渡

### 10.1 过渡原则

- **默认过渡**：`transition-colors`（最常用）
- **时长**：`duration-200`（UI 状态变化）
- **缓动**：默认 ease，无需自定义

### 10.2 动画

| 动画 | 用途 |
|------|------|
| `animate-pulse` | 加载中指示器 |
| `animate-spin` | 旋转图标（加载） |
| `animate-in fade-in slide-in-from-bottom-1 duration-300` | 步骤卡片出现 |
| `animate-bounce` | 极少使用 |

---

## 11. 滚动条

自定义滚动条样式：

```css
scrollbar-width: thin;
scrollbar-color: hsl(var(--border)) transparent;

::-webkit-scrollbar { width: 6px; background: transparent; }
::-webkit-scrollbar-thumb { background: hsl(var(--border)); border-radius: 3px; }
```

---

## 12. 新增组件/页面检查清单

在新增任何 UI 元素前，请确认：

- [ ] 颜色使用 CSS 变量（`hsl(var(--primary))`），而非硬编码色值
- [ ] 文字大小符合字号层级（`text-sm` / `text-xs`）
- [ ] 圆角使用设计系统值（`rounded-md` / `rounded-lg` / `rounded-xl`）
- [ ] 边框使用 `border-border`
- [ ] 阴影使用 `shadow` 或 `shadow-sm`，无自定义大阴影
- [ ] 使用 `cn()` 合并 Tailwind 类名
- [ ] 按钮/输入框有明确的交互状态（hover/focus/disabled）
- [ ] 图标尺寸符合规范
- [ ] 间距使用 Tailwind 标准值（gap-2/4, space-y-4, p-6 等）
- [ ] 在深色和浅色模式下均测试通过

---

## 13. 文件位置速查

| 文件 | 用途 |
|------|------|
| `src/index.css` | CSS 变量定义、字体加载、全局样式、滚动条 |
| `tailwind.config.js` | Tailwind 主题扩展、颜色映射、动画、自定义工具类 |
| `components.json` | shadcn/ui 配置（New York 风格） |
| `src/components/ui/` | shadcn/ui 基础组件库 |
| `src/lib/utils.ts` | `cn()` 工具函数 |
| `src/App.tsx` | 主题色初始化逻辑 |
