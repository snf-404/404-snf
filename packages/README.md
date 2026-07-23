# packages/

JavaScript / TypeScript libraries for 404-snf, managed by **Vite+** (`vp`, which
sits on top of pnpm workspaces — see the root `pnpm-workspace.yaml` and
`vite.config.ts`).

## Members

| Package | Purpose |
| --- | --- |
| [`@snf/protocol`](protocol) | Shared TS definitions of the BLE fatigue service (UUIDs + payload decoder), mirroring `crates/ble`. Consumed by `apps/www`. |

## Common commands

```bash
vp install        # install/link all workspace deps (packages/* + apps/*)
vp pack           # build the libraries here
vp check          # format + lint + typecheck
vp add <pkg>      # add a dependency
```

New libraries go in their own `packages/<name>/` directory with a `package.json`
(`vp pack` builds them); the workspace picks them up via `packages/*`.
