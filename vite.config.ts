import { defineConfig } from 'vite-plus'

// Vite+ repository config. Lingguang keeps its official npm dependencies and
// quality gate; Vite+ remains an additional repository-level check and hook.
// TOML and Rust sources are handled by rustfmt/taplo.
export default defineConfig({
  run: {
    tasks: {
      'install:lingguang': {
        command: 'npm --prefix apps/lingguang ci',
        cache: false,
      },
      'check:lingguang': {
        command: 'npm --prefix apps/lingguang run check',
        cache: false,
        dependsOn: ['install:lingguang'],
      },
      'build:lingguang': {
        command: 'npm --prefix apps/lingguang run build',
        cache: false,
      },
      'dev:lingguang': {
        command: 'npm --prefix apps/lingguang run dev',
        cache: false,
      },
    },
  },
  staged: {
    '*': 'vp check --fix',
  },
  fmt: {
    singleQuote: true,
    semi: false,
    sortImports: true,
    sortPackageJson: true,
    ignorePatterns: ['crates/**', 'target/**', 'models/**', '**/*.toml', '*.toml'],
  },
  lint: {
    options: { typeAware: true, typeCheck: true },
  },
})
