// ESLint flat config（ESLint 10 + ESM），面向 Vue 3 + TypeScript。
//
// 设计原则：务实优先。脚手架已有一批存量代码，规则以"能捕获真实问题"为准，
// 不为纯风格问题制造噪音（风格交给 Prettier）；若某条规则在存量代码上大面积报错，
// 就近降级为 warn 或关闭，并在下方注明原因。
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import pluginVue from "eslint-plugin-vue";
import globals from "globals";
import configPrettier from "eslint-config-prettier/flat";

export default tseslint.config(
  {
    // 构建产物 / 依赖 / Rust 目标目录 / 生成代码 / 签名密钥，一律不检查
    ignores: [
      "**/dist/**",
      "**/dist-web-static/**",
      "**/node_modules/**",
      "**/target/**",
      "**/vendor/**",
      "src-tauri/gen/**",
      ".signing/**",
    ],
  },
  {
    // 检查范围：前端源码 + Node 脚本
    files: ["apps/desktop/src/**/*.{ts,vue}", "scripts/**/*.mjs"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
  ...pluginVue.configs["flat/recommended"],
  {
    // .vue 的 <script> 用 TS 解析
    files: ["**/*.vue"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: [".vue"],
      },
    },
  },
  {
    // 类型声明文件（如 vite-env.d.ts 的 DefineComponent<{}, {}, any> 样板）里的
    // 空对象类型是框架约定写法，关闭该规则避免误报。
    files: ["**/*.d.ts"],
    rules: {
      "@typescript-eslint/no-empty-object-type": "off",
    },
  },
  {
    files: ["apps/desktop/src/**/*.{ts,vue}", "scripts/**/*.mjs"],
    rules: {
      // TS 已负责标识符检查，no-undef 在 TS 里误报率高（类型位置、全局类型等）
      "no-undef": "off",
      // 存量代码里有少量显式 any（如未建模的 IPC 返回值），保持现状不阻塞
      "@typescript-eslint/no-explicit-any": "off",
      // 允许用下划线前缀标记"故意不使用"的变量/参数
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],
      // 脚手架里 App.vue 等单字组件名是合理约定，关闭该命名规则
      "vue/multi-word-component-names": "off",
      // 存量代码存在 `<template>` 属性顺序等纯风格差异，降级为 warn 不阻塞 lint 退出码
      "vue/attributes-order": "warn",
      "vue/block-order": "warn",
      // 存量组件里有 `<template v-else class="...">`（class 对 template 无效但无害），
      // 属既有写法，降级为 warn 由开发者按需清理。
      "vue/no-useless-template-attributes": "warn",
    },
  },
  // 必须放在最后：关闭所有与 Prettier 冲突的格式类规则
  configPrettier,
);
