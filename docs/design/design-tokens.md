# Design Tokens: <project-name>

> 令牌规格。原则：Token 即代码。本文件是决策记录；代码中的 token 定义（CSS 变量 / 主题配置）才是真相。
> 组件禁止硬编码颜色、圆角、间距；全部引用 token。

## 色彩（明暗双模）

| Token | 深色 | 浅色 | 用途 |
|---|---|---|---|
| canvas |  |  | 窗口底色 |
| surface |  |  | 面板/卡片 |
| surface-2 |  |  | 输入/代码块底 |
| fg / fg-2 / fg-3 |  |  | 文字三级 |
| border / border-strong |  |  | 边框 |
| accent / accent-fg |  |  | 强调色 |
| success / warning / danger |  |  | 语义色 |

## 圆角

| 档位 | 值 | 应用 |
|---|---|---|
| 窗口 |  | 主窗口/浮层 |
| 控件 |  | 按钮/输入 |
| 徽标 |  | Badge/Kbd/Key |
| 胶囊 | 9999px | 标签 |

## 字体

- UI：<字体 + 字重>（必须随包发货，禁止声明未打包字体）
- Mono：<字体 + 字重>

## 阴影

- 窗口阴影：
- 悬浮阴影：
- （玻璃类主题补充：内发光描边、高光叠加）

## 间距与动效

- 间距刻度：
- 动效时长与缓动：入场 / hover / 主题切换 / prefers-reduced-motion 降级
