# tnls — SSH tunnel manager

Desktop app (Tauri 2 + React + Rust) quản lý SSH tunnels: lưu profiles,
bật/tắt bằng UI, chạy nền qua system tray. Xem thiết kế: `docs/DESIGN.md`,
tiến độ: `SNAPSHOT`.

## Develop

```bash
npm install
npm run dev      # frontend only (mock IPC ngoài Tauri)
npm run tauri dev
```

## Build

```bash
npm run build
npm run tauri build   # cần MSVC Build Tools trên Windows để link
```

## Test

```bash
cargo test -p tnls-core          # 38 unit tests (domain, persistence, ssh, manager, health)
cargo clippy --workspace --all-targets
cargo fmt -- --check
npx tsc --noEmit
```

CI (`.github/workflows/ci.yml`) chạy toàn bộ gates trên
Windows/macOS/Ubuntu + frontend typecheck/build.
