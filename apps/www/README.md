# apps/www

The 404-snf frontend: a **Nuxt** application using **Web Bluetooth** to talk to
the device's BLE fatigue service.

> **Not scaffolded here.** The Nuxt app is scaffolded elsewhere and dropped into
> this directory. This repo intentionally does **not** generate it. This README
> is the placeholder that keeps the directory (and its Vite+ workspace slot)
> present.

## How it fits

- Listed in the root `pnpm-workspace.yaml` under `apps/*`, so `vp install` links
  it against the shared libraries in `packages/*`.
- Consumes [`@snf/protocol`](../../packages/protocol) for the BLE service UUIDs
  and the notify-payload decoder — the single source of truth for the wire
  format shared with `crates/ble`.

## When the Nuxt app is added

```bash
# from the repo root
vp install        # links @snf/protocol into apps/www
vp dev            # run the frontend dev server
```
