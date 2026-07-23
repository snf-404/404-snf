import { defineConfig } from 'vite-plus'

// Vite+ workspace config. Mirrors the consortium repo's setup: format/lint on
// staged files, no semicolons, single quotes, sorted imports. TOML and the Rust
// tree are ignored — those are handled by rustfmt/taplo.
export default defineConfig({
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
