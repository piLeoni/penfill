# penfill browser playground

Vite + React demo that loads `penfill-wasm` and previews SVG fills.

```sh
# from repo root
./scripts/build-wasm.sh
cd web && npm install && npm run dev
```

Open http://localhost:5173. WASM output lands in `web/pkg/` (generated; not committed).
