/** @type {import('eslint').Linter.Config} */
module.exports = {
  root: true,
  env: {
    browser: true,
    esnext: true,
    node: true
  },
  extends: [
    'eslint:recommended',
    'standard',
    'prettier/@typescript-eslint',
    'plugin:svelte/recommended',
    'plugin:@typescript-eslint/recommended',
    'plugin:import/recommended',
    'plugin:prettier/recommended',
    'prettier'
  ],
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
    project: 'src/ic_panda_frontend/tsconfig.json',
    extraFileExtensions: []
  },
  overrides: [
    {
      files: ['*.svelte'],
      parser: 'svelte-eslint-parser',
      parserOptions: {
        parser: '@typescript-eslint/parser'
      }
    }
  ],
  plugins: ['@typescript-eslint', 'svelte', 'import', 'prettier'],
  rules: {
    '@typescript-eslint/consistent-type-exports': [
      'error',
      { fixMixedExportsWithInlineTypeSpecifier: true }
    ],
    '@typescript-eslint/consistent-type-imports': [
      'error',
      { fixStyle: 'inline-type-imports' }
    ],
    '@typescript-eslint/no-empty-function': 'off',
    '@typescript-eslint/no-empty-interface': 'off',
    '@typescript-eslint/no-unused-vars': 'off',
    'import/named': 'off',
    'import/newline-after-import': 'error',
    'import/no-unresolved': 'off',
    'import/order': [
      'error',
      {
        groups: [
          ['builtin', 'external', 'internal'],
          'parent',
          ['sibling', 'index']
        ],
        'newlines-between': 'never',
        alphabetize: { order: 'ignore' }
      }
    ],
    'no-console': 'warn',
    'no-restricted-imports': [
      'error',
      {
        'paths': []
      }
    ],
    'no-useless-rename': 'error',
    'object-shorthand': ['error', 'always']
  },
  settings: {
    'import/internal-regex': '^#'
  },
  ignorePatterns: ['dist', 'node_modules', 'examples', 'scripts']
}
