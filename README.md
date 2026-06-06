# DeepHarness Desktop

AI 编码助手桌面应用

## 介绍

基于 React + Tauri 的 AI 编码桌面应用，支持本地 SQLite 数据存储。

## 目录结构

```
├── README.md # 说明文档
├── components.json # 组件库配置
├── index.html # 入口文件
├── package.json # 包管理
├── postcss.config.js # postcss 配置
├── public # 静态资源目录
│   ├── favicon.png # 图标
│   └── images # 图片资源
├── src # 源码目录
│   ├── App.tsx # 入口文件
│   ├── components # 组件目录
│   ├── contexts # 上下文目录
│   ├── db # 数据库适配层
│   ├── hooks # 通用钩子函数目录
│   ├── index.css # 全局样式
│   ├── lib # 工具库目录
│   ├── main.tsx # 入口文件
│   ├── routes.tsx # 路由配置
│   ├── pages # 页面目录
│   ├── services  # 数据库交互目录
│   ├── types   # 类型定义目录
│   ├── src-tauri # Tauri 桌面端配置
├── tsconfig.app.json  # ts 前端配置文件
├── tsconfig.json # ts 配置文件
├── tsconfig.node.json # ts node端配置文件
└── vite.config.ts # vite 配置文件
```

## 技术栈

Vite、TypeScript、React、Tauri、SQLite

## 本地开发

### 环境要求

```
# Node.js ≥ 20
# npm ≥ 10
# Rust ≥ 1.70
```

### 安装步骤

```bash
# 安装前端依赖
npm i

# 开发模式（Web）
npm run dev

# Tauri 桌面端开发
npm run tauri-dev

# 构建桌面应用
npm run tauri-build
```

## 数据存储

应用使用本地 SQLite 数据库进行数据持久化，数据存储在用户目录下的应用数据文件夹中。
