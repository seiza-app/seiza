import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig } from 'eslint/config';
import vue from 'eslint-plugin-vue';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default defineConfig(
  {
    ignores: [
      '**/node_modules/',
      '**/dist/',
      '**/.output/',
      '**/.wxt/',
      '**/.nuxt/',
      '**/.nitro/',
      '**/.data/',
      'apps/desktop/src-tauri/',
      '.claude/',
    ],
  },
  js.configs.recommended,
  tseslint.configs.recommended,
  vue.configs['flat/recommended'],
  {
    files: ['**/*.vue'],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
  },
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: {
      // TypeScript already reports undefined names, and knows about WXT's and Nuxt's auto-imports.
      'no-undef': 'off',
      // A leading underscore marks a value that's deliberately unused, e.g. one left out of a rest spread.
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrors: 'none', ignoreRestSiblings: true },
      ],
      // Entry points are named App.vue by convention.
      'vue/multi-word-component-names': 'off',
    },
  },
  // Formatting is Prettier's job; this turns off every rule that would fight it.
  prettier,
);
